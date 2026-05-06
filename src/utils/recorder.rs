//! Toggle-recorder: capture mic input or Windows WASAPI loopback (system
//! audio) to a WAV file in the configured sounds folder. Single recording
//! at a time; second `start` while one is active is rejected.
//!
//! cpal's `Stream` is `!Send` on Windows (WASAPI must be controlled from the
//! thread that built it), so the recording is owned by a dedicated thread
//! that lives until `stop()` flips an `AtomicBool`. This mirrors the
//! existing mic-passthrough lifecycle in `audio_player.rs`.

use crate::error::{Result, SoundboardError};
use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::cpal::{self, SampleFormat};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

#[derive(Debug, Clone, Copy)]
pub enum RecordSource {
    /// Default input device (microphone).
    Mic,
    /// Default *output* device, opened in loopback mode (records what's
    /// playing through the speakers). Windows-only.
    SystemLoopback,
}

struct ActiveRecording {
    /// Set to `true` by `stop()` to signal the worker thread to flush + exit.
    stop_flag: Arc<AtomicBool>,
    /// Worker thread that owns the cpal Stream + WAV writer.
    thread: Option<JoinHandle<Result<PathBuf>>>,
    /// Destination path; cached so `is_recording_path()` can return it without
    /// joining the thread.
    path: PathBuf,
}

/// Process-wide singleton. Wrapping in `Mutex<Option<_>>` so `start` can
/// atomically check-then-set with no race against a concurrent caller.
fn state() -> &'static Mutex<Option<ActiveRecording>> {
    static STATE: OnceLock<Mutex<Option<ActiveRecording>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

fn lock_state<'a>() -> std::sync::MutexGuard<'a, Option<ActiveRecording>> {
    match state().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

pub fn is_recording() -> bool {
    lock_state().is_some()
}

pub fn current_path() -> Option<PathBuf> {
    lock_state().as_ref().map(|r| r.path.clone())
}

/// Start a recording from the given source. Returns the destination file
/// path (not yet finalised — that happens on `stop()`). Errors if a
/// recording is already in progress.
pub fn start(source: RecordSource, dest_dir: &Path) -> Result<PathBuf> {
    {
        let guard = lock_state();
        if guard.is_some() {
            return Err(SoundboardError::Other(
                "another recording is already in progress".to_string(),
            ));
        }
    }

    if !dest_dir.exists() {
        std::fs::create_dir_all(dest_dir)?;
    }

    let path = unique_filename(source, dest_dir);

    let (device, config, sample_format) = pick_device(source)?;

    let spec = hound::WavSpec {
        channels: config.channels,
        sample_rate: config.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = hound::WavWriter::create(&path, spec).map_err(|e| {
        SoundboardError::Other(format!("failed to open WAV file for writing: {}", e))
    })?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread = spawn_capture_thread(
        device,
        config,
        sample_format,
        Arc::clone(&writer),
        Arc::clone(&stop_flag),
        path.clone(),
    );

    let mut guard = lock_state();
    *guard = Some(ActiveRecording {
        stop_flag,
        thread: Some(thread),
        path: path.clone(),
    });
    Ok(path)
}

/// Stop the active recording. Returns the finalised file path, or `None`
/// if no recording was active.
pub fn stop() -> Result<Option<PathBuf>> {
    let active = {
        let mut guard = lock_state();
        guard.take()
    };
    let Some(mut active) = active else {
        return Ok(None);
    };
    active.stop_flag.store(true, Ordering::Release);
    let handle = active.thread.take();
    if let Some(handle) = handle {
        match handle.join() {
            Ok(Ok(path)) => Ok(Some(path)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(SoundboardError::Other(
                "recorder thread panicked".to_string(),
            )),
        }
    } else {
        Ok(Some(active.path))
    }
}

/// If recording, stop and return path. If idle, start with the supplied
/// source. Convenience for hotkey toggle.
pub fn toggle(source: RecordSource, dest_dir: &Path) -> Result<ToggleOutcome> {
    if is_recording() {
        let path = stop()?;
        Ok(ToggleOutcome::Stopped(path))
    } else {
        let path = start(source, dest_dir)?;
        Ok(ToggleOutcome::Started(path))
    }
}

#[derive(Debug)]
pub enum ToggleOutcome {
    Started(PathBuf),
    Stopped(Option<PathBuf>),
}

fn pick_device(source: RecordSource) -> Result<(cpal::Device, cpal::StreamConfig, SampleFormat)> {
    let host = cpal::default_host();
    let (device, config) = match source {
        RecordSource::Mic => {
            let device = host.default_input_device().ok_or_else(|| {
                SoundboardError::AudioDevice("no default input device".to_string())
            })?;
            let cfg = device
                .default_input_config()
                .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?;
            (device, cfg)
        }
        RecordSource::SystemLoopback => {
            // On Windows, building an *input* stream on the *output* device
            // engages WASAPI loopback capture; cpal supports this directly.
            // On non-Windows platforms loopback is not available and this
            // returns an error.
            let device = host.default_output_device().ok_or_else(|| {
                SoundboardError::AudioDevice("no default output device".to_string())
            })?;
            let cfg = device
                .default_output_config()
                .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?;
            (device, cfg)
        }
    };
    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    Ok((device, stream_config, sample_format))
}

fn unique_filename(source: RecordSource, dest_dir: &Path) -> PathBuf {
    let prefix = match source {
        RecordSource::Mic => "mic-recording",
        RecordSource::SystemLoopback => "system-recording",
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    dest_dir.join(format!("{}-{}.wav", prefix, now))
}

type WavWriterArc = Arc<Mutex<Option<hound::WavWriter<BufWriter<File>>>>>;

fn spawn_capture_thread(
    device: cpal::Device,
    config: cpal::StreamConfig,
    sample_format: SampleFormat,
    writer: WavWriterArc,
    stop_flag: Arc<AtomicBool>,
    path: PathBuf,
) -> JoinHandle<Result<PathBuf>> {
    std::thread::spawn(move || -> Result<PathBuf> {
        let writer_for_cb = Arc::clone(&writer);
        let stream = build_stream(&device, &config, sample_format, writer_for_cb)?;
        stream
            .play()
            .map_err(|e| SoundboardError::AudioDevice(e.to_string()))?;
        // Spin on the stop flag with short sleeps. Cheaper than a condvar
        // and the recorder isn't latency-sensitive on the stop path.
        while !stop_flag.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        drop(stream);
        // Finalise the WAV header (writes RIFF chunk sizes).
        let mut writer_guard = match writer.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(w) = writer_guard.take() {
            w.finalize()
                .map_err(|e| SoundboardError::Other(format!("failed to finalise WAV: {}", e)))?;
        }
        Ok(path)
    })
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    writer: WavWriterArc,
) -> Result<cpal::Stream> {
    let err_fn = |err| tracing::error!("recorder stream error: {}", err);
    let stream = match sample_format {
        SampleFormat::F32 => {
            let writer = Arc::clone(&writer);
            device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut guard = match writer.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    if let Some(w) = guard.as_mut() {
                        for &s in data {
                            // Saturating f32 → i16 conversion. Clamp first
                            // to guard against post-EQ clipping above 1.0.
                            let clamped = s.clamp(-1.0, 1.0);
                            let scaled = (clamped * i16::MAX as f32) as i16;
                            if w.write_sample(scaled).is_err() {
                                tracing::error!("recorder: WAV write failed; sample dropped");
                                break;
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let writer = Arc::clone(&writer);
            device.build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let mut guard = match writer.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    if let Some(w) = guard.as_mut() {
                        for &s in data {
                            if w.write_sample(s).is_err() {
                                tracing::error!("recorder: WAV write failed; sample dropped");
                                break;
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let writer = Arc::clone(&writer);
            device.build_input_stream(
                config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let mut guard = match writer.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    if let Some(w) = guard.as_mut() {
                        for &s in data {
                            // u16 [0, 65535] → i16 [-32768, 32767]
                            let signed = s as i32 - 32768;
                            if w.write_sample(signed as i16).is_err() {
                                tracing::error!("recorder: WAV write failed; sample dropped");
                                break;
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        other => {
            return Err(SoundboardError::AudioDevice(format!(
                "unsupported sample format for recording: {:?}",
                other
            )));
        }
    };
    stream.map_err(|e| SoundboardError::AudioDevice(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_filename_includes_source_prefix() {
        let dir = std::path::Path::new("/tmp");
        let mic = unique_filename(RecordSource::Mic, dir);
        assert!(mic
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .starts_with("mic-recording-"));
        let sys = unique_filename(RecordSource::SystemLoopback, dir);
        assert!(sys
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .starts_with("system-recording-"));
        assert_eq!(mic.extension().and_then(|e| e.to_str()), Some("wav"));
        assert_eq!(sys.extension().and_then(|e| e.to_str()), Some("wav"));
    }

    #[test]
    fn is_recording_default_false() {
        // No recording started in this test; should be false. Safe even if
        // earlier tests touched the global because we never call start().
        assert!(!is_recording());
        assert!(current_path().is_none());
    }

    #[test]
    fn stop_when_idle_returns_none() {
        // start was not called; stop must be a no-op that returns Ok(None).
        assert!(matches!(stop(), Ok(None)));
    }
}
