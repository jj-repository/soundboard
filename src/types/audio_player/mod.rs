mod device;
mod gain;
mod layer;
mod mic;

pub use device::{get_default_output_device, get_input_devices, get_output_devices};
pub use gain::{
    clamp_gain, clamp_layer_volume, clamp_mic_gain, clamp_volume, MAX_GAIN, MAX_MIC_GAIN,
    MIN_GAIN, MIN_MIC_GAIN,
};
pub use layer::{AudioLayer, LayerInfo, NUM_AUDIO_LAYERS};

use crate::error::{Result, SoundboardError};
use crate::utils::daemon::get_daemon_config;
use crate::utils::path::has_supported_audio_extension;
use mic::MicSource;
use ringbuf::{
    traits::{Producer as ProducerTrait, Split},
    HeapRb,
};
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
                self.duration = source.total_duration().map(|d| d.as_secs_f32());
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
                self.duration = source.total_duration().map(|d| d.as_secs_f32());
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

    /// Start capturing from the selected mic and routing audio through the output mixer.
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

        // Ring capacity ≈ 500ms. On overflow we drop the newest sample; the
        // head-room means overflow only occurs if the consumer is far behind.
        // checked_mul guards against pathological device reports (e.g. 192 kHz
        // × 1024 ch overflowing usize on 32-bit).
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

    // ── Layer Management ──────────────────────────────────────────────────────

    pub fn get_layer_count(&self) -> usize {
        self.layers.len()
    }

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
                layer.duration = source.total_duration().map(|d| d.as_secs_f32());
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

    pub fn stop_all_layers(&mut self) {
        for layer in &mut self.layers {
            layer.sink.stop();
            layer.current_file_path = None;
            layer.duration = None;
        }
    }

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

    pub fn get_layer_volume(&self, layer_index: usize) -> Result<f32> {
        if layer_index >= self.layers.len() {
            return Err(SoundboardError::LayerOutOfRange {
                index: layer_index,
                max: self.layers.len(),
            });
        }
        Ok(self.layers[layer_index].volume)
    }

    pub fn is_layer_playing(&self, layer_index: usize) -> bool {
        if layer_index >= self.layers.len() {
            return false;
        }
        self.layers[layer_index].is_playing()
    }

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

    pub fn get_all_layers_info(&self) -> Vec<LayerInfo> {
        (0..self.layers.len())
            .filter_map(|i| self.get_layer_info(i))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
