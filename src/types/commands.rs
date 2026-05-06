//! IPC command enum + dispatch. Replaces the prior 25-struct `Executable`
//! impls (~550 LOC of boilerplate). Each variant carries its parsed args; the
//! IPC parser (`utils::commands::parse_command`) is responsible for
//! validating + populating them, so `execute` can match exhaustively without
//! per-call argument re-checks.

use crate::{
    types::{audio_player::PlayerState, socket::Response},
    utils::daemon::get_audio_player,
};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum DaemonCommand {
    Ping,
    Pause,
    Resume,
    TogglePause,
    Stop,

    IsPaused,
    GetState,
    GetVolume,
    SetVolume(f32),
    GetGain,
    SetGain(f32),
    GetMicGain,
    SetMicGain(f32),
    GetPosition,
    Seek(f32),
    GetDuration,
    Play(PathBuf),
    Preview(PathBuf),

    GetCurrentFilePath,
    GetCurrentInput,
    GetAllInputs,
    SetCurrentInput(String),
    GetCurrentOutput,
    GetAllOutputs,
    SetCurrentOutput(String),

    GetLoop,
    SetLoop(bool),
    ToggleLoop,

    PlayOnLayer { index: usize, file_path: PathBuf },
    StopLayer(usize),
    StopAllLayers,
    SetLayerVolume { index: usize, volume: f32 },
    GetLayersInfo,
}

impl DaemonCommand {
    pub async fn execute(self) -> Response {
        match self {
            DaemonCommand::Ping => Response::new(true, "pong"),

            DaemonCommand::Pause => {
                let mut p = get_audio_player().lock().await;
                p.pause();
                Response::new(true, "Audio was paused")
            }
            DaemonCommand::Resume => {
                let mut p = get_audio_player().lock().await;
                p.resume();
                Response::new(true, "Audio was resumed")
            }
            DaemonCommand::TogglePause => {
                let mut p = get_audio_player().lock().await;
                if p.get_state() == PlayerState::Stopped {
                    return Response::new(false, "Audio is not playing");
                }
                if p.is_paused() {
                    p.resume();
                    Response::new(true, "Audio was resumed")
                } else {
                    p.pause();
                    Response::new(true, "Audio was paused")
                }
            }
            DaemonCommand::Stop => {
                let mut p = get_audio_player().lock().await;
                p.stop();
                Response::new(true, "Audio was stopped")
            }

            DaemonCommand::IsPaused => {
                let p = get_audio_player().lock().await;
                Response::new(true, p.is_paused().to_string())
            }
            DaemonCommand::GetState => {
                let p = get_audio_player().lock().await;
                match serde_json::to_string(&p.get_state()) {
                    Ok(json) => Response::new(true, json),
                    Err(_) => Response::new(false, "Failed to serialize player state"),
                }
            }
            DaemonCommand::GetVolume => {
                let p = get_audio_player().lock().await;
                Response::new(true, p.volume.to_string())
            }
            DaemonCommand::SetVolume(volume) => {
                let mut p = get_audio_player().lock().await;
                p.set_volume(volume);
                Response::new(true, format!("Audio volume was set to {}", volume))
            }
            DaemonCommand::GetGain => {
                let p = get_audio_player().lock().await;
                Response::new(true, p.get_gain().to_string())
            }
            DaemonCommand::SetGain(gain) => {
                let mut p = get_audio_player().lock().await;
                p.set_gain(gain);
                Response::new(true, format!("Audio gain was set to {}", gain))
            }
            DaemonCommand::GetMicGain => {
                let p = get_audio_player().lock().await;
                Response::new(true, p.get_mic_gain().to_string())
            }
            DaemonCommand::SetMicGain(mic_gain) => {
                let mut p = get_audio_player().lock().await;
                p.set_mic_gain(mic_gain);
                Response::new(true, format!("Mic gain was set to {}", mic_gain))
            }
            DaemonCommand::GetPosition => {
                let p = get_audio_player().lock().await;
                Response::new(true, p.get_position().to_string())
            }
            DaemonCommand::Seek(position) => {
                let mut p = get_audio_player().lock().await;
                match p.seek(position) {
                    Ok(_) => Response::new(true, format!("Audio position was set to {}", position)),
                    Err(e) => Response::new(false, e.to_string()),
                }
            }
            DaemonCommand::GetDuration => {
                let mut p = get_audio_player().lock().await;
                match p.get_duration() {
                    Ok(d) => Response::new(true, d.to_string()),
                    Err(e) => Response::new(false, e.to_string()),
                }
            }
            DaemonCommand::Play(file_path) => {
                let mut p = get_audio_player().lock().await;
                match p.play(&file_path).await {
                    Ok(_) => Response::new(true, format!("Now playing {}", file_path.display())),
                    Err(e) => Response::new(false, e.to_string()),
                }
            }
            DaemonCommand::Preview(file_path) => {
                let mut p = get_audio_player().lock().await;
                match p.preview(&file_path) {
                    Ok(_) => Response::new(true, format!("Previewing {}", file_path.display())),
                    Err(e) => Response::new(false, e.to_string()),
                }
            }

            DaemonCommand::GetCurrentFilePath => {
                let mut p = get_audio_player().lock().await;
                match p.get_current_file_path() {
                    Some(path) => match path.to_str() {
                        Some(s) => Response::new(true, s),
                        None => Response::new(false, "File path contains invalid UTF-8"),
                    },
                    None => Response::new(false, "No file is playing"),
                }
            }
            DaemonCommand::GetCurrentInput => {
                let p = get_audio_player().lock().await;
                match &p.current_input_device {
                    Some(name) => Response::new(true, format!("{} - {}", name, name)),
                    None => Response::new(false, "No input device selected"),
                }
            }
            DaemonCommand::GetAllInputs => {
                let devices = crate::types::audio_player::get_input_devices();
                let mut entries: Vec<String> = devices
                    .keys()
                    .map(|n| format!("{} - {}", n, n))
                    .collect();
                entries.sort();
                Response::new(true, entries.join("; "))
            }
            DaemonCommand::SetCurrentInput(name) => {
                let mut p = get_audio_player().lock().await;
                match p.set_current_input_device(&name).await {
                    Ok(_) => Response::new(true, "Input device was set"),
                    Err(e) => Response::new(false, e.to_string()),
                }
            }
            DaemonCommand::GetCurrentOutput => {
                let p = get_audio_player().lock().await;
                match p.get_current_output_device() {
                    Some(d) => Response::new(true, d.clone()),
                    None => Response::new(false, "No output device selected"),
                }
            }
            DaemonCommand::GetAllOutputs => {
                let p = get_audio_player().lock().await;
                let s = p
                    .get_all_output_devices()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ");
                Response::new(true, s)
            }
            DaemonCommand::SetCurrentOutput(name) => {
                let mut config = crate::utils::daemon::get_daemon_config();
                config.default_output_name = Some(name);
                match config.save_to_file() {
                    Ok(()) => Response::new(
                        true,
                        "Output device preference saved (restart daemon to apply)",
                    ),
                    Err(e) => Response::new(false, format!("Failed to save preference: {}", e)),
                }
            }

            DaemonCommand::GetLoop => {
                let p = get_audio_player().lock().await;
                Response::new(true, p.looped.to_string())
            }
            DaemonCommand::SetLoop(enabled) => {
                let mut p = get_audio_player().lock().await;
                p.looped = enabled;
                Response::new(true, format!("Loop was set to {}", enabled))
            }
            DaemonCommand::ToggleLoop => {
                let mut p = get_audio_player().lock().await;
                p.looped = !p.looped;
                Response::new(true, format!("Loop was set to {}", p.looped))
            }

            DaemonCommand::PlayOnLayer { index, file_path } => {
                let mut p = get_audio_player().lock().await;
                match p.play_on_layer(index, &file_path).await {
                    Ok(_) => Response::new(
                        true,
                        format!("Playing {} on layer {}", file_path.display(), index),
                    ),
                    Err(e) => Response::new(false, e.to_string()),
                }
            }
            DaemonCommand::StopLayer(index) => {
                let mut p = get_audio_player().lock().await;
                match p.stop_layer(index) {
                    Ok(_) => Response::new(true, format!("Stopped layer {}", index)),
                    Err(e) => Response::new(false, e.to_string()),
                }
            }
            DaemonCommand::StopAllLayers => {
                let mut p = get_audio_player().lock().await;
                p.stop_all_layers();
                Response::new(true, "All layers stopped")
            }
            DaemonCommand::SetLayerVolume { index, volume } => {
                let mut p = get_audio_player().lock().await;
                match p.set_layer_volume(index, volume) {
                    Ok(_) => Response::new(
                        true,
                        format!("Layer {} volume set to {}", index, volume),
                    ),
                    Err(e) => Response::new(false, e.to_string()),
                }
            }
            DaemonCommand::GetLayersInfo => {
                let p = get_audio_player().lock().await;
                match serde_json::to_string(&p.get_all_layers_info()) {
                    Ok(json) => Response::new(true, json),
                    Err(_) => Response::new(false, "Failed to serialize layers info"),
                }
            }
        }
    }
}
