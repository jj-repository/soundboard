use crate::{
    types::{
        audio_player::{LayerInfo, PlayerState},
        config::GuiConfig,
        gui::AudioPlayerState,
        socket::{Request, Response},
    },
    utils::daemon::{make_request, wait_for_daemon},
};
use std::{
    collections::HashMap,
    error::Error,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::time::{Duration, sleep};

/// Extension trait for Mutex that handles poisoning gracefully
trait MutexExt<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            eprintln!("Warning: Mutex was poisoned, recovering...");
            poisoned.into_inner()
        })
    }
}

pub fn get_gui_config() -> GuiConfig {
    GuiConfig::load_from_file().unwrap_or_else(|_| {
        let mut config = GuiConfig::default();
        config.save_to_file().ok();
        config
    })
}

pub fn make_request_sync(request: Request) -> Result<Response, Box<dyn Error>> {
    // Try to use the existing runtime if available
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // We have a runtime - use block_in_place for multi-threaded runtimes
        match handle.runtime_flavor() {
            tokio::runtime::RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(|| {
                    handle
                        .block_on(make_request(request))
                        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })
                })
            }
            _ => {
                // For current-thread runtime, use a oneshot channel to get the result
                let (tx, rx) = std::sync::mpsc::channel();
                let request_clone = request.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .ok();
                    if let Some(rt) = rt {
                        let result = rt.block_on(make_request(request_clone));
                        let _ = tx.send(result.map_err(|e| e.to_string()));
                    }
                });
                rx.recv()
                    .map_err(|_| "Thread communication failed")?
                    .map_err(|e| -> Box<dyn Error> { e.into() })
            }
        }
    } else {
        // No runtime available - create a temporary one
        // This is less efficient but prevents panics
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(make_request(request))
            .map_err(|e| -> Box<dyn Error> { e.to_string().into() })
    }
}

pub fn format_time_pair(position: f32, duration: f32) -> String {
    fn format_time(seconds: f32) -> String {
        let total_seconds = seconds.round() as u32;
        let minutes = total_seconds / 60;
        let secs = total_seconds % 60;
        format!("{:02}:{:02}", minutes, secs)
    }

    format!("{}/{}", format_time(position), format_time(duration))
}

pub fn start_app_state_thread(audio_player_state_shared: Arc<Mutex<AudioPlayerState>>) {
    tokio::spawn(async move {
        let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);
        let mut last_error_logged: Option<String> = None;

        loop {
            wait_for_daemon().await.ok();

            let state_req = Request::get_state();
            let file_path_req = Request::get_current_file_path();
            let is_paused_req = Request::get_is_paused();
            let volume_req = Request::get_volume();
            let gain_req = Request::get_gain();
            let mic_gain_req = Request::get_mic_gain();
            let position_req = Request::get_position();
            let duration_req = Request::get_duration();
            let current_input_req = Request::get_input();
            let all_inputs_req = Request::get_inputs();
            let current_output_req = Request::get_output();
            let all_outputs_req = Request::get_outputs();
            let looped_req = Request::get_loop();
            let layers_info_req = Request::get_layers_info();

            let (
                state_res,
                file_path_res,
                is_paused_res,
                volume_res,
                gain_res,
                mic_gain_res,
                position_res,
                duration_res,
                current_input_res,
                all_inputs_res,
                current_output_res,
                all_outputs_res,
                looped_res,
                layers_info_res,
            ) = tokio::join!(
                make_request(state_req),
                make_request(file_path_req),
                make_request(is_paused_req),
                make_request(volume_req),
                make_request(gain_req),
                make_request(mic_gain_req),
                make_request(position_req),
                make_request(duration_req),
                make_request(current_input_req),
                make_request(all_inputs_req),
                make_request(current_output_req),
                make_request(all_outputs_req),
                make_request(looped_req),
                make_request(layers_info_req),
            );

            // Track connection status and errors
            let mut error_count = 0;
            let mut first_error: Option<String> = None;

            // Helper macro to handle results with error tracking
            macro_rules! handle_result {
                ($res:expr) => {
                    match $res {
                        Ok(response) => response,
                        Err(e) => {
                            error_count += 1;
                            if first_error.is_none() {
                                first_error = Some(e.to_string());
                            }
                            Response::default()
                        }
                    }
                };
            }

            let state_res = handle_result!(state_res);
            let file_path_res = handle_result!(file_path_res);
            let is_paused_res = handle_result!(is_paused_res);
            let volume_res = handle_result!(volume_res);
            let gain_res = handle_result!(gain_res);
            let mic_gain_res = handle_result!(mic_gain_res);
            let position_res = handle_result!(position_res);
            let duration_res = handle_result!(duration_res);
            let current_input_res = handle_result!(current_input_res);
            let all_inputs_res = handle_result!(all_inputs_res);
            let current_output_res = handle_result!(current_output_res);
            let all_outputs_res = handle_result!(all_outputs_res);
            let looped_res = handle_result!(looped_res);
            let layers_info_res = handle_result!(layers_info_res);

            // Determine connection status
            let daemon_connected = error_count == 0;

            // Log errors only when they change (avoid spam)
            if let Some(ref err) = first_error {
                if last_error_logged.as_ref() != Some(err) {
                    eprintln!("Daemon communication error ({} requests failed): {}", error_count, err);
                    last_error_logged = Some(err.clone());
                }
            } else if last_error_logged.is_some() {
                // Connection restored
                println!("Daemon connection restored");
                last_error_logged = None;
            }

            let state = match state_res.status {
                true => serde_json::from_str::<PlayerState>(&state_res.message)
                    .unwrap_or_default(),
                false => PlayerState::default(),
            };

            let file_path = match file_path_res.status {
                true => PathBuf::from(file_path_res.message),
                false => PathBuf::new(),
            };
            let is_paused = match is_paused_res.status {
                true => is_paused_res.message == "true",
                false => false,
            };
            let volume = match volume_res.status {
                true => volume_res.message.parse::<f32>().unwrap_or(1.0),
                false => 1.0,
            };
            let gain = match gain_res.status {
                true => gain_res.message.parse::<f32>().unwrap_or(1.0),
                false => 1.0,
            };
            let mic_gain = match mic_gain_res.status {
                true => mic_gain_res.message.parse::<f32>().unwrap_or(1.0),
                false => 1.0,
            };
            let position = match position_res.status {
                true => position_res.message.parse::<f32>().unwrap_or(0.0),
                false => 0.0,
            };
            let duration = match duration_res.status {
                true => duration_res.message.parse::<f32>().unwrap_or(0.0),
                false => 0.0,
            };
            let current_input = match current_input_res.status {
                true => current_input_res
                    .message
                    .as_str()
                    .split(" - ")
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                false => String::new(),
            };
            let all_inputs = match all_inputs_res.status {
                true => all_inputs_res
                    .message
                    .as_str()
                    .split(';')
                    .filter_map(|entry| {
                        let entry = entry.trim();
                        if entry.is_empty() {
                            return None;
                        }
                        entry
                            .split_once(" - ")
                            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    })
                    .collect::<HashMap<String, String>>(),
                false => HashMap::new(),
            };
            let current_output = match current_output_res.status {
                true => current_output_res.message,
                false => String::new(),
            };
            let all_outputs = match all_outputs_res.status {
                true => all_outputs_res
                    .message
                    .as_str()
                    .split(';')
                    .filter_map(|entry| {
                        let entry = entry.trim();
                        if entry.is_empty() {
                            return None;
                        }
                        Some((entry.to_string(), entry.to_string()))
                    })
                    .collect::<HashMap<String, String>>(),
                false => HashMap::new(),
            };
            let looped = match looped_res.status {
                true => looped_res.message.parse::<bool>().unwrap_or_default(),
                false => false,
            };
            let layers = match layers_info_res.status {
                true => serde_json::from_str::<Vec<LayerInfo>>(&layers_info_res.message)
                    .unwrap_or_default(),
                false => Vec::new(),
            };

            {
                let mut guard = audio_player_state_shared.lock_or_recover();

                guard.state = guard.new_state.take().unwrap_or(state);
                guard.current_file_path = file_path;
                guard.is_paused = is_paused;
                guard.volume = guard.new_volume.take().unwrap_or(volume);
                guard.gain = guard.new_gain.take().unwrap_or(gain);
                guard.mic_gain = guard.new_mic_gain.take().unwrap_or(mic_gain);
                guard.position = guard.new_position.take().unwrap_or(position);
                guard.duration = if duration > 0.0 { duration } else { 1.0 };
                guard.current_input = current_input;
                guard.all_inputs = all_inputs;
                guard.current_output = current_output;
                guard.all_outputs = all_outputs;
                guard.looped = looped;
                guard.layers = layers;

                // Update connection status
                guard.daemon_connected = daemon_connected;
                guard.last_error = first_error.clone();
            }

            sleep(sleep_duration).await;
        }
    });
}
