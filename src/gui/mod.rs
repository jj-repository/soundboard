mod draw;
mod gamepad;
mod hotkeys;
mod input;
mod midi;
pub mod tray; // pub: tray handle is stored in SoundpadGui and started externally
mod update;
mod vbcable_wizard;

use crate::gui::hotkeys::{HotkeyAction, HotkeyManager};
use crate::gui::tray::{start_tray, TrayHandle, TrayMessage};
use eframe::{icon_data::from_png_bytes, run_native, HardwareAcceleration, NativeOptions};
use egui::{Context, Vec2, ViewportBuilder};
use rfd::FileDialog;
use soundboard::{
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
    MutexExt,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
};

use soundboard::error::Result;

const SUPPORTED_EXTENSIONS: [&str; 13] = [
    "mp3", "wav", "ogg", "flac", "mp4", "m4a", "aac", "mov", "mkv", "webm", "avi", "opus", "wma",
];

use soundboard::utils::path::validate_path_within;

struct SoundpadGui {
    pub app_state: AppState,
    pub config: GuiConfig,
    pub audio_player_state: AudioPlayerState,
    pub audio_player_state_shared: Arc<Mutex<AudioPlayerState>>,
    pub tray_handle: Option<TrayHandle>,
    pub hotkey_manager: Option<HotkeyManager>,
    pub update_receiver: Option<mpsc::Receiver<UpdateStatus>>,
    /// Flag to track if startup update check has been performed
    pub startup_update_checked: bool,
    /// Cache of path-exists results, refreshed periodically to avoid per-frame syscalls
    pub file_existence_cache: HashMap<PathBuf, bool>,
    pub file_existence_checked_at: Option<std::time::Instant>,
    /// 500ms-TTL caches for per-frame Vec<String> rebuilds (tags + playlist order)
    pub tags_cache: Option<(std::time::Instant, Vec<String>)>,
    pub ordered_playlists_cache: Option<(std::time::Instant, Vec<String>)>,
    /// One-shot check on first frame whether VB-Cable is already installed.
    pub vbcable_checked: bool,
    pub vbcable_wizard_visible: bool,
    /// `None` = wizard is at the prompt stage; `Some(_)` = install in flight.
    pub vbcable_wizard_status: Option<soundboard::utils::vbcable_install::InstallStatus>,
    pub vbcable_install_rx:
        Option<mpsc::Receiver<soundboard::utils::vbcable_install::InstallStatus>>,
    /// Receiver for the active yt-dlp grab thread; `None` when idle.
    pub url_grab_rx: Option<mpsc::Receiver<soundboard::utils::yt_grab::GrabStatus>>,
    /// MIDI input listener; `None` if disabled or no MIDI ports available.
    /// Field exists only to keep the manager alive (its `Drop` closes ports).
    #[allow(dead_code)]
    pub midi_manager: Option<crate::gui::midi::MidiManager>,
    /// Gamepad input listener; `None` if disabled or gilrs failed to init.
    /// Field exists only to keep the manager alive (its `Drop` joins thread).
    #[allow(dead_code)]
    pub gamepad_manager: Option<crate::gui::gamepad::GamepadManager>,
}

impl SoundpadGui {
    fn new(ctx: &Context) -> Self {
        let audio_player_state = Arc::new(Mutex::new(AudioPlayerState::default()));
        start_app_state_thread(audio_player_state.clone());

        let config = get_gui_config();
        ctx.set_zoom_factor(config.scale_factor);

        let app_state = AppState {
            dirs: config.dirs.clone(),
            gain_slider_value: 1.0,
            mic_gain_slider_value: 1.0,
            // Check if sounds folder setup is needed
            show_sounds_folder_setup: config.sounds_folder.is_none(),
            // Auto-select "All Sounds" playlist if sounds folder is set
            current_playlist: if config.sounds_folder.is_some() {
                Some("All Sounds".to_string())
            } else {
                None
            },
            ..Default::default()
        };

        let audio_player_state_local = AudioPlayerState {
            gain: 1.0,
            mic_gain: 1.0,
            ..Default::default()
        };

        let tray_handle = start_tray();
        let hotkey_manager = HotkeyManager::new(&config.hotkeys);

        // Plumb MIDI + gamepad input into the same HotkeyAction channel.
        // Both are off by default; users opt in via gui.json.
        let (midi_manager, gamepad_manager) = if let Some(ref hk) = hotkey_manager {
            let sender = hk.sender();
            let midi = crate::gui::midi::MidiManager::new(sender.clone(), &config.hotkeys.midi);
            let gp = crate::gui::gamepad::GamepadManager::new(sender, &config.hotkeys.gamepad);
            (midi, gp)
        } else {
            (None, None)
        };

        SoundpadGui {
            app_state,
            config,
            audio_player_state: audio_player_state_local,
            audio_player_state_shared: audio_player_state,
            tray_handle,
            hotkey_manager,
            update_receiver: None,
            startup_update_checked: false,
            file_existence_cache: HashMap::new(),
            file_existence_checked_at: None,
            tags_cache: None,
            ordered_playlists_cache: None,
            vbcable_checked: false,
            vbcable_wizard_visible: false,
            vbcable_wizard_status: None,
            vbcable_install_rx: None,
            url_grab_rx: None,
            midi_manager,
            gamepad_manager,
        }
    }

    /// Returns true if `path` exists, refreshing the cache at most once per second.
    /// Avoids the per-row stat syscall in the 60fps draw loop.
    pub fn path_exists_cached(&mut self, path: &Path) -> bool {
        let now = std::time::Instant::now();
        let stale = self
            .file_existence_checked_at
            .is_none_or(|t| now.duration_since(t) > std::time::Duration::from_secs(1));
        if stale {
            self.file_existence_cache.clear();
            self.file_existence_checked_at = Some(now);
        }
        *self
            .file_existence_cache
            .entry(path.to_path_buf())
            .or_insert_with(|| path.exists())
    }

    fn poll_tray_messages(&mut self, ctx: &Context) {
        let messages: Vec<TrayMessage> = if let Some(ref tray) = self.tray_handle {
            let mut msgs = Vec::new();
            while let Ok(msg) = tray.receiver.try_recv() {
                msgs.push(msg);
            }
            msgs
        } else {
            Vec::new()
        };

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
        let actions: Vec<HotkeyAction> = if let Some(ref hk) = self.hotkey_manager {
            let mut acts = Vec::new();
            while let Ok(action) = hk.receiver.try_recv() {
                acts.push(action);
            }
            acts
        } else {
            Vec::new()
        };

        for action in actions {
            match action {
                HotkeyAction::ToggleRecording => {
                    self.toggle_recording();
                }
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
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    sender
                        .send(UpdateStatus::Error(format!(
                            "Failed to create runtime: {}",
                            e
                        )))
                        .ok();
                    return;
                }
            };
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
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    sender
                        .send(UpdateStatus::Error(format!(
                            "Failed to create runtime: {}",
                            e
                        )))
                        .ok();
                    return;
                }
            };
            let result = rt.block_on(download_update(&url, |_downloaded, _total| {}));

            let status = match result {
                Ok(path) => UpdateStatus::Downloaded { file_path: path },
                Err(e) => UpdateStatus::Error(e.to_string()),
            };

            sender.send(status).ok();
        });
    }

    pub fn play_toggle(&mut self) {
        let (new_state, request) = {
            let guard = self.audio_player_state_shared.lock_or_recover();
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
            let mut guard = self.audio_player_state_shared.lock_or_recover();
            guard.new_state = Some(state);
            guard.state = state;
        }
    }

    pub fn open_dir(&mut self, path: &Path) {
        self.app_state.current_dir = Some(path.to_path_buf());
        self.app_state.files = match path.read_dir() {
            Ok(entries) => entries
                .filter_map(|res| match res {
                    Ok(e) => Some(e.path()),
                    Err(e) => {
                        tracing::warn!("Skipping unreadable entry under {}: {}", path.display(), e);
                        None
                    }
                })
                .collect(),
            Err(e) => {
                tracing::error!("Failed to read directory {}: {}", path.display(), e);
                Default::default()
            }
        };
    }

    pub fn play_file(&mut self, path: &PathBuf) {
        // Apply per-sound volume if set
        if let Some(sound_volume) = self.get_sound_volume(path) {
            if let Err(e) = make_request_sync(Request::set_volume(sound_volume)) {
                tracing::warn!("Failed to apply per-sound volume {}: {}", sound_volume, e);
            }
            // Update local state to reflect the volume change
            let mut guard = self.audio_player_state_shared.lock_or_recover();
            guard.volume = sound_volume;
            guard.new_volume = Some(sound_volume);
        }

        if let Some(path_str) = path.to_str() {
            if let Err(e) = make_request_sync(Request::play(path_str)) {
                tracing::error!("Failed to send play request: {}", e);
            }
        } else {
            tracing::error!("Invalid file path encoding");
        }
    }

    pub fn preview_file(&mut self, path: &Path) {
        if let Some(path_str) = path.to_str() {
            if let Err(e) = make_request_sync(Request::preview(path_str)) {
                tracing::error!("Failed to send preview request: {}", e);
            }
        } else {
            tracing::error!("Invalid file path encoding");
        }
    }

    pub fn set_input(&mut self, name: String) {
        make_request_sync(Request::set_input(&name)).ok();

        if self.config.save_input {
            let mut daemon_config = get_daemon_config();
            daemon_config.default_input_name = Some(name);
            if let Err(e) = daemon_config.save_to_file() {
                tracing::error!("Failed to save daemon config: {}", e);
            }
        }
    }

    pub fn set_output(&mut self, name: String) {
        make_request_sync(Request::set_output(&name)).ok();

        // Save output preference to daemon config
        let mut daemon_config = get_daemon_config();
        daemon_config.default_output_name = Some(name);
        if let Err(e) = daemon_config.save_to_file() {
            tracing::error!("Failed to save daemon config: {}", e);
        }
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
        let mut guard = self.audio_player_state_shared.lock_or_recover();
        guard.new_state = Some(PlayerState::Stopped);
        guard.state = PlayerState::Stopped;
    }

    pub fn stop_layer(&mut self, layer_index: usize) {
        if let Err(e) = make_request_sync(Request::stop_layer(layer_index)) {
            tracing::error!("Failed to stop layer {}: {}", layer_index, e);
        }
    }

    pub fn stop_all_layers(&mut self) {
        if let Err(e) = make_request_sync(Request::stop_all_layers()) {
            tracing::error!("Failed to stop all layers: {}", e);
        }
    }

    pub fn toggle_favorite(&mut self, path: &PathBuf) {
        let was_favorite = self.config.favorites.contains(path);
        if was_favorite {
            self.config.favorites.remove(path);
            // If viewing Favourites playlist, remove from display
            if self.app_state.current_playlist.as_deref() == Some("Favourites") {
                self.app_state.files.remove(path);
                self.invalidate_files_cache();
            }
        } else {
            self.config.favorites.insert(path.clone());
            // If viewing Favourites playlist, add to display
            if self.app_state.current_playlist.as_deref() == Some("Favourites") && path.exists() {
                self.app_state.files.insert(path.clone());
                self.invalidate_files_cache();
            }
        }
        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    pub fn is_favorite(&self, path: &PathBuf) -> bool {
        self.config.favorites.contains(path)
    }

    /// Invalidate the sorted files cache (call when files are added/removed)
    #[inline]
    fn invalidate_files_cache(&mut self) {
        self.app_state.sorted_files_cache = None;
    }

    pub fn add_to_category(&mut self, category_name: &str, path: &Path) {
        if let Some(category) = self.config.categories.get_mut(category_name) {
            category.add_sound(path.to_path_buf());
            if let Err(e) = self.config.save_to_file() {
                tracing::error!("Failed to save config: {}", e);
            }
        }
    }

    pub fn remove_from_category(&mut self, category_name: &str, path: &PathBuf) {
        if let Some(category) = self.config.categories.get_mut(category_name) {
            category.remove_sound(path);
            if let Err(e) = self.config.save_to_file() {
                tracing::error!("Failed to save config: {}", e);
            }
        }
    }

    /// Open a playlist (either "All Sounds", "Favourites", or a user-created playlist)
    pub fn open_playlist(&mut self, name: &str) {
        self.app_state.current_playlist = Some(name.to_string());
        self.app_state.current_category = None;
        self.app_state.current_dir = None;

        if name == "All Sounds" {
            // Load all files from sounds folder
            self.load_all_sounds();
        } else if name == "Favourites" {
            // Load all favorited files - collect existing paths to avoid clone
            let existing_favorites: Vec<_> = self
                .config
                .favorites
                .iter()
                .filter(|p| p.exists())
                .cloned()
                .collect();
            self.app_state.files.clear();
            self.app_state.files.extend(existing_favorites);
            self.invalidate_files_cache();
        } else {
            // Load files from the playlist
            self.app_state.files.clear();
            if let Some(playlist) = self.config.categories.get(name) {
                for path in &playlist.sounds {
                    if path.exists() {
                        self.app_state.files.insert(path.clone());
                    }
                }
            }
            self.invalidate_files_cache();
        }
    }

    /// Load all sounds from the sounds folder
    fn load_all_sounds(&mut self) {
        self.app_state.files.clear();
        if let Some(ref sounds_folder) = self.config.sounds_folder {
            if let Ok(entries) = sounds_folder.read_dir() {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        // Check if it's a supported audio file
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if SUPPORTED_EXTENSIONS.iter().any(|&e| e.eq_ignore_ascii_case(ext)) {
                                self.app_state.files.insert(path);
                            }
                        }
                    }
                }
            }
        }
        self.invalidate_files_cache();
    }

    /// Create a new playlist
    pub fn create_playlist(&mut self, name: &str) {
        if !name.is_empty()
            && name != "All Sounds"
            && name != "Favourites"
            && !self.config.categories.contains_key(name)
        {
            use soundboard::types::config::SoundCategory;
            self.config
                .categories
                .insert(name.to_string(), SoundCategory::new(name));
            // Add to playlist order
            self.config.playlist_order.push(name.to_string());
            if let Err(e) = self.config.save_to_file() {
                tracing::error!("Failed to save config: {}", e);
            }
        }
    }

    /// Delete a playlist
    pub fn delete_playlist(&mut self, name: &str) {
        self.config.categories.remove(name);
        // Remove from playlist order
        self.config.playlist_order.retain(|n| n != name);
        if self.app_state.current_playlist.as_deref() == Some(name) {
            self.app_state.current_playlist = None;
            self.app_state.files.clear();
            self.invalidate_files_cache();
        }
        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    /// Rename a playlist
    pub fn rename_playlist(&mut self, old_name: &str, new_name: &str) {
        if !new_name.is_empty()
            && new_name != "All Sounds"
            && !self.config.categories.contains_key(new_name)
        {
            if let Some(mut playlist) = self.config.categories.remove(old_name) {
                playlist.name = new_name.to_string();
                self.config
                    .categories
                    .insert(new_name.to_string(), playlist);
                if self.app_state.current_playlist.as_deref() == Some(old_name) {
                    self.app_state.current_playlist = Some(new_name.to_string());
                }
                // Update playlist order
                if let Some(pos) = self
                    .config
                    .playlist_order
                    .iter()
                    .position(|n| n == old_name)
                {
                    self.config.playlist_order[pos] = new_name.to_string();
                }
                if let Err(e) = self.config.save_to_file() {
                    tracing::error!("Failed to save config: {}", e);
                }
            }
        }
    }

    /// Get playlists in their configured order
    pub fn get_ordered_playlists(&mut self) -> Vec<String> {
        const TTL: std::time::Duration = std::time::Duration::from_millis(500);
        let now = std::time::Instant::now();
        if let Some((when, list)) = &self.ordered_playlists_cache {
            if now.duration_since(*when) < TTL {
                return list.clone();
            }
        }

        let mut ordered = Vec::new();

        // First, add playlists in the configured order
        let ordered_set: std::collections::HashSet<&String> =
            self.config.playlist_order.iter().collect();
        for name in &self.config.playlist_order {
            if self.config.categories.contains_key(name) {
                ordered.push(name.clone());
            }
        }

        // Then, add any playlists not in the order list (sorted alphabetically).
        // O(1) contains check via set instead of O(n) Vec::contains.
        let mut remaining: Vec<_> = self
            .config
            .categories
            .keys()
            .filter(|k| !ordered_set.contains(*k))
            .cloned()
            .collect();
        remaining.sort();
        ordered.extend(remaining);

        self.ordered_playlists_cache = Some((now, ordered.clone()));
        ordered
    }

    /// Move a playlist from one position to another
    pub fn move_playlist(&mut self, from_index: usize, to_index: usize) {
        // Ensure playlist_order contains all playlists
        self.sync_playlist_order();

        if from_index >= self.config.playlist_order.len()
            || to_index >= self.config.playlist_order.len()
        {
            return;
        }

        let playlist = self.config.playlist_order.remove(from_index);
        self.config.playlist_order.insert(to_index, playlist);
        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    /// Sync playlist_order to contain all playlists
    fn sync_playlist_order(&mut self) {
        // Remove entries that no longer exist
        self.config
            .playlist_order
            .retain(|name| self.config.categories.contains_key(name));

        // Add missing entries
        for name in self.config.categories.keys() {
            if !self.config.playlist_order.contains(name) {
                self.config.playlist_order.push(name.clone());
            }
        }
    }

    /// Add a sound to a playlist
    pub fn add_to_playlist(&mut self, playlist_name: &str, path: &Path) {
        if playlist_name == "All Sounds" {
            return; // Can't manually add to All Sounds
        }
        if let Some(playlist) = self.config.categories.get_mut(playlist_name) {
            playlist.add_sound(path.to_path_buf());
            if let Err(e) = self.config.save_to_file() {
                tracing::error!("Failed to save config: {}", e);
            }
        }
    }

    /// Move a sound within a playlist from one position to another
    pub fn move_sound_in_playlist(
        &mut self,
        playlist_name: &str,
        from_index: usize,
        to_index: usize,
    ) {
        if playlist_name == "All Sounds" || playlist_name == "Favourites" {
            return; // Can't reorder these virtual playlists
        }
        if let Some(playlist) = self.config.categories.get_mut(playlist_name) {
            if from_index >= playlist.sounds.len() || to_index >= playlist.sounds.len() {
                return;
            }
            let sound = playlist.sounds.remove(from_index);
            playlist.sounds.insert(to_index, sound);
            if let Err(e) = self.config.save_to_file() {
                tracing::error!("Failed to save config: {}", e);
            }
        }
    }

    /// Remove a sound from a playlist
    /// If "All Sounds", this deletes the actual file
    /// For other playlists, it just removes from the playlist
    pub fn remove_from_playlist(&mut self, playlist_name: &str, path: &PathBuf) {
        if playlist_name == "All Sounds" {
            // Delete the actual file
            self.delete_sound_file(path);
        } else {
            // Just remove from playlist
            if let Some(playlist) = self.config.categories.get_mut(playlist_name) {
                playlist.remove_sound(path);
                if let Err(e) = self.config.save_to_file() {
                    tracing::error!("Failed to save config: {}", e);
                }
            }
        }
        // Remove from current files view
        self.app_state.files.remove(path);
        self.invalidate_files_cache();
    }

    /// Delete a sound file from the sounds folder
    /// Only deletes files that are within the configured sounds folder for security
    pub fn delete_sound_file(&mut self, path: &PathBuf) {
        // Security: Validate the path is within the sounds folder
        let Some(ref sounds_folder) = self.config.sounds_folder else {
            tracing::error!("Cannot delete file: sounds folder not configured");
            return;
        };

        let validated_path = match validate_path_within(path, sounds_folder) {
            Some(p) => p,
            None => {
                tracing::error!(
                    "Security: Refusing to delete file outside sounds folder: {}",
                    path.display()
                );
                return;
            }
        };

        // Delete the file
        if let Err(e) = std::fs::remove_file(&validated_path) {
            tracing::error!("Failed to delete file {}: {}", validated_path.display(), e);
            return;
        }

        // Remove from all playlists
        for playlist in self.config.categories.values_mut() {
            playlist.remove_sound(path);
        }

        // Remove from favorites
        self.config.favorites.remove(path);

        // Remove metadata
        self.config.sound_metadata.remove(path);

        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }

        // Remove from current files view
        self.app_state.files.remove(path);
        self.invalidate_files_cache();
    }

    pub fn set_sound_custom_name(&mut self, path: &PathBuf, name: Option<String>) {
        let metadata = self.config.sound_metadata.entry(path.clone()).or_default();
        metadata.custom_name = name.filter(|s| !s.is_empty());
        if metadata.is_empty() {
            self.config.sound_metadata.remove(path);
        }
        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    pub fn add_sound_tag(&mut self, path: &Path, tag: &str) {
        let metadata = self
            .config
            .sound_metadata
            .entry(path.to_path_buf())
            .or_default();
        metadata.add_tag(tag);
        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    pub fn remove_sound_tag(&mut self, path: &PathBuf, tag: &str) {
        if let Some(metadata) = self.config.sound_metadata.get_mut(path) {
            metadata.remove_tag(tag);
            if metadata.is_empty() {
                self.config.sound_metadata.remove(path);
            }
            if let Err(e) = self.config.save_to_file() {
                tracing::error!("Failed to save config: {}", e);
            }
        }
    }

    /// Persist a button colour for the sound (or clear with `None`).
    pub fn set_sound_color(&mut self, path: &Path, color_rgb: Option<u32>) {
        let metadata = self
            .config
            .sound_metadata
            .entry(path.to_path_buf())
            .or_default();
        metadata.color_rgb = color_rgb;
        if metadata.is_empty() {
            self.config.sound_metadata.remove(path);
        }
        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    /// Persist a Material-icon name for the sound (or clear with `None`).
    pub fn set_sound_icon(&mut self, path: &Path, icon: Option<String>) {
        let metadata = self
            .config
            .sound_metadata
            .entry(path.to_path_buf())
            .or_default();
        metadata.icon = icon.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        if metadata.is_empty() {
            self.config.sound_metadata.remove(path);
        }
        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    pub fn get_all_tags(&mut self) -> Vec<String> {
        const TTL: std::time::Duration = std::time::Duration::from_millis(500);
        let now = std::time::Instant::now();
        if let Some((when, list)) = &self.tags_cache {
            if now.duration_since(*when) < TTL {
                return list.clone();
            }
        }

        let mut all_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
        for metadata in self.config.sound_metadata.values() {
            all_tags.extend(metadata.tags.iter().cloned());
        }
        let mut tags: Vec<_> = all_tags.into_iter().collect();
        tags.sort();
        self.tags_cache = Some((now, tags.clone()));
        tags
    }

    /// Get the individual volume for a sound (None = use global volume)
    pub fn get_sound_volume(&self, path: &PathBuf) -> Option<f32> {
        self.config.sound_metadata.get(path).and_then(|m| m.volume)
    }

    /// Set the individual volume for a sound (None = use global volume)
    pub fn set_sound_volume(&mut self, path: &PathBuf, volume: Option<f32>) {
        let metadata = self.config.sound_metadata.entry(path.clone()).or_default();
        metadata.volume = volume.map(|v| v.clamp(0.0, 1.0));
        if metadata.is_empty() {
            self.config.sound_metadata.remove(path);
        }
        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    /// Import files into the sounds folder by copying them
    pub fn import_files(&mut self, files: Vec<PathBuf>) {
        let Some(sounds_folder) = self.config.sounds_folder.clone() else {
            tracing::error!("Sounds folder not configured");
            return;
        };

        // Ensure directory exists
        if !sounds_folder.exists() {
            if let Err(e) = std::fs::create_dir_all(&sounds_folder) {
                tracing::error!("Failed to create sounds folder: {}", e);
                return;
            }
        }

        let mut imported = 0;
        let mut skipped = 0;

        for file in &files {
            // Only process supported audio files
            let ext = file
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_lowercase();

            if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
                continue;
            }

            if let Some(filename) = file.file_name() {
                // Security: Sanitize filename to prevent path traversal
                let filename_str = filename.to_string_lossy();
                let safe_filename: String = filename_str
                    .chars()
                    .filter(|c| *c != '/' && *c != '\\' && *c != '\0')
                    .collect();

                if safe_filename.is_empty() || safe_filename.starts_with('.') {
                    tracing::error!("Skipping file with invalid name: {}", file.display());
                    skipped += 1;
                    continue;
                }

                let dest = sounds_folder.join(&safe_filename);

                // Handle duplicates - generate unique name
                let final_dest = self.get_unique_path(&dest);

                // Final safety check: ensure destination is within sounds folder
                // Use validate_path_within for consistent path validation
                let validated_dest = match validate_path_within(&final_dest, &sounds_folder) {
                    Some(p) => p,
                    None => {
                        tracing::error!(
                            "Security: Skipping file that would escape sounds folder: {}",
                            final_dest.display()
                        );
                        skipped += 1;
                        continue;
                    }
                };

                match std::fs::copy(file, &validated_dest) {
                    Ok(_) => {
                        imported += 1;
                    }
                    Err(e) => {
                        tracing::error!("Failed to copy {}: {}", file.display(), e);
                        skipped += 1;
                    }
                }
            }
        }

        tracing::error!(
            "Import complete: {} imported, {} skipped",
            imported,
            skipped
        );

        // Refresh file list if viewing "All Sounds" playlist
        if self.app_state.current_playlist.as_deref() == Some("All Sounds") {
            self.load_all_sounds();
        }
    }

    /// Generate unique path if file already exists
    fn get_unique_path(&self, path: &Path) -> PathBuf {
        unique_path(
            path,
            |p| p.exists(),
            || {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
            },
        )
    }

    /// Open sounds folder in system file manager
    pub fn open_sounds_folder(&self) {
        if let Some(ref path) = self.config.sounds_folder {
            let _ = open::that(path);
        }
    }

    /// Set the sounds folder path
    pub fn set_sounds_folder(&mut self, path: PathBuf) {
        // Create directory if it doesn't exist
        if !path.exists() {
            if let Err(e) = std::fs::create_dir_all(&path) {
                tracing::error!("Failed to create sounds folder {}: {}", path.display(), e);
            }
        }

        self.config.sounds_folder = Some(path.clone());
        if let Err(e) = self.config.save_to_file() {
            tracing::error!("Failed to save config: {}", e);
        }

        // Open the "All Sounds" playlist
        self.open_playlist("All Sounds");
    }

    /// Show dialog to select sounds folder
    pub fn pick_sounds_folder(&mut self) {
        let file_dialog = FileDialog::new();
        if let Some(path) = file_dialog.pick_folder() {
            self.set_sounds_folder(path);
        }
    }

    /// Import via file dialog (Add Sound button)
    pub fn import_sounds_dialog(&mut self) {
        let file_dialog = FileDialog::new().add_filter("Audio Files", &SUPPORTED_EXTENSIONS);

        if let Some(paths) = file_dialog.pick_files() {
            self.import_files(paths);
        }
    }

    /// Toggle the audio recorder. Saves into the configured sounds folder.
    /// Source (mic vs system) is read from `app_state.record_source`. After
    /// stopping, refreshes the "All Sounds" view if active.
    pub fn toggle_recording(&mut self) {
        use soundboard::types::gui::RecordSourceUi;
        use soundboard::utils::recorder::{self, RecordSource, ToggleOutcome};

        let Some(sounds_folder) = self.config.sounds_folder.clone() else {
            self.app_state.record_status_message =
                Some("Sounds folder is not set; configure it first.".to_string());
            return;
        };
        let source = match self.app_state.record_source {
            RecordSourceUi::Mic => RecordSource::Mic,
            RecordSourceUi::SystemLoopback => RecordSource::SystemLoopback,
        };
        match recorder::toggle(source, &sounds_folder) {
            Ok(ToggleOutcome::Started(path)) => {
                self.app_state.record_status_message =
                    Some(format!("Recording -> {}", path.display()));
            }
            Ok(ToggleOutcome::Stopped(Some(path))) => {
                self.app_state.record_status_message = Some(format!("Saved {}", path.display()));
                if self.app_state.current_playlist.as_deref() == Some("All Sounds") {
                    self.load_all_sounds();
                }
            }
            Ok(ToggleOutcome::Stopped(None)) => {
                self.app_state.record_status_message = Some("No recording was active.".to_string());
            }
            Err(e) => {
                tracing::error!("recorder toggle failed: {}", e);
                self.app_state.record_status_message = Some(format!("Recording failed: {}", e));
            }
        }
    }

    /// Kick off a background yt-dlp download. Validates URL, requires sounds
    /// folder to be set, and stores the receiver so `poll_url_grab_status`
    /// can update the UI.
    pub fn start_url_grab(&mut self, url: String) {
        use soundboard::utils::yt_grab;

        let Some(sounds_folder) = self.config.sounds_folder.clone() else {
            self.app_state.url_grab_status =
                Some("Sounds folder is not set. Configure it in settings first.".to_string());
            return;
        };
        if let Err(e) = yt_grab::validate_url(&url) {
            self.app_state.url_grab_status = Some(format!("Invalid URL: {}", e));
            return;
        }
        if !yt_grab::is_yt_dlp_available() {
            self.app_state.url_grab_status = Some(
                "yt-dlp is not installed or not on PATH. Install it from https://github.com/yt-dlp/yt-dlp/releases."
                    .to_string(),
            );
            return;
        }
        let (tx, rx) = mpsc::channel();
        yt_grab::spawn_grab(url, sounds_folder, tx);
        self.url_grab_rx = Some(rx);
        self.app_state.url_grab_in_progress = true;
        self.app_state.url_grab_status = Some("Downloading...".to_string());
    }

    /// Drain the yt-dlp status channel. On terminal status, refreshes the
    /// "All Sounds" view if active.
    pub fn poll_url_grab_status(&mut self) {
        let Some(rx) = self.url_grab_rx.as_ref() else {
            return;
        };
        let mut terminal = false;
        loop {
            match rx.try_recv() {
                Ok(soundboard::utils::yt_grab::GrabStatus::Starting) => {
                    self.app_state.url_grab_status = Some("Downloading...".to_string());
                }
                Ok(soundboard::utils::yt_grab::GrabStatus::Done(path)) => {
                    self.app_state.url_grab_status =
                        Some(format!("Downloaded: {}", path.display()));
                    terminal = true;
                }
                Ok(soundboard::utils::yt_grab::GrabStatus::Error(msg)) => {
                    self.app_state.url_grab_status = Some(format!("Failed: {}", msg));
                    terminal = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    terminal = true;
                    break;
                }
            }
        }
        if terminal {
            self.url_grab_rx = None;
            self.app_state.url_grab_in_progress = false;
            if self.app_state.current_playlist.as_deref() == Some("All Sounds") {
                self.load_all_sounds();
            }
        }
    }
}

pub async fn run() -> Result<()> {
    // Embed the daemon in this process. Failure is non-fatal: the state-sync
    // loop already polls `wait_for_daemon`, and a pre-existing `--headless`
    // instance is a legitimate "already running" case (we just connect to
    // it instead).
    if let Err(e) = soundboard::utils::daemon_run::start_in_process().await {
        tracing::warn!("Could not start embedded daemon: {e}");
    }

    const ICON: &[u8] = include_bytes!("../../assets/icon.png");

    let options = NativeOptions {
        vsync: true,
        centered: true,
        hardware_acceleration: HardwareAcceleration::Preferred,

        viewport: ViewportBuilder::default()
            .with_inner_size(Vec2::new(1200.0, 800.0))
            .with_min_inner_size(Vec2::new(800.0, 600.0))
            .with_icon(
                from_png_bytes(ICON)
                    .map_err(|e| soundboard::SoundboardError::Other(e.to_string()))?,
            ),

        ..Default::default()
    };

    match run_native(
        "Soundboard",
        options,
        Box::new(|cc| {
            egui_material_icons::initialize(&cc.egui_ctx);
            Ok(Box::new(SoundpadGui::new(&cc.egui_ctx)))
        }),
    ) {
        Ok(_) => {
            // Finalise any in-flight recording so the WAV header is written.
            // Otherwise the file is left without RIFF chunk sizes and is
            // unreadable.
            if let Err(e) = soundboard::utils::recorder::stop() {
                tracing::error!("failed to finalise recording on exit: {}", e);
            }
            let config = get_gui_config();
            if config.pause_on_exit {
                make_request_sync(Request::pause()).ok();
            }
            Ok(())
        }
        Err(e) => Err(soundboard::SoundboardError::Other(e.to_string())),
    }
}

/// Pure helper for unique-path generation. Generates a path that does not
/// collide according to `path_exists`, given a current-time provider for the
/// timestamp fallback. Extracted from `SoundpadGui::get_unique_path` so the
/// numbering / fallback logic is testable without filesystem state.
fn unique_path<E, N>(path: &Path, path_exists: E, now: N) -> PathBuf
where
    E: Fn(&Path) -> bool,
    N: FnOnce() -> std::time::Duration,
{
    if !path_exists(path) {
        return path.to_path_buf();
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str());
    let parent = path.parent().unwrap_or(std::path::Path::new("."));

    let make_filename = |base: String| -> String {
        match ext {
            Some(e) => format!("{}.{}", base, e),
            None => base,
        }
    };

    for i in 1..1000 {
        let new_name = make_filename(format!("{} ({})", stem, i));
        let new_path = parent.join(&new_name);
        if !path_exists(&new_path) {
            return new_path;
        }
    }

    let duration = now();
    let timestamp = format!("{}_{}", duration.as_secs(), duration.subsec_nanos());

    let fallback_name = make_filename(format!("{}_{}", stem, timestamp));
    let fallback_path = parent.join(&fallback_name);

    if path_exists(&fallback_path) {
        let random_suffix = (duration.as_nanos() % 100_000) as u32;
        let final_name = make_filename(format!("{}_{}_{}", stem, timestamp, random_suffix));
        return parent.join(&final_name);
    }

    fallback_path
}

#[cfg(test)]
mod tests {
    use super::unique_path;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    fn make_exists(taken: &[&str]) -> impl Fn(&Path) -> bool {
        let set: HashSet<PathBuf> = taken.iter().map(PathBuf::from).collect();
        move |p: &Path| set.contains(p)
    }

    fn fixed_now() -> impl FnOnce() -> Duration {
        || Duration::new(1_700_000_000, 123_456_789)
    }

    #[test]
    fn unique_path_returns_input_when_no_collision() {
        let exists = make_exists(&[]);
        let result = unique_path(Path::new("/tmp/sound.mp3"), exists, fixed_now());
        assert_eq!(result, PathBuf::from("/tmp/sound.mp3"));
    }

    #[test]
    fn unique_path_appends_one_on_first_collision() {
        let exists = make_exists(&["/tmp/sound.mp3"]);
        let result = unique_path(Path::new("/tmp/sound.mp3"), exists, fixed_now());
        assert_eq!(result, PathBuf::from("/tmp/sound (1).mp3"));
    }

    #[test]
    fn unique_path_increments_until_free() {
        let exists = make_exists(&[
            "/tmp/sound.mp3",
            "/tmp/sound (1).mp3",
            "/tmp/sound (2).mp3",
            "/tmp/sound (3).mp3",
        ]);
        let result = unique_path(Path::new("/tmp/sound.mp3"), exists, fixed_now());
        assert_eq!(result, PathBuf::from("/tmp/sound (4).mp3"));
    }

    #[test]
    fn unique_path_preserves_extension_with_dot() {
        let exists = make_exists(&["/tmp/clip.wav"]);
        let result = unique_path(Path::new("/tmp/clip.wav"), exists, fixed_now());
        assert_eq!(result.extension().and_then(|e| e.to_str()), Some("wav"));
        assert_eq!(
            result.file_stem().and_then(|s| s.to_str()),
            Some("clip (1)")
        );
    }

    #[test]
    fn unique_path_handles_no_extension() {
        let exists = make_exists(&["/tmp/noext"]);
        let result = unique_path(Path::new("/tmp/noext"), exists, fixed_now());
        assert_eq!(result, PathBuf::from("/tmp/noext (1)"));
        assert_eq!(result.extension(), None);
    }

    #[test]
    fn unique_path_handles_bare_filename() {
        // `Path::new("bare.mp3").parent()` returns Some("") (not None), so
        // join produces a relative path without an explicit "." prefix.
        let exists = make_exists(&["bare.mp3"]);
        let result = unique_path(Path::new("bare.mp3"), exists, fixed_now());
        assert_eq!(result, PathBuf::from("bare (1).mp3"));
    }

    #[test]
    fn unique_path_falls_back_to_timestamp_after_999_collisions() {
        // Simulate 1..1000 all taken: function must reach the timestamp branch
        // and return a non-numbered name containing the seconds value.
        let mut taken: Vec<String> = vec!["/tmp/sound.mp3".to_string()];
        for i in 1..1000 {
            taken.push(format!("/tmp/sound ({}).mp3", i));
        }
        let taken_refs: Vec<&str> = taken.iter().map(String::as_str).collect();
        let exists = make_exists(&taken_refs);
        let result = unique_path(Path::new("/tmp/sound.mp3"), exists, fixed_now());
        let name = result.file_name().and_then(|n| n.to_str()).unwrap();
        // Format: "sound_<secs>_<nanos>.mp3"
        assert!(
            name.starts_with("sound_1700000000_"),
            "expected timestamp-based name, got {}",
            name
        );
        assert!(name.ends_with(".mp3"), "expected .mp3 ext, got {}", name);
    }

    #[test]
    fn unique_path_appends_random_suffix_when_timestamp_also_taken() {
        // Force 1..1000 numbered names taken AND timestamp name taken; expect
        // a third format: stem_<secs>_<nanos>_<random>.ext
        let mut taken: Vec<String> = vec!["/tmp/sound.mp3".to_string()];
        for i in 1..1000 {
            taken.push(format!("/tmp/sound ({}).mp3", i));
        }
        // matches timestamp branch from fixed_now()
        taken.push("/tmp/sound_1700000000_123456789.mp3".to_string());
        let taken_refs: Vec<&str> = taken.iter().map(String::as_str).collect();
        let exists = make_exists(&taken_refs);
        let result = unique_path(Path::new("/tmp/sound.mp3"), exists, fixed_now());
        let name = result.file_name().and_then(|n| n.to_str()).unwrap();
        // Expected: sound_1700000000_123456789_<n>.mp3 where n = nanos % 100_000
        let expected_random = (1_700_000_000u128 * 1_000_000_000 + 123_456_789) % 100_000;
        let expected = format!("sound_1700000000_123456789_{}.mp3", expected_random as u32);
        assert_eq!(name, expected);
    }
}
