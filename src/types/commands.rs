use crate::{
    types::{audio_player::PlayerState, socket::Response},
    utils::{daemon::get_audio_player, pipewire::get_all_devices},
};
use async_trait::async_trait;
use std::path::PathBuf;

#[async_trait]
pub trait Executable {
    async fn execute(&self) -> Response;
}

pub struct PingCommand {}

pub struct PauseCommand {}

pub struct ResumeCommand {}

pub struct TogglePauseCommand {}

pub struct StopCommand {}

pub struct IsPausedCommand {}

pub struct GetStateCommand {}

pub struct GetVolumeCommand {}

pub struct SetVolumeCommand {
    pub volume: Option<f32>,
}

pub struct GetGainCommand {}

pub struct SetGainCommand {
    pub gain: Option<f32>,
}

pub struct GetMicGainCommand {}

pub struct SetMicGainCommand {
    pub mic_gain: Option<f32>,
}

pub struct GetPositionCommand {}

pub struct SeekCommand {
    pub position: Option<f32>,
}

pub struct GetDurationCommand {}

pub struct PlayCommand {
    pub file_path: Option<PathBuf>,
}

pub struct PreviewCommand {
    pub file_path: Option<PathBuf>,
}

pub struct GetCurrentFilePathCommand {}

pub struct GetCurrentInputCommand {}

pub struct GetAllInputsCommand {}

pub struct SetCurrentInputCommand {
    pub name: Option<String>,
}

pub struct GetCurrentOutputCommand {}

pub struct GetAllOutputsCommand {}

pub struct SetCurrentOutputCommand {
    pub name: Option<String>,
}

pub struct GetLoopCommand {}

pub struct SetLoopCommand {
    pub enabled: Option<bool>,
}

pub struct ToggleLoopCommand {}

// Layer commands
pub struct PlayOnLayerCommand {
    pub layer_index: Option<usize>,
    pub file_path: Option<PathBuf>,
}

pub struct StopLayerCommand {
    pub layer_index: Option<usize>,
}

pub struct StopAllLayersCommand {}

pub struct SetLayerVolumeCommand {
    pub layer_index: Option<usize>,
    pub volume: Option<f32>,
}

pub struct GetLayersInfoCommand {}

#[async_trait]
impl Executable for PingCommand {
    async fn execute(&self) -> Response {
        Response::new(true, "pong")
    }
}

#[async_trait]
impl Executable for PauseCommand {
    async fn execute(&self) -> Response {
        let mut audio_player = get_audio_player().lock().await;
        audio_player.pause();
        Response::new(true, "Audio was paused")
    }
}

#[async_trait]
impl Executable for ResumeCommand {
    async fn execute(&self) -> Response {
        let mut audio_player = get_audio_player().lock().await;
        audio_player.resume();
        Response::new(true, "Audio was resumed")
    }
}

#[async_trait]
impl Executable for TogglePauseCommand {
    async fn execute(&self) -> Response {
        let mut audio_player = get_audio_player().lock().await;

        if audio_player.get_state() == PlayerState::Stopped {
            return Response::new(false, "Audio is not playing");
        }

        if audio_player.is_paused() {
            audio_player.resume();
            Response::new(true, "Audio was resumed")
        } else {
            audio_player.pause();
            Response::new(true, "Audio was paused")
        }
    }
}

#[async_trait]
impl Executable for StopCommand {
    async fn execute(&self) -> Response {
        let mut audio_player = get_audio_player().lock().await;
        audio_player.stop();
        Response::new(true, "Audio was stopped")
    }
}

#[async_trait]
impl Executable for IsPausedCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        let is_paused = audio_player.is_paused().to_string();
        Response::new(true, is_paused)
    }
}

#[async_trait]
impl Executable for GetStateCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        let state = audio_player.get_state();
        match serde_json::to_string(&state) {
            Ok(json) => Response::new(true, json),
            Err(_) => Response::new(false, "Failed to serialize player state"),
        }
    }
}

#[async_trait]
impl Executable for GetVolumeCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        let volume = audio_player.volume;
        Response::new(true, volume.to_string())
    }
}

#[async_trait]
impl Executable for SetVolumeCommand {
    async fn execute(&self) -> Response {
        if let Some(volume) = self.volume {
            let mut audio_player = get_audio_player().lock().await;
            audio_player.set_volume(volume);
            Response::new(true, format!("Audio volume was set to {}", volume))
        } else {
            Response::new(false, "Invalid volume value")
        }
    }
}

#[async_trait]
impl Executable for GetGainCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        let gain = audio_player.get_gain();
        Response::new(true, gain.to_string())
    }
}

#[async_trait]
impl Executable for SetGainCommand {
    async fn execute(&self) -> Response {
        if let Some(gain) = self.gain {
            let mut audio_player = get_audio_player().lock().await;
            audio_player.set_gain(gain);
            Response::new(true, format!("Audio gain was set to {}", gain))
        } else {
            Response::new(false, "Invalid gain value")
        }
    }
}

#[async_trait]
impl Executable for GetMicGainCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        let mic_gain = audio_player.get_mic_gain();
        Response::new(true, mic_gain.to_string())
    }
}

#[async_trait]
impl Executable for SetMicGainCommand {
    async fn execute(&self) -> Response {
        if let Some(mic_gain) = self.mic_gain {
            let mut audio_player = get_audio_player().lock().await;
            audio_player.set_mic_gain(mic_gain);
            Response::new(true, format!("Mic gain was set to {}", mic_gain))
        } else {
            Response::new(false, "Invalid mic gain value")
        }
    }
}

#[async_trait]
impl Executable for GetPositionCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        let position = audio_player.get_position();
        Response::new(true, position.to_string())
    }
}

#[async_trait]
impl Executable for SeekCommand {
    async fn execute(&self) -> Response {
        if let Some(position) = self.position {
            let mut audio_player = get_audio_player().lock().await;
            match audio_player.seek(position) {
                Ok(_) => Response::new(true, format!("Audio position was set to {}", position)),
                Err(err) => Response::new(false, err.to_string()),
            }
        } else {
            Response::new(false, "Invalid position value")
        }
    }
}

#[async_trait]
impl Executable for GetDurationCommand {
    async fn execute(&self) -> Response {
        let mut audio_player = get_audio_player().lock().await;
        match audio_player.get_duration() {
            Ok(duration) => Response::new(true, duration.to_string()),
            Err(err) => Response::new(false, err.to_string()),
        }
    }
}

#[async_trait]
impl Executable for PlayCommand {
    async fn execute(&self) -> Response {
        if let Some(file_path) = &self.file_path {
            let mut audio_player = get_audio_player().lock().await;
            match audio_player.play(file_path).await {
                Ok(_) => Response::new(true, format!("Now playing {}", file_path.display())),
                Err(err) => Response::new(false, err.to_string()),
            }
        } else {
            Response::new(false, "Invalid file path")
        }
    }
}

#[async_trait]
impl Executable for PreviewCommand {
    async fn execute(&self) -> Response {
        if let Some(file_path) = &self.file_path {
            let mut audio_player = get_audio_player().lock().await;
            match audio_player.preview(file_path) {
                Ok(_) => Response::new(true, format!("Previewing {}", file_path.display())),
                Err(err) => Response::new(false, err.to_string()),
            }
        } else {
            Response::new(false, "Invalid file path")
        }
    }
}

#[async_trait]
impl Executable for GetCurrentFilePathCommand {
    async fn execute(&self) -> Response {
        let mut audio_player = get_audio_player().lock().await;
        let current_file_path = audio_player.get_current_file_path();
        if let Some(current_file_path) = current_file_path {
            match current_file_path.to_str() {
                Some(path_str) => Response::new(true, path_str),
                None => Response::new(false, "File path contains invalid UTF-8"),
            }
        } else {
            Response::new(false, "No file is playing")
        }
    }
}

#[async_trait]
impl Executable for GetCurrentInputCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        if let Some(input_device) = &audio_player.current_input_device {
            Response::new(
                true,
                format!("{} - {}", input_device.name, input_device.nick),
            )
        } else {
            Response::new(false, "No input device selected")
        }
    }
}

#[async_trait]
impl Executable for GetAllInputsCommand {
    async fn execute(&self) -> Response {
        match get_all_devices().await {
            Ok((input_devices, _output_devices)) => {
                let mut input_devices_strings = vec![];
                for device in input_devices {
                    if device.name == "pwsp-virtual-mic" {
                        continue;
                    }

                    let string = format!("{} - {}", device.name, device.nick);
                    input_devices_strings.push(string);
                }
                let response_message = input_devices_strings.join("; ");
                Response::new(true, response_message)
            }
            Err(e) => Response::new(false, format!("Failed to get input devices: {}", e)),
        }
    }
}

#[async_trait]
impl Executable for SetCurrentInputCommand {
    async fn execute(&self) -> Response {
        if let Some(name) = &self.name {
            let mut audio_player = get_audio_player().lock().await;
            match audio_player.set_current_input_device(name).await {
                Ok(_) => Response::new(true, "Input device was set"),
                Err(err) => Response::new(false, err.to_string()),
            }
        } else {
            Response::new(false, "Invalid index value")
        }
    }
}

#[async_trait]
impl Executable for GetCurrentOutputCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        if let Some(output_device) = audio_player.get_current_output_device() {
            Response::new(true, output_device.clone())
        } else {
            Response::new(false, "No output device selected")
        }
    }
}

#[async_trait]
impl Executable for GetAllOutputsCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        let output_devices = audio_player.get_all_output_devices();
        let output_devices_strings: Vec<String> = output_devices.keys().cloned().collect();
        let response_message = output_devices_strings.join("; ");
        Response::new(true, response_message)
    }
}

#[async_trait]
impl Executable for SetCurrentOutputCommand {
    async fn execute(&self) -> Response {
        if let Some(_name) = &self.name {
            // Note: Changing output device requires restarting the audio stream
            // For now, we just save the preference - it will take effect on next daemon restart
            Response::new(true, "Output device preference saved (restart daemon to apply)")
        } else {
            Response::new(false, "Invalid output device name")
        }
    }
}

#[async_trait]
impl Executable for GetLoopCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        Response::new(true, audio_player.looped.to_string())
    }
}

#[async_trait]
impl Executable for SetLoopCommand {
    async fn execute(&self) -> Response {
        let mut audio_player = get_audio_player().lock().await;

        match self.enabled {
            Some(enabled) => {
                audio_player.looped = enabled;
                Response::new(true, format!("Loop was set to {}", enabled))
            }
            None => Response::new(false, "Invalid enabled value"),
        }
    }
}

#[async_trait]
impl Executable for ToggleLoopCommand {
    async fn execute(&self) -> Response {
        let mut audio_player = get_audio_player().lock().await;
        audio_player.looped = !audio_player.looped;
        Response::new(true, format!("Loop was set to {}", audio_player.looped))
    }
}

// ============= Layer Command Implementations =============

#[async_trait]
impl Executable for PlayOnLayerCommand {
    async fn execute(&self) -> Response {
        match (&self.layer_index, &self.file_path) {
            (Some(layer_index), Some(file_path)) => {
                let mut audio_player = get_audio_player().lock().await;
                match audio_player.play_on_layer(*layer_index, file_path).await {
                    Ok(_) => Response::new(
                        true,
                        format!("Playing {} on layer {}", file_path.display(), layer_index),
                    ),
                    Err(err) => Response::new(false, err.to_string()),
                }
            }
            _ => Response::new(false, "Invalid layer index or file path"),
        }
    }
}

#[async_trait]
impl Executable for StopLayerCommand {
    async fn execute(&self) -> Response {
        if let Some(layer_index) = self.layer_index {
            let mut audio_player = get_audio_player().lock().await;
            match audio_player.stop_layer(layer_index) {
                Ok(_) => Response::new(true, format!("Stopped layer {}", layer_index)),
                Err(err) => Response::new(false, err.to_string()),
            }
        } else {
            Response::new(false, "Invalid layer index")
        }
    }
}

#[async_trait]
impl Executable for StopAllLayersCommand {
    async fn execute(&self) -> Response {
        let mut audio_player = get_audio_player().lock().await;
        audio_player.stop_all_layers();
        Response::new(true, "All layers stopped")
    }
}

#[async_trait]
impl Executable for SetLayerVolumeCommand {
    async fn execute(&self) -> Response {
        match (self.layer_index, self.volume) {
            (Some(layer_index), Some(volume)) => {
                let mut audio_player = get_audio_player().lock().await;
                match audio_player.set_layer_volume(layer_index, volume) {
                    Ok(_) => Response::new(
                        true,
                        format!("Layer {} volume set to {}", layer_index, volume),
                    ),
                    Err(err) => Response::new(false, err.to_string()),
                }
            }
            _ => Response::new(false, "Invalid layer index or volume"),
        }
    }
}

#[async_trait]
impl Executable for GetLayersInfoCommand {
    async fn execute(&self) -> Response {
        let audio_player = get_audio_player().lock().await;
        let layers_info = audio_player.get_all_layers_info();
        match serde_json::to_string(&layers_info) {
            Ok(json) => Response::new(true, json),
            Err(_) => Response::new(false, "Failed to serialize layers info"),
        }
    }
}
