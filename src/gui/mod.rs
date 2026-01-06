mod draw;
mod hotkeys;
mod input;
pub mod tray;
mod update;

use crate::gui::hotkeys::{HotkeyAction, HotkeyManager};
use crate::gui::tray::{TrayHandle, TrayMessage, start_tray};
use eframe::{HardwareAcceleration, NativeOptions, icon_data::from_png_bytes, run_native};
use egui::{Context, Vec2, ViewportBuilder};
use pwsp::{
    types::{
        audio_player::PlayerState,
        config::GuiConfig,
        gui::{AppState, AudioPlayerState, UpdateStatus},
        socket::Request,
    },
    utils::{
        daemon::get_daemon_config,
        gui::{get_gui_config, make_request_sync, start_app_state_thread},
        updater::{check_for_updates, download_update},
    },
};
use rfd::FileDialog;
use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};
use std::{
    error::Error,
    sync::{Arc, Mutex},
    thread,
};

const SUPPORTED_EXTENSIONS: [&str; 11] = [
    "mp3", "wav", "ogg", "flac", "mp4", "m4a", "aac", "mov", "mkv", "webm", "avi",
];

struct SoundpadGui {
    pub app_state: AppState,
    pub config: GuiConfig,
    pub audio_player_state: AudioPlayerState,
    pub audio_player_state_shared: Arc<Mutex<AudioPlayerState>>,
    pub tray_handle: Option<TrayHandle>,
    pub hotkey_manager: Option<HotkeyManager>,
    pub update_receiver: Option<mpsc::Receiver<UpdateStatus>>,
}

impl SoundpadGui {
    fn new(ctx: &Context) -> Self {
        let audio_player_state = Arc::new(Mutex::new(AudioPlayerState::default()));
        start_app_state_thread(audio_player_state.clone());

        let config = get_gui_config();
        ctx.set_zoom_factor(config.scale_factor);

        let mut app_state = AppState::default();
        app_state.dirs = config.dirs.clone();
        app_state.gain_slider_value = 1.0;
        app_state.mic_gain_slider_value = 1.0;

        let mut audio_player_state_local = AudioPlayerState::default();
        audio_player_state_local.gain = 1.0;
        audio_player_state_local.mic_gain = 1.0;

        let tray_handle = start_tray();
        let hotkey_manager = HotkeyManager::new(&config.hotkeys);

        SoundpadGui {
            app_state,
            config,
            audio_player_state: audio_player_state_local,
            audio_player_state_shared: audio_player_state,
            tray_handle,
            hotkey_manager,
            update_receiver: None,
        }
    }

    fn poll_tray_messages(&mut self, ctx: &Context) {
        // Collect messages first to avoid borrow issues
        let messages: Vec<TrayMessage> = if let Some(ref tray) = self.tray_handle {
            let mut msgs = Vec::new();
            loop {
                match tray.receiver.try_recv() {
                    Ok(msg) => msgs.push(msg),
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            }
            msgs
        } else {
            Vec::new()
        };

        // Process messages
        for msg in messages {
            match msg {
                TrayMessage::PlayPause => {
                    self.play_toggle();
                }
                TrayMessage::Stop => {
                    self.stop();
                }
                TrayMessage::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn poll_hotkey_messages(&mut self) {
        // Collect messages first to avoid borrow issues
        let actions: Vec<HotkeyAction> = if let Some(ref hk) = self.hotkey_manager {
            let mut acts = Vec::new();
            loop {
                match hk.receiver.try_recv() {
                    Ok(action) => acts.push(action),
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            }
            acts
        } else {
            Vec::new()
        };

        // Process actions
        for action in actions {
            match action {
                HotkeyAction::PlayPause => {
                    self.play_toggle();
                }
                HotkeyAction::Stop => {
                    self.stop();
                }
            }
        }
    }

    fn poll_update_status(&mut self) {
        if let Some(ref receiver) = self.update_receiver {
            match receiver.try_recv() {
                Ok(status) => {
                    self.app_state.update_status = status;
                    // Clear receiver if we got a final status
                    match &self.app_state.update_status {
                        UpdateStatus::Checking | UpdateStatus::Downloading { .. } => {}
                        _ => {
                            self.update_receiver = None;
                        }
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.update_receiver = None;
                }
            }
        }
    }

    pub fn check_for_updates(&mut self) {
        self.app_state.update_status = UpdateStatus::Checking;

        let (sender, receiver) = mpsc::channel();
        self.update_receiver = Some(receiver);

        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(check_for_updates());

            let status = match result {
                Ok(info) => {
                    if info.update_available {
                        UpdateStatus::UpdateAvailable {
                            latest_version: info.latest_version,
                            release_url: info.release_url,
                            download_url: info.download_url,
                        }
                    } else {
                        UpdateStatus::UpToDate
                    }
                }
                Err(e) => UpdateStatus::Error(e.to_string()),
            };

            sender.send(status).ok();
        });
    }

    pub fn download_update(&mut self, url: String) {
        self.app_state.update_status = UpdateStatus::Downloading { progress: 0.0 };

        let (sender, receiver) = mpsc::channel();
        self.update_receiver = Some(receiver);

        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(download_update(&url, |downloaded, total| {
                if total > 0 {
                    let progress = downloaded as f32 / total as f32;
                    // Note: We can't send progress updates easily here since we're in a closure
                    // For simplicity, we just wait for completion
                    let _ = progress;
                }
            }));

            let status = match result {
                Ok(path) => UpdateStatus::Downloaded { file_path: path },
                Err(e) => UpdateStatus::Error(e.to_string()),
            };

            sender.send(status).ok();
        });
    }

    pub fn play_toggle(&mut self) {
        let (new_state, request) = {
            let guard = self.audio_player_state_shared.lock().unwrap();
            match guard.state {
                PlayerState::Playing => (Some(PlayerState::Paused), Some(Request::pause())),
                PlayerState::Paused => (Some(PlayerState::Playing), Some(Request::resume())),
                PlayerState::Stopped => (None, None),
            }
        };

        if let Some(req) = request {
            make_request_sync(req).ok();
        }

        if let Some(state) = new_state {
            let mut guard = self.audio_player_state_shared.lock().unwrap();
            guard.new_state = Some(state);
            guard.state = state;
        }
    }

    pub fn open_file(&mut self) {
        let file_dialog = FileDialog::new().add_filter("Audio File", &SUPPORTED_EXTENSIONS);
        if let Some(path) = file_dialog.pick_file() {
            self.play_file(&path);
        }
    }

    pub fn add_dirs(&mut self) {
        let file_dialog = FileDialog::new();
        if let Some(paths) = file_dialog.pick_folders() {
            for path in paths {
                self.app_state.dirs.insert(path);
            }
            self.save_dirs_config();
        }
    }

    pub fn remove_dir(&mut self, path: &PathBuf) {
        self.app_state.dirs.remove(path);
        if self.app_state.current_dir.as_ref() == Some(path) {
            self.app_state.current_dir = None;
            self.app_state.files.clear();
        }
        self.save_dirs_config();
    }

    fn save_dirs_config(&mut self) {
        self.config.dirs = self.app_state.dirs.clone();
        self.config.save_to_file().ok();
    }

    pub fn open_dir(&mut self, path: &PathBuf) {
        self.app_state.current_dir = Some(path.clone());
        self.app_state.files = match path.read_dir() {
            Ok(entries) => entries.filter_map(|res| res.ok()).map(|e| e.path()).collect(),
            Err(e) => {
                eprintln!("Failed to read directory {}: {}", path.display(), e);
                Default::default()
            }
        };
    }

    pub fn play_file(&mut self, path: &PathBuf) {
        if let Some(path_str) = path.to_str() {
            if let Err(e) = make_request_sync(Request::play(path_str)) {
                eprintln!("Failed to send play request: {}", e);
            }
        } else {
            eprintln!("Invalid file path encoding");
        }
    }

    pub fn preview_file(&mut self, path: &PathBuf) {
        if let Some(path_str) = path.to_str() {
            if let Err(e) = make_request_sync(Request::preview(path_str)) {
                eprintln!("Failed to send preview request: {}", e);
            }
        } else {
            eprintln!("Invalid file path encoding");
        }
    }

    pub fn set_input(&mut self, name: String) {
        make_request_sync(Request::set_input(&name)).ok();

        if self.config.save_input {
            let mut daemon_config = get_daemon_config();
            daemon_config.default_input_name = Some(name);
            daemon_config.save_to_file().ok();
        }
    }

    pub fn set_output(&mut self, name: String) {
        make_request_sync(Request::set_output(&name)).ok();

        // Save output preference to daemon config
        let mut daemon_config = get_daemon_config();
        daemon_config.default_output_name = Some(name);
        daemon_config.save_to_file().ok();
    }

    pub fn toggle_loop(&mut self) {
        make_request_sync(Request::toggle_loop()).ok();
    }

    pub fn update_hotkeys(&mut self) {
        if let Some(ref mut hk) = self.hotkey_manager {
            hk.update_hotkeys(&self.config.hotkeys);
        }
    }

    pub fn stop(&mut self) {
        make_request_sync(Request::stop()).ok();
        let mut guard = self.audio_player_state_shared.lock().unwrap();
        guard.new_state = Some(PlayerState::Stopped);
        guard.state = PlayerState::Stopped;
    }

    pub fn play_on_layer(&mut self, layer_index: usize, path: &PathBuf) {
        if let Some(path_str) = path.to_str() {
            if let Err(e) = make_request_sync(Request::play_on_layer(layer_index, path_str)) {
                eprintln!("Failed to play on layer {}: {}", layer_index, e);
            }
        }
    }

    pub fn stop_layer(&mut self, layer_index: usize) {
        if let Err(e) = make_request_sync(Request::stop_layer(layer_index)) {
            eprintln!("Failed to stop layer {}: {}", layer_index, e);
        }
    }

    pub fn stop_all_layers(&mut self) {
        if let Err(e) = make_request_sync(Request::stop_all_layers()) {
            eprintln!("Failed to stop all layers: {}", e);
        }
    }

    pub fn toggle_favorite(&mut self, path: &PathBuf) {
        if self.config.favorites.contains(path) {
            self.config.favorites.remove(path);
        } else {
            self.config.favorites.insert(path.clone());
        }
        self.config.save_to_file().ok();
    }

    pub fn is_favorite(&self, path: &PathBuf) -> bool {
        self.config.favorites.contains(path)
    }

    pub fn create_category(&mut self, name: &str) {
        if !name.is_empty() && !self.config.categories.contains_key(name) {
            use pwsp::types::config::SoundCategory;
            self.config.categories.insert(name.to_string(), SoundCategory::new(name));
            self.config.save_to_file().ok();
        }
    }

    pub fn delete_category(&mut self, name: &str) {
        self.config.categories.remove(name);
        if self.app_state.current_category.as_deref() == Some(name) {
            self.app_state.current_category = None;
        }
        self.config.save_to_file().ok();
    }

    pub fn rename_category(&mut self, old_name: &str, new_name: &str) {
        if !new_name.is_empty() && !self.config.categories.contains_key(new_name) {
            if let Some(mut category) = self.config.categories.remove(old_name) {
                category.name = new_name.to_string();
                self.config.categories.insert(new_name.to_string(), category);
                if self.app_state.current_category.as_deref() == Some(old_name) {
                    self.app_state.current_category = Some(new_name.to_string());
                }
                self.config.save_to_file().ok();
            }
        }
    }

    pub fn add_to_category(&mut self, category_name: &str, path: &PathBuf) {
        if let Some(category) = self.config.categories.get_mut(category_name) {
            category.add_sound(path.clone());
            self.config.save_to_file().ok();
        }
    }

    pub fn remove_from_category(&mut self, category_name: &str, path: &PathBuf) {
        if let Some(category) = self.config.categories.get_mut(category_name) {
            category.remove_sound(path);
            self.config.save_to_file().ok();
        }
    }

    pub fn open_category(&mut self, name: &str) {
        self.app_state.current_category = Some(name.to_string());
        self.app_state.current_dir = None;
        self.app_state.files.clear();
    }

    pub fn get_sound_metadata(&self, path: &PathBuf) -> Option<&pwsp::types::config::SoundMetadata> {
        self.config.sound_metadata.get(path)
    }

    pub fn set_sound_custom_name(&mut self, path: &PathBuf, name: Option<String>) {
        let metadata = self.config.sound_metadata.entry(path.clone()).or_default();
        metadata.custom_name = name.filter(|s| !s.is_empty());
        if metadata.is_empty() {
            self.config.sound_metadata.remove(path);
        }
        self.config.save_to_file().ok();
    }

    pub fn set_sound_description(&mut self, path: &PathBuf, description: Option<String>) {
        let metadata = self.config.sound_metadata.entry(path.clone()).or_default();
        metadata.description = description.filter(|s| !s.is_empty());
        if metadata.is_empty() {
            self.config.sound_metadata.remove(path);
        }
        self.config.save_to_file().ok();
    }

    pub fn add_sound_tag(&mut self, path: &PathBuf, tag: &str) {
        let metadata = self.config.sound_metadata.entry(path.clone()).or_default();
        metadata.add_tag(tag);
        self.config.save_to_file().ok();
    }

    pub fn remove_sound_tag(&mut self, path: &PathBuf, tag: &str) {
        if let Some(metadata) = self.config.sound_metadata.get_mut(path) {
            metadata.remove_tag(tag);
            if metadata.is_empty() {
                self.config.sound_metadata.remove(path);
            }
            self.config.save_to_file().ok();
        }
    }

    pub fn get_all_tags(&self) -> Vec<String> {
        let mut all_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
        for metadata in self.config.sound_metadata.values() {
            all_tags.extend(metadata.tags.iter().cloned());
        }
        let mut tags: Vec<_> = all_tags.into_iter().collect();
        tags.sort();
        tags
    }
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    const ICON: &[u8] = include_bytes!("../../assets/icon.png");

    let options = NativeOptions {
        vsync: true,
        centered: true,
        hardware_acceleration: HardwareAcceleration::Preferred,

        viewport: ViewportBuilder::default()
            .with_app_id("ru.arabianq.pwsp")
            .with_inner_size(Vec2::new(1200.0, 800.0))
            .with_min_inner_size(Vec2::new(800.0, 600.0))
            .with_icon(from_png_bytes(ICON)?),

        ..Default::default()
    };

    match run_native(
        "Pipewire Soundpad",
        options,
        Box::new(|cc| {
            egui_material_icons::initialize(&cc.egui_ctx);
            Ok(Box::new(SoundpadGui::new(&cc.egui_ctx)))
        }),
    ) {
        Ok(_) => {
            let config = get_gui_config();
            if config.pause_on_exit {
                make_request_sync(Request::pause()).ok();
            }
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
