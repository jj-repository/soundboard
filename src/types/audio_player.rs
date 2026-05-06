use crate::error::{Result, SoundboardError};
use crate::utils::daemon::get_daemon_config;
use crate::utils::path::has_supported_audio_extension;
use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
use rodio::{cpal, Decoder, Player, Source};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

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

/// Get all available input devices (for mic selection)
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

/// Defense-in-depth validation for paths handed to the player. The IPC parser
/// already runs `validate_audio_path`, but the player API can be reached from
/// anywhere in the binary (looped playback, future GUI direct hooks), so
/// re-check here.
fn validate_audio_file(file_path: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(file_path)
        .map_err(|_| SoundboardError::FileNotFound(file_path.to_path_buf()))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(SoundboardError::PathNotAllowed(file_path.to_path_buf()));
    }
    if !has_supported_audio_extension(file_path) {
        return Err(SoundboardError::PathNotAllowed(file_path.to_path_buf()));
    }
    Ok(())
}

/// Maximum gain multiplier for main audio output (5x = +14dB)
pub const MAX_GAIN: f32 = 5.0;
/// Minimum gain multiplier for main audio output
pub const MIN_GAIN: f32 = 0.0;

/// Maximum mic gain multiplier (3x = +9.5dB)
pub const MAX_MIC_GAIN: f32 = 3.0;
/// Minimum mic gain multiplier (0.5x = -6dB, prevents complete silence)
pub const MIN_MIC_GAIN: f32 = 0.5;

/// Pure clamp helpers so gain/volume bounds can be unit-tested without
/// constructing an `AudioPlayer` (which needs an audio device).
/// `f32::clamp` propagates NaN, which would feed NaN samples to rodio; we
/// substitute the lower bound so the failure mode is silence.
pub fn clamp_volume(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}
pub fn clamp_gain(g: f32) -> f32 {
    if g.is_nan() {
        MIN_GAIN
    } else {
        g.clamp(MIN_GAIN, MAX_GAIN)
    }
}
pub fn clamp_mic_gain(g: f32) -> f32 {
    if g.is_nan() {
        MIN_MIC_GAIN
    } else {
        g.clamp(MIN_MIC_GAIN, MAX_MIC_GAIN)
    }
}
pub fn clamp_layer_volume(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

/// Lock-free SPSC ring buffer wiring for the mic passthrough.
/// The cpal input callback pushes samples; the rodio output thread pops
/// them. Dropping the per-sample Mutex avoids priority inversion and the
/// per-sample lock acquisition at 48 kHz.
use ringbuf::{
    traits::{Consumer as ConsumerTrait, Producer as ProducerTrait, Split},
    HeapRb,
};

type MicConsumer = <HeapRb<f32> as Split>::Cons;

/// Custom rodio Source that reads captured mic audio from a lock-free ring.
struct MicSource {
    consumer: MicConsumer,
    sample_rate: std::num::NonZero<u32>,
    channels: std::num::NonZero<u16>,
}

impl Iterator for MicSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        // Return silence on underflow rather than stopping the source.
        Some(self.consumer.try_pop().unwrap_or(0.0))
    }
}

impl Source for MicSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> std::num::NonZero<u16> {
        self.channels
    }
    fn sample_rate(&self) -> std::num::NonZero<u32> {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct AudioPlayer {
    _stream_handle: MixerDeviceSink,
    sink: Player,
    layers: Vec<AudioLayer>,

    pub current_input_device: Option<String>,
    mic_stop_sender: Option<std::sync::mpsc::Sender<()>>,
    mic_thread: Option<std::thread::JoinHandle<()>>,
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
    pub async fn new() -> Result<Self> {
        let daemon_config = get_daemon_config();
        let default_volume = daemon_config.default_volume.unwrap_or(1.0);
        let default_gain = daemon_config.default_gain.unwrap_or(1.0);
        let default_mic_gain = daemon_config.default_mic_gain.unwrap_or(1.0);
        let default_input_name = daemon_config.default_input_name;

        let (stream_handle, current_output_device) =
            if let Some(ref output_name) = daemon_config.default_output_name {
                match Self::create_stream_for_device(output_name) {
                    Ok(stream) => (stream, Some(output_name.clone())),
                    Err(_) => {
                        tracing::error!(
                            "Failed to use output device '{}', falling back to default",
                            output_name
                        );
                        (
                            DeviceSinkBuilder::open_default_sink()
                                .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?,
                            get_default_output_device(),
                        )
                    }
                }
            } else {
                let vb_cable = Self::find_virtual_cable();
                if let Some(ref cable_name) = vb_cable {
                    match Self::create_stream_for_device(cable_name) {
                        Ok(stream) => (stream, Some(cable_name.clone())),
                        Err(_) => (
                            DeviceSinkBuilder::open_default_sink()
                                .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?,
                            get_default_output_device(),
                        ),
                    }
                } else {
                    (
                        DeviceSinkBuilder::open_default_sink()
                            .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?,
                        get_default_output_device(),
                    )
                }
            };

        let mixer: &rodio::mixer::Mixer = stream_handle.mixer();
        let sink = Player::connect_new(mixer);
        sink.set_volume(default_volume * default_gain);

        let mut layers = Vec::with_capacity(NUM_AUDIO_LAYERS);
        for _ in 0..NUM_AUDIO_LAYERS {
            layers.push(AudioLayer::new(mixer));
        }

        let mic_sink = {
            let s = Player::connect_new(mixer);
            s.stop();
            s
        };

        let mut audio_player = AudioPlayer {
            _stream_handle: stream_handle,
            sink,
            layers,

            current_input_device: default_input_name,
            mic_stop_sender: None,
            mic_thread: None,
            mic_sink,

            current_output_device,

            volume: default_volume,
            gain: default_gain,
            mic_gain: default_mic_gain,
            duration: None,

            current_file_path: None,

            looped: false,
        };

        if audio_player.current_input_device.is_some() {
            if let Err(e) = audio_player.start_mic_passthrough() {
                tracing::error!("Failed to start mic passthrough: {}", e);
            }
            audio_player.apply_mic_gain();
        }

        Ok(audio_player)
    }

    fn create_stream_for_device(device_name: &str) -> Result<MixerDeviceSink> {
        let host = cpal::default_host();
        let devices = host
            .output_devices()
            .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?;

        for device in devices {
            if let Ok(desc) = device.description() {
                if desc.name() == device_name {
                    return DeviceSinkBuilder::from_device(device)
                        .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?
                        .open_sink_or_fallback()
                        .map_err(|e| SoundboardError::AudioDevice(e.to_string()));
                }
            }
        }

        Err(SoundboardError::DeviceNotFound(device_name.to_string()))
    }

    /// Try to find VB-Audio Virtual Cable output device
    fn find_virtual_cable() -> Option<String> {
        let host = cpal::default_host();
        if let Ok(devices) = host.output_devices() {
            for device in devices {
                if let Ok(desc) = device.description() {
                    let name = desc.name().to_string();
                    let lower = name.to_lowercase();
                    if lower.contains("cable input") || lower.contains("vb-audio") {
                        tracing::info!("Auto-detected VB-Audio Virtual Cable: {}", name);
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

    /// True if either the main sink or any layer sink has audio queued and is
    /// not paused. Used by the auto-PTT poll loop to decide whether the
    /// configured PTT key should currently be held.
    pub fn has_active_audio(&self) -> bool {
        if self.sink.len() > 0 && !self.sink.is_paused() {
            return true;
        }
        self.layers
            .iter()
            .any(|l| l.sink.len() > 0 && !l.sink.is_paused())
    }

    fn update_sink_volume(&self) {
        self.sink.set_volume(self.volume * self.gain);
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = clamp_volume(volume);
        self.update_sink_volume();
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = clamp_gain(gain);
        self.update_sink_volume();
    }

    pub fn get_gain(&self) -> f32 {
        self.gain
    }

    /// Apply mic gain by adjusting the mic passthrough sink volume.
    fn apply_mic_gain(&self) {
        self.mic_sink.set_volume(self.mic_gain);
    }

    pub fn set_mic_gain(&mut self, mic_gain: f32) {
        self.mic_gain = clamp_mic_gain(mic_gain);
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

    pub fn seek(&mut self, mut position: f32) -> Result<()> {
        if position < 0.0 {
            position = 0.0;
        }

        self.sink
            .try_seek(Duration::from_secs_f32(position))
            .map_err(|e| SoundboardError::Seek(e.to_string()))
    }

    pub fn get_duration(&mut self) -> Result<f32> {
        if self.get_state() == PlayerState::Stopped {
            Err(SoundboardError::NothingPlaying)
        } else {
            self.duration.ok_or(SoundboardError::DurationUnknown)
        }
    }

    pub async fn play(&mut self, file_path: &Path) -> Result<()> {
        validate_audio_file(file_path)?;
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

                Ok(())
            }
            Err(err) => Err(SoundboardError::AudioDecode(format!(
                "failed to decode '{}' (format: {}): {}",
                file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown"),
                extension,
                err
            ))),
        }
    }

    /// Preview audio through speakers only (not through virtual mic)
    pub fn preview(&mut self, file_path: &Path) -> Result<()> {
        validate_audio_file(file_path)?;
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

                Ok(())
            }
            Err(err) => Err(SoundboardError::AudioDecode(format!(
                "failed to decode '{}' (format: {}): {}",
                file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown"),
                extension,
                err
            ))),
        }
    }

    pub fn get_current_file_path(&mut self) -> &Option<PathBuf> {
        if self.get_state() == PlayerState::Stopped && !self.looped {
            self.current_file_path = None;
        }
        &self.current_file_path
    }

    pub async fn set_current_input_device(&mut self, name: &str) -> Result<()> {
        self.current_input_device = Some(name.to_string());
        self.start_mic_passthrough()?;
        self.apply_mic_gain();
        Ok(())
    }

    /// Stop the current mic capture thread and clear the mic sink. Joins the
    /// capture thread so the cpal stream is dropped before we return — leaving
    /// it dangling means a second `start_mic_passthrough` could race two
    /// `Stream`s on the same device.
    fn stop_mic_passthrough(&mut self) {
        if let Some(sender) = self.mic_stop_sender.take() {
            sender.send(()).ok();
        }
        if let Some(handle) = self.mic_thread.take() {
            if let Err(e) = handle.join() {
                tracing::error!("mic passthrough thread panicked on join: {:?}", e);
            }
        }
        self.mic_sink.stop();
    }

    /// Start capturing from the selected mic and routing audio through the output mixer
    fn start_mic_passthrough(&mut self) -> Result<()> {
        self.stop_mic_passthrough();

        let device_name = match &self.current_input_device {
            Some(name) => name.clone(),
            None => return Ok(()),
        };

        let host = cpal::default_host();
        let device = host
            .input_devices()
            .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?
            .find(|d| {
                d.description()
                    .map(|desc| desc.name() == device_name)
                    .unwrap_or(false)
            })
            .ok_or_else(|| SoundboardError::DeviceNotFound(device_name.clone()))?;

        let supported_config = device
            .default_input_config()
            .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?;
        let sample_rate_hz: u32 = supported_config.sample_rate();
        let sample_rate_nz = std::num::NonZero::<u32>::new(sample_rate_hz).ok_or_else(|| {
            SoundboardError::AudioDevice("input device reported sample rate of 0".into())
        })?;
        let channels: u16 = supported_config.channels();
        let channels_nz = std::num::NonZero::<u16>::new(channels).ok_or_else(|| {
            SoundboardError::AudioDevice("input device reported channel count of 0".into())
        })?;
        let sample_format = supported_config.sample_format();
        let stream_config: cpal::StreamConfig = supported_config.into();

        // Ring capacity = ~500ms of audio. On overflow we drop the newest
        // sample (ringbuf 0.4 has no producer-side overwrite); the 500ms
        // head-room means overflow only happens if the consumer is already
        // far behind, in which case the behavior doesn't much matter.
        // Use checked_mul to defend against pathological device reports
        // (e.g. 192 kHz × 1024 ch overflows usize on 32-bit targets).
        let max_samples = (sample_rate_hz as usize)
            .checked_mul(channels as usize)
            .and_then(|p| p.checked_div(2))
            .unwrap_or(48_000)
            .max(1);
        let ring = HeapRb::<f32>::new(max_samples);
        let (mut producer, consumer) = ring.split();

        let (stop_tx, stop_rx) = std::sync::mpsc::channel();

        let join = std::thread::spawn(move || {
            let stream_result = match sample_format {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        for &s in data {
                            let _ = producer.try_push(s);
                        }
                    },
                    |err| tracing::error!("Mic input error: {}", err),
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        for &s in data {
                            let _ = producer.try_push(s as f32 / 32768.0);
                        }
                    },
                    |err| tracing::error!("Mic input error: {}", err),
                    None,
                ),
                format => {
                    tracing::error!("Unsupported mic sample format: {:?}", format);
                    return;
                }
            };

            match stream_result {
                Ok(stream) => {
                    if let Err(e) = stream.play() {
                        tracing::error!("Failed to start mic stream: {}", e);
                        return;
                    }
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    tracing::error!("Failed to build mic input stream: {}", e);
                }
            }
        });

        let mic_source = MicSource {
            consumer,
            sample_rate: sample_rate_nz,
            channels: channels_nz,
        };
        self.mic_sink.stop();
        self.mic_sink.append(mic_source);
        self.mic_sink.play();

        self.mic_stop_sender = Some(stop_tx);
        self.mic_thread = Some(join);
        tracing::info!("Mic passthrough started: {}", device_name);
        Ok(())
    }

    // ============= Layer Management Methods =============

    /// Get the number of available layers
    pub fn get_layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Play a sound on a specific layer
    pub async fn play_on_layer(&mut self, layer_index: usize, file_path: &Path) -> Result<()> {
        if layer_index >= self.layers.len() {
            return Err(SoundboardError::LayerOutOfRange {
                index: layer_index,
                max: self.layers.len(),
            });
        }

        validate_audio_file(file_path)?;

        // File::open + Decoder::try_from both touch disk and decode the
        // header. Run them on a blocking pool so we don't stall the daemon
        // command-loop task while reading large files.
        let owned_path = file_path.to_path_buf();
        let decode_result = tokio::task::spawn_blocking(move || {
            let file = fs::File::open(&owned_path)?;
            Decoder::try_from(file).map_err(|e| SoundboardError::AudioDecode(e.to_string()))
        })
        .await
        .map_err(|e| SoundboardError::AudioDecode(format!("decode task join: {e}")))?;

        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");

        match decode_result {
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

                Ok(())
            }
            Err(err) => Err(SoundboardError::AudioDecode(format!(
                "failed to decode '{}' (format: {}) on layer {}: {}",
                file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown"),
                extension,
                layer_index,
                err
            ))),
        }
    }

    /// Stop playback on a specific layer
    pub fn stop_layer(&mut self, layer_index: usize) -> Result<()> {
        if layer_index >= self.layers.len() {
            return Err(SoundboardError::LayerOutOfRange {
                index: layer_index,
                max: self.layers.len(),
            });
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
    pub fn pause_layer(&mut self, layer_index: usize) -> Result<()> {
        if layer_index >= self.layers.len() {
            return Err(SoundboardError::LayerOutOfRange {
                index: layer_index,
                max: self.layers.len(),
            });
        }

        self.layers[layer_index].sink.pause();
        Ok(())
    }

    /// Resume a specific layer
    pub fn resume_layer(&mut self, layer_index: usize) -> Result<()> {
        if layer_index >= self.layers.len() {
            return Err(SoundboardError::LayerOutOfRange {
                index: layer_index,
                max: self.layers.len(),
            });
        }

        self.layers[layer_index].sink.play();
        Ok(())
    }

    /// Set volume for a specific layer (0.0 to 1.0)
    pub fn set_layer_volume(&mut self, layer_index: usize, volume: f32) -> Result<()> {
        if layer_index >= self.layers.len() {
            return Err(SoundboardError::LayerOutOfRange {
                index: layer_index,
                max: self.layers.len(),
            });
        }

        let layer = &mut self.layers[layer_index];
        layer.volume = clamp_layer_volume(volume);
        layer.sink.set_volume(layer.volume * self.gain);
        Ok(())
    }

    /// Get volume for a specific layer
    pub fn get_layer_volume(&self, layer_index: usize) -> Result<f32> {
        if layer_index >= self.layers.len() {
            return Err(SoundboardError::LayerOutOfRange {
                index: layer_index,
                max: self.layers.len(),
            });
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
#[serde(deny_unknown_fields)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_volume_bounds() {
        assert_eq!(clamp_volume(-1.0), 0.0);
        assert_eq!(clamp_volume(0.0), 0.0);
        assert_eq!(clamp_volume(0.5), 0.5);
        assert_eq!(clamp_volume(1.0), 1.0);
        assert_eq!(clamp_volume(2.0), 1.0);
        assert_eq!(clamp_volume(f32::INFINITY), 1.0);
        assert_eq!(clamp_volume(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn clamp_volume_nan_pins_to_low_bound() {
        // f32::clamp returns the lower bound when given NaN, which is the
        // safe failure mode for an audio mixer (silence beats screech).
        assert_eq!(clamp_volume(f32::NAN), 0.0);
    }

    #[test]
    fn clamp_gain_bounds() {
        assert_eq!(clamp_gain(MIN_GAIN - 1.0), MIN_GAIN);
        assert_eq!(clamp_gain(MIN_GAIN), MIN_GAIN);
        assert_eq!(clamp_gain(1.0), 1.0);
        assert_eq!(clamp_gain(MAX_GAIN), MAX_GAIN);
        assert_eq!(clamp_gain(MAX_GAIN + 5.0), MAX_GAIN);
        assert_eq!(clamp_gain(f32::INFINITY), MAX_GAIN);
    }

    #[test]
    fn clamp_mic_gain_bounds() {
        assert_eq!(clamp_mic_gain(0.0), MIN_MIC_GAIN);
        assert_eq!(clamp_mic_gain(MIN_MIC_GAIN - 0.1), MIN_MIC_GAIN);
        assert_eq!(clamp_mic_gain(MIN_MIC_GAIN), MIN_MIC_GAIN);
        assert_eq!(clamp_mic_gain(1.0), 1.0);
        assert_eq!(clamp_mic_gain(MAX_MIC_GAIN), MAX_MIC_GAIN);
        assert_eq!(clamp_mic_gain(10.0), MAX_MIC_GAIN);
    }

    #[test]
    fn clamp_layer_volume_bounds() {
        assert_eq!(clamp_layer_volume(-0.5), 0.0);
        assert_eq!(clamp_layer_volume(0.7), 0.7);
        assert_eq!(clamp_layer_volume(1.5), 1.0);
    }

    #[test]
    fn layer_info_default_is_inactive() {
        let info = LayerInfo::default();
        assert_eq!(info.index, 0);
        assert!(!info.is_playing);
        assert!(!info.is_paused);
        assert!(!info.is_empty);
        assert_eq!(info.volume, 0.0);
        assert!(info.current_file.is_none());
        assert_eq!(info.position, 0.0);
        assert!(info.duration.is_none());
    }

    #[test]
    fn layer_info_serde_roundtrip() {
        let original = LayerInfo {
            index: 2,
            is_playing: true,
            is_paused: false,
            is_empty: false,
            volume: 0.42,
            current_file: Some(PathBuf::from("/sounds/a.mp3")),
            position: 1.5,
            duration: Some(3.0),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: LayerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.index, original.index);
        assert!(parsed.is_playing);
        assert_eq!(parsed.volume, 0.42);
        assert_eq!(parsed.current_file, original.current_file);
    }

    #[test]
    fn player_state_serde_round_trip() {
        for state in [
            PlayerState::Stopped,
            PlayerState::Paused,
            PlayerState::Playing,
        ] {
            let s = serde_json::to_string(&state).unwrap();
            let parsed: PlayerState = serde_json::from_str(&s).unwrap();
            assert_eq!(state, parsed);
        }
    }

    #[test]
    fn num_audio_layers_const() {
        assert_eq!(NUM_AUDIO_LAYERS, 4);
    }
}
