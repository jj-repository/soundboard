use crate::{
    types::pipewire::{AudioDevice, DeviceType, Terminate},
    utils::{
        daemon::get_daemon_config,
        pipewire::{create_link, get_all_devices, get_device},
    },
};
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Debug, Eq, PartialEq, Default, Clone, Copy, Serialize, Deserialize)]
pub enum PlayerState {
    #[default]
    Stopped,
    Paused,
    Playing,
}

pub struct AudioPlayer {
    _stream_handle: OutputStream,
    sink: Sink,

    input_link_sender: Option<pipewire::channel::Sender<Terminate>>,
    pub current_input_device: Option<AudioDevice>,

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
        let mut default_input_device: Option<AudioDevice> = None;
        if let Some(name) = daemon_config.default_input_name
            && let Ok(device) = get_device(&name).await
            && device.device_type == DeviceType::Input
        {
            default_input_device = Some(device);
        }

        let stream_handle = OutputStreamBuilder::open_default_stream()?;
        let sink = Sink::connect_new(stream_handle.mixer());
        sink.set_volume(default_volume * default_gain);

        let has_input_device = default_input_device.is_some();
        let mut audio_player = AudioPlayer {
            _stream_handle: stream_handle,
            sink,

            input_link_sender: None,
            current_input_device: default_input_device,

            volume: default_volume,
            gain: default_gain,
            mic_gain: default_mic_gain,
            duration: None,

            current_file_path: None,

            looped: false,
        };

        if has_input_device {
            audio_player.link_devices().await?;
            audio_player.apply_mic_gain();
        }

        Ok(audio_player)
    }

    fn abort_link_thread(&mut self) {
        if let Some(sender) = &self.input_link_sender {
            if sender.send(Terminate {}).is_err() {
                eprintln!("Failed to send terminate signal to link thread");
            }
        }
    }

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
                .find(|d| d.name == "pwsp-virtual-mic")
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
        self.volume = volume;
        self.update_sink_volume();
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 5.0); // Allow up to 5x boost
        self.update_sink_volume();
    }

    pub fn get_gain(&self) -> f32 {
        self.gain
    }

    fn apply_mic_gain(&self) {
        if let Some(device) = &self.current_input_device {
            // Use wpctl to set the source volume
            let _ = Command::new("wpctl")
                .args(["set-volume", &device.id.to_string(), &self.mic_gain.to_string()])
                .output();
        }
    }

    pub fn set_mic_gain(&mut self, mic_gain: f32) {
        self.mic_gain = mic_gain.clamp(0.5, 3.0);
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

        match self.sink.try_seek(Duration::from_secs_f32(position)) {
            Ok(_) => Ok(()),
            Err(err) => Err(err.into()),
        }
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
            Err(err) => Err(err.into()),
        }
    }

    /// Preview audio through speakers only (not through virtual mic)
    pub fn preview(&mut self, file_path: &Path) -> Result<(), Box<dyn Error>> {
        if !file_path.exists() {
            return Err(format!("File does not exist: {}", file_path.display()).into());
        }

        let file = fs::File::open(file_path)?;
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
                self.abort_link_thread();

                self.sink.append(source);
                self.sink.play();
                // Note: We do NOT call link_devices() here - audio goes to speakers only

                Ok(())
            }
            Err(err) => Err(err.into()),
        }
    }

    pub fn get_current_file_path(&mut self) -> &Option<PathBuf> {
        if self.get_state() == PlayerState::Stopped && !self.looped {
            self.current_file_path = None;
        }
        &self.current_file_path
    }

    pub async fn set_current_input_device(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        let input_device = get_device(name).await?;

        if input_device.device_type != DeviceType::Input {
            return Err("Selected device is not an input device".into());
        }

        self.current_input_device = Some(input_device);

        self.link_devices().await?;

        Ok(())
    }
}
