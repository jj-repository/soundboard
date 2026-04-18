#[cfg(target_os = "linux")]
use crate::{
    types::pipewire::{AudioDevice, DeviceType, Terminate},
    utils::{
        pipewire::{create_link, get_all_devices, get_device},
    },
};
use crate::utils::daemon::get_daemon_config;
use rodio::{cpal, Decoder, Player, Source};
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
#[cfg(target_os = "windows")]
use rodio::cpal::traits::StreamTrait;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
#[cfg(target_os = "windows")]
use std::{collections::VecDeque, sync::Arc};

#[derive(Debug, Eq, PartialEq, Default, Clone, Copy, Serialize, Deserialize)]
pub enum PlayerState {
    #[default]
    Stopped,
    Paused,
    Playing,
}

/// Get all available output devices
pub fn get_output_devices() -> HashMap<String, String> {
    let mut devices = HashMap::new();
    let host = cpal::default_host();

    if let Ok(output_devices) = host.output_devices() {
        for device in output_devices {
            if let Ok(desc) = device.description() {
                let name = desc.name().to_string();
                devices.insert(name.clone(), name);
            }
        }
    }

    devices
}

/// Get the default output device name
pub fn get_default_output_device() -> Option<String> {
    let host = cpal::default_host();
    host.default_output_device()
        .and_then(|d| d.description().ok())
        .map(|desc| desc.name().to_string())
}

/// Get all available input devices (for mic selection on Windows)
#[cfg(target_os = "windows")]
pub fn get_input_devices() -> HashMap<String, String> {
    let mut devices = HashMap::new();
    let host = cpal::default_host();
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(desc) = device.description() {
                let name = desc.name().to_string();
                devices.insert(name.clone(), name);
            }
        }
    }
    devices
}

/// Represents a single audio layer that can play sounds independently
pub struct AudioLayer {
    pub sink: Player,
    pub volume: f32,
    pub current_file_path: Option<PathBuf>,
    pub duration: Option<f32>,
}

impl AudioLayer {
    pub fn new(mixer: &rodio::mixer::Mixer) -> Self {
        let sink = Player::connect_new(mixer);
        sink.set_volume(1.0);
        Self {
            sink,
            volume: 1.0,
            current_file_path: None,
            duration: None,
        }
    }

    pub fn is_playing(&self) -> bool {
        !self.sink.empty() && !self.sink.is_paused()
    }

    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }
}

/// Number of audio layers available for mixing
pub const NUM_AUDIO_LAYERS: usize = 4;

/// Maximum gain multiplier for main audio output (5x = +14dB)
pub const MAX_GAIN: f32 = 5.0;
/// Minimum gain multiplier for main audio output
pub const MIN_GAIN: f32 = 0.0;

/// Maximum mic gain multiplier (3x = +9.5dB)
pub const MAX_MIC_GAIN: f32 = 3.0;
/// Minimum mic gain multiplier (0.5x = -6dB, prevents complete silence)
pub const MIN_MIC_GAIN: f32 = 0.5;

/// Shared ring buffer for mic audio samples (Windows mic passthrough)
#[cfg(target_os = "windows")]
struct MicBuffer {
    samples: std::sync::Mutex<VecDeque<f32>>,
    sample_rate: u32,
    channels: u16,
}

/// Custom rodio Source that reads captured mic audio from a shared buffer
#[cfg(target_os = "windows")]
struct MicSource {
    buffer: Arc<MicBuffer>,
}

#[cfg(target_os = "windows")]
impl Iterator for MicSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Ok(mut buf) = self.buffer.samples.lock() {
            Some(buf.pop_front().unwrap_or(0.0))
        } else {
            Some(0.0) // Return silence on lock failure
        }
    }
}

#[cfg(target_os = "windows")]
impl Source for MicSource {
    fn current_span_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { self.buffer.channels }
    fn sample_rate(&self) -> u32 { self.buffer.sample_rate }
    fn total_duration(&self) -> Option<Duration> { None }
}

pub struct AudioPlayer {
    _stream_handle: MixerDeviceSink,
    sink: Player, // Main player for primary playback
    layers: Vec<AudioLayer>, // Additional layers for mixing

    #[cfg(target_os = "linux")]
    input_link_sender: Option<pipewire::channel::Sender<Terminate>>,
    #[cfg(target_os = "linux")]
    pub current_input_device: Option<AudioDevice>,

    #[cfg(target_os = "windows")]
    pub current_input_device: Option<String>,
    #[cfg(target_os = "windows")]
    mic_stop_sender: Option<std::sync::mpsc::Sender<()>>,
    #[cfg(target_os = "windows")]
    mic_sink: Player,

    pub current_output_device: Option<String>,

    pub volume: f32,
    pub gain: f32,
    pub mic_gain: f32,
    pub duration: Option<f32>,

    pub current_file_path: Option<PathBuf>,

    pub looped: bool,
}

impl AudioPlayer {
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        let daemon_config = get_daemon_config();
        let default_volume = daemon_config.default_volume.unwrap_or(1.0);
        let default_gain = daemon_config.default_gain.unwrap_or(1.0);
        let default_mic_gain = daemon_config.default_mic_gain.unwrap_or(1.0);

        #[cfg(target_os = "linux")]
        let default_input_device = {
            let mut device: Option<AudioDevice> = None;
            if let Some(name) = daemon_config.default_input_name {
                if let Ok(d) = get_device(&name).await {
                    if d.device_type == DeviceType::Input {
                        device = Some(d);
                    }
                }
            }
            device
        };

        #[cfg(target_os = "windows")]
        let default_input_name = daemon_config.default_input_name;

        // Try to use configured output device, fall back to default
        let (stream_handle, current_output_device) =
            if let Some(ref output_name) = daemon_config.default_output_name {
                match Self::create_stream_for_device(output_name) {
                    Ok(stream) => (stream, Some(output_name.clone())),
                    Err(_) => {
                        eprintln!(
                            "Failed to use output device '{}', falling back to default",
                            output_name
                        );
                        (
                            DeviceSinkBuilder::open_default_sink()?,
                            get_default_output_device(),
                        )
                    }
                }
            } else {
                // On Windows, try to auto-detect VB-Audio Virtual Cable
                #[cfg(target_os = "windows")]
                {
                    let vb_cable = Self::find_virtual_cable();
                    if let Some(ref cable_name) = vb_cable {
                        match Self::create_stream_for_device(cable_name) {
                            Ok(stream) => (stream, Some(cable_name.clone())),
                            Err(_) => (
                                DeviceSinkBuilder::open_default_sink()?,
                                get_default_output_device(),
                            ),
                        }
                    } else {
                        (
                            DeviceSinkBuilder::open_default_sink()?,
                            get_default_output_device(),
                        )
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    (
                        DeviceSinkBuilder::open_default_sink()?,
                        get_default_output_device(),
                    )
                }
            };

        let mixer: &rodio::mixer::Mixer = stream_handle.mixer();
        let sink = Player::connect_new(mixer);
        sink.set_volume(default_volume * default_gain);

        // Initialize audio layers for mixing
        let mut layers = Vec::with_capacity(NUM_AUDIO_LAYERS);
        for _ in 0..NUM_AUDIO_LAYERS {
            layers.push(AudioLayer::new(mixer));
        }

        // Windows: create dedicated sink for mic passthrough audio
        #[cfg(target_os = "windows")]
        let mic_sink = {
            let s = Player::connect_new(mixer);
            s.stop();
            s
        };

        #[cfg(target_os = "linux")]
        let has_input_device = default_input_device.is_some();

        let mut audio_player = AudioPlayer {
            _stream_handle: stream_handle,
            sink,
            layers,

            #[cfg(target_os = "linux")]
            input_link_sender: None,
            #[cfg(target_os = "linux")]
            current_input_device: default_input_device,

            #[cfg(target_os = "windows")]
            current_input_device: default_input_name,
            #[cfg(target_os = "windows")]
            mic_stop_sender: None,
            #[cfg(target_os = "windows")]
            mic_sink,

            current_output_device,

            volume: default_volume,
            gain: default_gain,
            mic_gain: default_mic_gain,
            duration: None,

            current_file_path: None,

            looped: false,
        };

        #[cfg(target_os = "linux")]
        if has_input_device {
            audio_player.link_devices().await?;
            audio_player.apply_mic_gain();
        }

        #[cfg(target_os = "windows")]
        if audio_player.current_input_device.is_some() {
            if let Err(e) = audio_player.start_mic_passthrough() {
                eprintln!("Failed to start mic passthrough: {}", e);
            }
            audio_player.apply_mic_gain();
        }

        Ok(audio_player)
    }

    fn create_stream_for_device(device_name: &str) -> Result<MixerDeviceSink, Box<dyn Error>> {
        let host = cpal::default_host();
        let devices = host.output_devices()?;

        for device in devices {
            if let Ok(desc) = device.description() {
                if desc.name() == device_name {
                    return Ok(DeviceSinkBuilder::from_device(device)?.open_sink_or_fallback()?);
                }
            }
        }

        Err(format!("Output device '{}' not found", device_name).into())
    }

    /// On Windows, try to find VB-Audio Virtual Cable output device
    #[cfg(target_os = "windows")]
    fn find_virtual_cable() -> Option<String> {
        let host = cpal::default_host();
        if let Ok(devices) = host.output_devices() {
            for device in devices {
                if let Ok(desc) = device.description() {
                    let name = desc.name().to_string();
                    let lower = name.to_lowercase();
                    if lower.contains("cable input") || lower.contains("vb-audio") {
                        println!("Auto-detected VB-Audio Virtual Cable: {}", name);
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    pub fn get_current_output_device(&self) -> Option<&String> {
        self.current_output_device.as_ref()
    }

    pub fn get_all_output_devices(&self) -> HashMap<String, String> {
        get_output_devices()
    }

    #[cfg(target_os = "linux")]
    fn abort_link_thread(&mut self) {
        if let Some(sender) = &self.input_link_sender {
            if sender.send(Terminate {}).is_err() {
                eprintln!("Failed to send terminate signal to link thread");
            }
        }
    }

    #[cfg(target_os = "linux")]
    async fn link_devices(&mut self) -> Result<(), Box<dyn Error>> {
        self.abort_link_thread();

        let current_input_name = match &self.current_input_device {
            Some(device) => device.name.clone(),
            None => {
                println!("No input device selected, skipping device linking");
                return Ok(());
            }
        };

        // Retry up to 5 times with 100ms delay to handle PipeWire race conditions
        const MAX_RETRIES: u32 = 5;
        const RETRY_DELAY_MS: u64 = 100;

        for attempt in 1..=MAX_RETRIES {
            let (input_devices, _) = get_all_devices().await?;

            // Find the virtual mic
            let pwsp_daemon_input = match input_devices
                .iter()
                .find(|d| d.name == crate::VIRTUAL_MIC_NAME)
                .cloned()
            {
                Some(device) => device,
                None => {
                    if attempt == MAX_RETRIES {
                        println!("Could not find pwsp-virtual-mic after {} attempts, skipping device linking", MAX_RETRIES);
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    continue;
                }
            };

            // Re-fetch the current input device to get updated port info
            let current_input_device = match input_devices
                .iter()
                .find(|d| d.name == current_input_name)
                .cloned()
            {
                Some(device) => device,
                None => {
                    if attempt == MAX_RETRIES {
                        println!("Could not find input device '{}' after {} attempts, skipping device linking", current_input_name, MAX_RETRIES);
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    continue;
                }
            };

            // Check if all required ports are available
            match (
                &current_input_device.output_fl,
                &current_input_device.output_fr,
                &pwsp_daemon_input.input_fl,
                &pwsp_daemon_input.input_fr,
            ) {
                (Some(output_fl), Some(output_fr), Some(input_fl), Some(input_fr)) => {
                    // All ports available, create the link
                    self.input_link_sender = Some(create_link(
                        output_fl.clone(),
                        output_fr.clone(),
                        input_fl.clone(),
                        input_fr.clone(),
                    )?);
                    return Ok(());
                }
                (out_fl, out_fr, in_fl, in_fr) => {
                    if attempt == MAX_RETRIES {
                        println!(
                            "Ports not available after {} attempts (output_fl: {}, output_fr: {}, input_fl: {}, input_fr: {}), skipping device linking",
                            MAX_RETRIES,
                            out_fl.is_some(),
                            out_fr.is_some(),
                            in_fl.is_some(),
                            in_fr.is_some()
                        );
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    continue;
                }
            }
        }

        Ok(())
    }

    // On Windows, link_devices is a no-op (routing is via output device selection)
    #[cfg(target_os = "windows")]
    async fn link_devices(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    pub fn pause(&mut self) {
        if self.get_state() == PlayerState::Playing {
            self.sink.pause();
        }
    }

    pub fn resume(&mut self) {
        if self.get_state() == PlayerState::Paused {
            self.sink.play();
        }
    }

    pub fn stop(&mut self) {
        self.sink.stop();
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    pub fn get_state(&self) -> PlayerState {
        if self.sink.len() == 0 {
            return PlayerState::Stopped;
        }

        if self.sink.is_paused() {
            return PlayerState::Paused;
        }

        PlayerState::Playing
    }

    fn update_sink_volume(&self) {
        self.sink.set_volume(self.volume * self.gain);
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.update_sink_volume();
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(MIN_GAIN, MAX_GAIN);
        self.update_sink_volume();
    }

    pub fn get_gain(&self) -> f32 {
        self.gain
    }

    #[cfg(target_os = "linux")]
    fn apply_mic_gain(&self) {
        if let Some(device) = &self.current_input_device {
            // Use wpctl to set the source volume
            // Safety: device.id is u32 and mic_gain is f32 (clamped to MIN_MIC_GAIN-MAX_MIC_GAIN),
            // so no shell injection is possible. Command::args() also bypasses shell.
            let id_str = device.id.to_string();
            let gain_str = format!("{:.2}", self.mic_gain);

            match std::process::Command::new("wpctl")
                .args(["set-volume", &id_str, &gain_str])
                .output()
            {
                Ok(output) => {
                    if !output.status.success() {
                        eprintln!(
                            "wpctl set-volume failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Failed to execute wpctl: {}", e);
                }
            }
        }
    }

    // On Windows, apply mic gain by adjusting the mic passthrough sink volume
    #[cfg(target_os = "windows")]
    fn apply_mic_gain(&self) {
        self.mic_sink.set_volume(self.mic_gain);
    }

    pub fn set_mic_gain(&mut self, mic_gain: f32) {
        self.mic_gain = mic_gain.clamp(MIN_MIC_GAIN, MAX_MIC_GAIN);
        self.apply_mic_gain();
    }

    pub fn get_mic_gain(&self) -> f32 {
        self.mic_gain
    }

    pub fn get_position(&self) -> f32 {
        if self.get_state() == PlayerState::Stopped {
            return 0.0;
        }

        self.sink.get_pos().as_secs_f32()
    }

    pub fn seek(&mut self, mut position: f32) -> Result<(), Box<dyn Error>> {
        if position < 0.0 {
            position = 0.0;
        }

        self.sink
            .try_seek(Duration::from_secs_f32(position))
            .map_err(|e| -> Box<dyn Error> { Box::new(e) })
    }

    pub fn get_duration(&mut self) -> Result<f32, Box<dyn Error>> {
        if self.get_state() == PlayerState::Stopped {
            Err("Nothing is playing right now".into())
        } else {
            match self.duration {
                Some(duration) => Ok(duration),
                None => Err("Couldn't determine duration for current file".into()),
            }
        }
    }

    pub async fn play(&mut self, file_path: &Path) -> Result<(), Box<dyn Error>> {
        if !file_path.exists() {
            return Err(format!("File does not exist: {}", file_path.display()).into());
        }

        let file = fs::File::open(file_path)?;
        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");

        match Decoder::try_from(file) {
            Ok(source) => {
                self.current_file_path = Some(file_path.to_path_buf());

                if let Some(duration) = source.total_duration() {
                    self.duration = Some(duration.as_secs_f32());
                } else {
                    self.duration = None;
                }

                self.sink.stop();
                self.sink.append(source);
                self.sink.play();
                self.link_devices().await?;

                Ok(())
            }
            Err(err) => Err(format!(
                "Failed to decode '{}' (format: {}): {}",
                file_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
                extension,
                err
            ).into()),
        }
    }

    /// Preview audio through speakers only (not through virtual mic)
    pub fn preview(&mut self, file_path: &Path) -> Result<(), Box<dyn Error>> {
        if !file_path.exists() {
            return Err(format!("File does not exist: {}", file_path.display()).into());
        }

        let file = fs::File::open(file_path)?;
        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");

        match Decoder::try_from(file) {
            Ok(source) => {
                self.current_file_path = Some(file_path.to_path_buf());

                if let Some(duration) = source.total_duration() {
                    self.duration = Some(duration.as_secs_f32());
                } else {
                    self.duration = None;
                }

                // Stop current playback and abort virtual mic link
                self.sink.stop();
                #[cfg(target_os = "linux")]
                self.abort_link_thread();

                self.sink.append(source);
                self.sink.play();
                // Note: We do NOT call link_devices() here - audio goes to speakers only

                Ok(())
            }
            Err(err) => Err(format!(
                "Failed to decode '{}' (format: {}): {}",
                file_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
                extension,
                err
            ).into()),
        }
    }

    pub fn get_current_file_path(&mut self) -> &Option<PathBuf> {
        if self.get_state() == PlayerState::Stopped && !self.looped {
            self.current_file_path = None;
        }
        &self.current_file_path
    }

    #[cfg(target_os = "linux")]
    pub async fn set_current_input_device(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        let input_device = get_device(name).await?;

        if input_device.device_type != DeviceType::Input {
            return Err("Selected device is not an input device".into());
        }

        self.current_input_device = Some(input_device);

        self.link_devices().await?;

        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub async fn set_current_input_device(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        self.current_input_device = Some(name.to_string());
        self.start_mic_passthrough()?;
        self.apply_mic_gain();
        Ok(())
    }

    /// Stop the current mic capture thread and clear the mic sink
    #[cfg(target_os = "windows")]
    fn stop_mic_passthrough(&mut self) {
        if let Some(sender) = self.mic_stop_sender.take() {
            sender.send(()).ok();
        }
        self.mic_sink.stop();
    }

    /// Start capturing from the selected mic and routing audio through the output mixer
    #[cfg(target_os = "windows")]
    fn start_mic_passthrough(&mut self) -> Result<(), Box<dyn Error>> {
        self.stop_mic_passthrough();

        let device_name = match &self.current_input_device {
            Some(name) => name.clone(),
            None => return Ok(()),
        };

        // Find the CPAL input device
        let host = cpal::default_host();
        let device = host.input_devices()?
            .find(|d| d.description().map(|desc| desc.name() == device_name).unwrap_or(false))
            .ok_or_else(|| format!("Input device '{}' not found", device_name))?;

        let supported_config = device.default_input_config()?;
        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();
        let sample_format = supported_config.sample_format();
        let stream_config: cpal::StreamConfig = supported_config.into();

        let buffer = Arc::new(MicBuffer {
            samples: std::sync::Mutex::new(VecDeque::with_capacity(sample_rate as usize)),
            sample_rate,
            channels,
        });

        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let buffer_clone = buffer.clone();
        // Cap buffer at ~500ms to prevent latency buildup
        let max_samples = (sample_rate as usize * channels as usize) / 2;

        // Spawn capture thread (cpal::Stream may not be Send on all platforms)
        std::thread::spawn(move || {
            let push_f32 = {
                let buf = buffer_clone.clone();
                move |data: &[f32]| {
                    if let Ok(mut b) = buf.samples.lock() {
                        while b.len() > max_samples {
                            b.pop_front();
                        }
                        b.extend(data);
                    }
                }
            };

            let stream_result = match sample_format {
                cpal::SampleFormat::F32 => {
                    let push = push_f32;
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[f32], _: &cpal::InputCallbackInfo| { push(data); },
                        |err| eprintln!("Mic input error: {}", err),
                        None,
                    )
                }
                cpal::SampleFormat::I16 => {
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            let f32_data: Vec<f32> = data.iter()
                                .map(|&s| s as f32 / 32768.0)
                                .collect();
                            if let Ok(mut b) = buffer_clone.samples.lock() {
                                while b.len() > max_samples {
                                    b.pop_front();
                                }
                                b.extend(f32_data);
                            }
                        },
                        |err| eprintln!("Mic input error: {}", err),
                        None,
                    )
                }
                format => {
                    eprintln!("Unsupported mic sample format: {:?}", format);
                    return;
                }
            };

            match stream_result {
                Ok(stream) => {
                    if let Err(e) = stream.play() {
                        eprintln!("Failed to start mic stream: {}", e);
                        return;
                    }
                    // Keep stream alive until stop signal
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    eprintln!("Failed to build mic input stream: {}", e);
                }
            }
        });

        // Connect MicSource to the mic sink on the output mixer
        let mic_source = MicSource { buffer };
        self.mic_sink.stop();
        self.mic_sink.append(mic_source);
        self.mic_sink.play();

        self.mic_stop_sender = Some(stop_tx);
        println!("Mic passthrough started: {}", device_name);
        Ok(())
    }

    // ============= Layer Management Methods =============

    /// Get the number of available layers
    pub fn get_layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Play a sound on a specific layer
    pub async fn play_on_layer(&mut self, layer_index: usize, file_path: &Path) -> Result<(), Box<dyn Error>> {
        if layer_index >= self.layers.len() {
            return Err(format!("Invalid layer index: {}", layer_index).into());
        }

        if !file_path.exists() {
            return Err(format!("File does not exist: {}", file_path.display()).into());
        }

        let file = fs::File::open(file_path)?;
        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");

        match Decoder::try_from(file) {
            Ok(source) => {
                let layer = &mut self.layers[layer_index];
                layer.current_file_path = Some(file_path.to_path_buf());

                if let Some(duration) = source.total_duration() {
                    layer.duration = Some(duration.as_secs_f32());
                } else {
                    layer.duration = None;
                }

                layer.sink.stop();
                layer.sink.append(source);
                layer.sink.play();

                // Ensure devices are linked for virtual mic output
                self.link_devices().await?;

                Ok(())
            }
            Err(err) => Err(format!(
                "Failed to decode '{}' (format: {}) on layer {}: {}",
                file_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
                extension,
                layer_index,
                err
            ).into()),
        }
    }

    /// Stop playback on a specific layer
    pub fn stop_layer(&mut self, layer_index: usize) -> Result<(), Box<dyn Error>> {
        if layer_index >= self.layers.len() {
            return Err(format!("Invalid layer index: {}", layer_index).into());
        }

        self.layers[layer_index].sink.stop();
        self.layers[layer_index].current_file_path = None;
        self.layers[layer_index].duration = None;
        Ok(())
    }

    /// Stop all layers
    pub fn stop_all_layers(&mut self) {
        for layer in &mut self.layers {
            layer.sink.stop();
            layer.current_file_path = None;
            layer.duration = None;
        }
    }

    /// Pause a specific layer
    pub fn pause_layer(&mut self, layer_index: usize) -> Result<(), Box<dyn Error>> {
        if layer_index >= self.layers.len() {
            return Err(format!("Invalid layer index: {}", layer_index).into());
        }

        self.layers[layer_index].sink.pause();
        Ok(())
    }

    /// Resume a specific layer
    pub fn resume_layer(&mut self, layer_index: usize) -> Result<(), Box<dyn Error>> {
        if layer_index >= self.layers.len() {
            return Err(format!("Invalid layer index: {}", layer_index).into());
        }

        self.layers[layer_index].sink.play();
        Ok(())
    }

    /// Set volume for a specific layer (0.0 to 1.0)
    pub fn set_layer_volume(&mut self, layer_index: usize, volume: f32) -> Result<(), Box<dyn Error>> {
        if layer_index >= self.layers.len() {
            return Err(format!("Invalid layer index: {}", layer_index).into());
        }

        let layer = &mut self.layers[layer_index];
        layer.volume = volume.clamp(0.0, 1.0);
        layer.sink.set_volume(layer.volume * self.gain);
        Ok(())
    }

    /// Get volume for a specific layer
    pub fn get_layer_volume(&self, layer_index: usize) -> Result<f32, Box<dyn Error>> {
        if layer_index >= self.layers.len() {
            return Err(format!("Invalid layer index: {}", layer_index).into());
        }

        Ok(self.layers[layer_index].volume)
    }

    /// Check if a layer is playing
    pub fn is_layer_playing(&self, layer_index: usize) -> bool {
        if layer_index >= self.layers.len() {
            return false;
        }

        self.layers[layer_index].is_playing()
    }

    /// Get layer state information
    pub fn get_layer_info(&self, layer_index: usize) -> Option<LayerInfo> {
        if layer_index >= self.layers.len() {
            return None;
        }

        let layer = &self.layers[layer_index];
        Some(LayerInfo {
            index: layer_index,
            is_playing: layer.is_playing(),
            is_paused: layer.sink.is_paused(),
            is_empty: layer.is_empty(),
            volume: layer.volume,
            current_file: layer.current_file_path.clone(),
            position: layer.sink.get_pos().as_secs_f32(),
            duration: layer.duration,
        })
    }

    /// Get all layers info
    pub fn get_all_layers_info(&self) -> Vec<LayerInfo> {
        (0..self.layers.len())
            .filter_map(|i| self.get_layer_info(i))
            .collect()
    }
}

/// Information about an audio layer
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayerInfo {
    pub index: usize,
    pub is_playing: bool,
    pub is_paused: bool,
    pub is_empty: bool,
    pub volume: f32,
    pub current_file: Option<PathBuf>,
    pub position: f32,
    pub duration: Option<f32>,
}
