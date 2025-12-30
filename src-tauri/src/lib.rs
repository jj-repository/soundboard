mod audio;
#[cfg(target_os = "linux")]
mod hotkeys;
mod pipewire;
mod sound_manager;

use audio::AudioManager;
#[cfg(target_os = "linux")]
use hotkeys::HotkeyManager;
use pipewire::PipeWireManager;
use sound_manager::{Sound, SoundManager, SoundboardState};

use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
#[cfg(target_os = "linux")]
use tauri::Emitter;
use tauri::State;

pub struct AppState {
    audio_manager: Arc<AudioManager>,
    sound_manager: Arc<SoundManager>,
    pipewire_manager: Arc<Mutex<PipeWireManager>>,
    #[cfg(target_os = "linux")]
    hotkey_manager: Arc<Mutex<HotkeyManager>>,
}

// Tauri Commands

#[tauri::command]
async fn load_folder(folder: String, state: State<'_, AppState>) -> Result<Vec<Sound>, String> {
    let path = PathBuf::from(folder);
    state
        .sound_manager
        .load_folder(path)
        .map_err(|e| e.to_string())?;
    Ok(state.sound_manager.get_all_sounds())
}

#[tauri::command]
async fn add_sound(path: String, state: State<'_, AppState>) -> Result<Sound, String> {
    let path = PathBuf::from(path);
    state
        .sound_manager
        .add_sound(path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_sound(id: String, state: State<'_, AppState>) -> Result<(), String> {
    // Unregister hotkey if exists (Linux only)
    #[cfg(target_os = "linux")]
    let _ = state.hotkey_manager.lock().unregister(&id);

    state
        .sound_manager
        .remove_sound(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_sound(
    id: String,
    name: Option<String>,
    volume: Option<f32>,
    hotkey: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Update hotkey if provided (Linux only)
    #[cfg(target_os = "linux")]
    if let Some(ref hotkey_str) = hotkey {
        if !hotkey_str.is_empty() {
            state
                .hotkey_manager
                .lock()
                .register(id.clone(), hotkey_str)
                .map_err(|e| format!("Failed to register hotkey: {}", e))?;
        } else {
            let _ = state.hotkey_manager.lock().unregister(&id);
        }
    }

    state
        .sound_manager
        .update_sound(&id, name, volume, hotkey)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn play_sound(id: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(sound) = state.sound_manager.get_sound(&id) {
        state
            .audio_manager
            .play_sound(id, sound.path, sound.volume)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn stop_sound(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .audio_manager
        .stop_sound(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_all_sounds(state: State<'_, AppState>) -> Result<(), String> {
    state
        .audio_manager
        .stop_all()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_all_sounds(state: State<'_, AppState>) -> Result<Vec<Sound>, String> {
    Ok(state.sound_manager.get_all_sounds())
}

#[tauri::command]
async fn get_state(state: State<'_, AppState>) -> Result<SoundboardState, String> {
    Ok(state.sound_manager.get_state())
}

#[tauri::command]
async fn set_master_volume(volume: f32, state: State<'_, AppState>) -> Result<(), String> {
    state.sound_manager.set_master_volume(volume);
    state
        .audio_manager
        .set_master_volume(volume)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn setup_virtual_microphone(state: State<'_, AppState>) -> Result<String, String> {
    let pw = state.pipewire_manager.lock();
    pw.setup_virtual_microphone()
        .map_err(|e| e.to_string())?;

    // Immediately route any existing tauri-app audio to Soundboard_Mix
    pw.route_all_app_audio_to_sink()
        .map_err(|e| e.to_string())?;

    drop(pw);

    // Start a background thread to continuously route new audio streams
    // This is needed because CPAL creates new sink inputs as sounds play
    let pw_manager = state.pipewire_manager.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let pw = pw_manager.lock();
            let _ = pw.route_all_app_audio_to_sink();
        }
    });

    Ok("Soundboard_Mix".to_string())
}

#[tauri::command]
async fn check_virtual_mic_exists(state: State<'_, AppState>) -> Result<bool, String> {
    state
        .pipewire_manager
        .lock()
        .check_virtual_mic_exists()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn cleanup_virtual_microphone(state: State<'_, AppState>) -> Result<(), String> {
    // Cleanup PipeWire virtual devices
    state
        .pipewire_manager
        .lock()
        .cleanup()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_system_audio_routing(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.sound_manager.set_system_audio_routing(enabled);
    state
        .pipewire_manager
        .lock()
        .route_system_audio_to_mic(enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_audio_sources(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .pipewire_manager
        .lock()
        .list_sources()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_audio_sinks(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .pipewire_manager
        .lock()
        .list_sinks()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_default_source(state: State<'_, AppState>) -> Result<String, String> {
    state
        .pipewire_manager
        .lock()
        .get_default_source()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_default_source(source: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .pipewire_manager
        .lock()
        .set_default_source(&source)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_output_devices(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .audio_manager
        .list_output_devices()
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let audio_manager = Arc::new(AudioManager::new().expect("Failed to create audio manager"));
            let sound_manager = Arc::new(SoundManager::new());
            let pipewire_manager = Arc::new(Mutex::new(PipeWireManager::new()));

            #[cfg(target_os = "linux")]
            let hotkey_manager = Arc::new(Mutex::new(
                HotkeyManager::new().expect("Failed to create hotkey manager"),
            ));

            app.manage(AppState {
                audio_manager: audio_manager.clone(),
                sound_manager: sound_manager.clone(),
                pipewire_manager: pipewire_manager.clone(),
                #[cfg(target_os = "linux")]
                hotkey_manager: hotkey_manager.clone(),
            });

            // Setup hotkey listener in background
            // Note: Global hotkeys are currently only supported on Linux due to thread safety issues
            // with the Windows implementation in the global-hotkey crate
            #[cfg(target_os = "linux")]
            {
                let app_handle = app.handle().clone();
                let sound_manager_clone = sound_manager.clone();
                let audio_manager_clone = audio_manager.clone();

                std::thread::spawn(move || {
                    let receiver = HotkeyManager::get_receiver().expect("Failed to get hotkey receiver");
                    loop {
                        if receiver.recv().is_ok() {
                            // Find sound by hotkey and play it
                            if let Some(sound) = sound_manager_clone.get_all_sounds().iter().find(|s| {
                                s.hotkey.is_some() // Match the hotkey ID with the event
                            }) {
                                let _ = audio_manager_clone.play_sound(
                                    sound.id.clone(),
                                    sound.path.clone(),
                                    sound.volume,
                                );

                                // Emit event to frontend
                                let _ = app_handle.emit("sound-played", sound.id.clone());
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_folder,
            add_sound,
            remove_sound,
            update_sound,
            play_sound,
            stop_sound,
            stop_all_sounds,
            get_all_sounds,
            get_state,
            set_master_volume,
            setup_virtual_microphone,
            check_virtual_mic_exists,
            cleanup_virtual_microphone,
            toggle_system_audio_routing,
            list_audio_sources,
            list_audio_sinks,
            get_default_source,
            set_default_source,
            list_output_devices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
