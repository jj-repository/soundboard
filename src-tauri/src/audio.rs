use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::RwLock;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

pub struct AudioManager {
    stream_handle: OutputStreamHandle,
    active_sounds: Arc<RwLock<Vec<Arc<Sink>>>>,
    master_volume: Arc<RwLock<f32>>,
}

impl AudioManager {
    pub fn new() -> Result<Self> {
        // Always use default device - we'll route to Soundboard_Mix using pactl
        let (stream, stream_handle) = OutputStream::try_default()
            .context("Failed to create audio output stream")?;

        // Leak the OutputStream to keep it alive for the entire application lifetime.
        // This is safe because the audio stream should remain active until the app exits.
        // OutputStream is not Send/Sync, but we only need the handle which is.
        std::mem::forget(stream);

        Ok(Self {
            stream_handle,
            active_sounds: Arc::new(RwLock::new(Vec::new())),
            master_volume: Arc::new(RwLock::new(1.0)),
        })
    }

    pub fn play_sound(&self, path: PathBuf, volume: f32) -> Result<()> {
        let file = File::open(&path)
            .context(format!("Failed to open audio file: {:?}", path))?;
        let source = Decoder::new(BufReader::new(file))
            .context("Failed to decode audio file")?;

        let sink = Sink::try_new(&self.stream_handle)
            .context("Failed to create audio sink")?;

        let master_vol = *self.master_volume.read();
        sink.set_volume(volume * master_vol);
        sink.append(source);

        let sink_arc = Arc::new(sink);
        self.active_sounds.write().push(sink_arc.clone());

        // Clean up finished sounds in background
        let active_sounds = self.active_sounds.clone();
        std::thread::spawn(move || {
            sink_arc.sleep_until_end();
            active_sounds.write().retain(|s| !s.empty());
        });

        Ok(())
    }

    pub fn stop_sound(&self) -> Result<()> {
        // Individual sound stopping is not implemented in rodio for overlapping sounds.
        // Each sound plays independently in its own sink and tracking individual sinks
        // by ID would require significant changes to the architecture.
        // Use stop_all() to stop all sounds at once.
        Ok(())
    }

    pub fn stop_all(&self) -> Result<()> {
        let mut sounds = self.active_sounds.write();
        for sink in sounds.drain(..) {
            sink.stop();
        }
        Ok(())
    }

    pub fn set_master_volume(&self, volume: f32) -> Result<()> {
        *self.master_volume.write() = volume.clamp(0.0, 2.0);

        // Update all active sounds
        let sounds = self.active_sounds.read();
        for sink in sounds.iter() {
            sink.set_volume(volume);
        }

        Ok(())
    }

    pub fn list_output_devices(&self) -> Result<Vec<String>> {
        let host = cpal::default_host();
        let devices: Vec<String> = host
            .output_devices()?
            .filter_map(|device| device.name().ok())
            .collect();
        Ok(devices)
    }
}
