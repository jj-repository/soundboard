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
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::{
    error::Error,
    sync::{Arc, Mutex, MutexGuard},
    thread,
};

/// Extension trait for Mutex that handles poisoning gracefully
trait MutexExt<T> {
    /// Lock the mutex, recovering from poison if necessary.
    /// This is preferred over `.lock().unwrap()` as it won't panic on poisoned locks.
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

const SUPPORTED_EXTENSIONS: [&str; 13] = [
    "mp3", "wav", "ogg", "flac", "mp4", "m4a", "aac", "mov", "mkv", "webm", "avi", "opus", "wma",
];

/// Validates that a path is safely within an allowed base directory.
/// Returns the canonicalized path if valid, or None if the path escapes the base directory.
fn validate_path_within(path: &Path, base_dir: &Path) -> Option<PathBuf> {
    // Canonicalize both paths to resolve symlinks and normalize
    let canonical_base = match base_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return None,
    };

    let canonical_path = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // If path doesn't exist yet, check the parent directory
            if let Some(parent) = path.parent() {
                let canonical_parent = parent.canonicalize().ok()?;
                if !canonical_parent.starts_with(&canonical_base) {
                    return None;
                }
                // Return the original path since file doesn't exist yet
                return Some(path.to_path_buf());
            }
            return None;
        }
    };

    // Check if the canonical path is within the base directory
    if canonical_path.starts_with(&canonical_base) {
        Some(canonical_path)
    } else {
        None
    }
}

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
            while let Ok(msg) = tray.receiver.try_recv() {
                msgs.push(msg);
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
            while let Ok(action) = hk.receiver.try_recv() {
                acts.push(action);
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
            Ok(entries) => entries.filter_map(|res| res.ok()).map(|e| e.path()).collect(),
            Err(e) => {
                eprintln!("Failed to read directory {}: {}", path.display(), e);
                Default::default()
            }
        };
    }

    pub fn play_file(&mut self, path: &PathBuf) {
        // Apply per-sound volume if set
        if let Some(sound_volume) = self.get_sound_volume(path) {
            make_request_sync(Request::set_volume(sound_volume)).ok();
            // Update local state to reflect the volume change
            let mut guard = self.audio_player_state_shared.lock_or_recover();
            guard.volume = sound_volume;
            guard.new_volume = Some(sound_volume);
        }

        if let Some(path_str) = path.to_str() {
            if let Err(e) = make_request_sync(Request::play(path_str)) {
                eprintln!("Failed to send play request: {}", e);
            }
        } else {
            eprintln!("Invalid file path encoding");
        }
    }

    pub fn preview_file(&mut self, path: &Path) {
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
        let mut guard = self.audio_player_state_shared.lock_or_recover();
        guard.new_state = Some(PlayerState::Stopped);
        guard.state = PlayerState::Stopped;
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
        self.config.save_to_file().ok();
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
            self.config.save_to_file().ok();
        }
    }

    pub fn remove_from_category(&mut self, category_name: &str, path: &PathBuf) {
        if let Some(category) = self.config.categories.get_mut(category_name) {
            category.remove_sound(path);
            self.config.save_to_file().ok();
        }
    }

    // ============= Playlist Methods =============

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
        if let Some(ref sounds_folder) = self.config.sounds_folder
            && let Ok(entries) = sounds_folder.read_dir() {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        // Check if it's a supported audio file
                        if let Some(ext) = path.extension().and_then(|e| e.to_str())
                            && SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                                self.app_state.files.insert(path);
                            }
                    }
                }
            }
        self.invalidate_files_cache();
    }

    /// Create a new playlist
    pub fn create_playlist(&mut self, name: &str) {
        if !name.is_empty() && name != "All Sounds" && name != "Favourites" && !self.config.categories.contains_key(name) {
            use pwsp::types::config::SoundCategory;
            self.config.categories.insert(name.to_string(), SoundCategory::new(name));
            // Add to playlist order
            self.config.playlist_order.push(name.to_string());
            self.config.save_to_file().ok();
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
        self.config.save_to_file().ok();
    }

    /// Rename a playlist
    pub fn rename_playlist(&mut self, old_name: &str, new_name: &str) {
        if !new_name.is_empty() && new_name != "All Sounds" && !self.config.categories.contains_key(new_name)
            && let Some(mut playlist) = self.config.categories.remove(old_name) {
                playlist.name = new_name.to_string();
                self.config.categories.insert(new_name.to_string(), playlist);
                if self.app_state.current_playlist.as_deref() == Some(old_name) {
                    self.app_state.current_playlist = Some(new_name.to_string());
                }
                // Update playlist order
                if let Some(pos) = self.config.playlist_order.iter().position(|n| n == old_name) {
                    self.config.playlist_order[pos] = new_name.to_string();
                }
                self.config.save_to_file().ok();
            }
    }

    /// Get playlists in their configured order
    pub fn get_ordered_playlists(&self) -> Vec<String> {
        let mut ordered = Vec::new();

        // First, add playlists in the configured order
        for name in &self.config.playlist_order {
            if self.config.categories.contains_key(name) {
                ordered.push(name.clone());
            }
        }

        // Then, add any playlists not in the order list (sorted alphabetically)
        let mut remaining: Vec<_> = self.config.categories.keys()
            .filter(|k| !self.config.playlist_order.contains(k))
            .cloned()
            .collect();
        remaining.sort();
        ordered.extend(remaining);

        ordered
    }

    /// Move a playlist from one position to another
    pub fn move_playlist(&mut self, from_index: usize, to_index: usize) {
        // Ensure playlist_order contains all playlists
        self.sync_playlist_order();

        if from_index >= self.config.playlist_order.len() || to_index >= self.config.playlist_order.len() {
            return;
        }

        let playlist = self.config.playlist_order.remove(from_index);
        self.config.playlist_order.insert(to_index, playlist);
        self.config.save_to_file().ok();
    }

    /// Sync playlist_order to contain all playlists
    fn sync_playlist_order(&mut self) {
        // Remove entries that no longer exist
        self.config.playlist_order.retain(|name| self.config.categories.contains_key(name));

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
            self.config.save_to_file().ok();
        }
    }

    /// Move a sound within a playlist from one position to another
    pub fn move_sound_in_playlist(&mut self, playlist_name: &str, from_index: usize, to_index: usize) {
        if playlist_name == "All Sounds" || playlist_name == "Favourites" {
            return; // Can't reorder these virtual playlists
        }
        if let Some(playlist) = self.config.categories.get_mut(playlist_name) {
            if from_index >= playlist.sounds.len() || to_index >= playlist.sounds.len() {
                return;
            }
            let sound = playlist.sounds.remove(from_index);
            playlist.sounds.insert(to_index, sound);
            self.config.save_to_file().ok();
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
                self.config.save_to_file().ok();
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
            eprintln!("Cannot delete file: sounds folder not configured");
            return;
        };

        let validated_path = match validate_path_within(path, sounds_folder) {
            Some(p) => p,
            None => {
                eprintln!(
                    "Security: Refusing to delete file outside sounds folder: {}",
                    path.display()
                );
                return;
            }
        };

        // Delete the file
        if let Err(e) = std::fs::remove_file(&validated_path) {
            eprintln!("Failed to delete file {}: {}", validated_path.display(), e);
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

        self.config.save_to_file().ok();

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
        self.config.save_to_file().ok();
    }

    pub fn add_sound_tag(&mut self, path: &Path, tag: &str) {
        let metadata = self.config.sound_metadata.entry(path.to_path_buf()).or_default();
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
        self.config.save_to_file().ok();
    }

    // ============= Sounds Folder Methods =============

    /// Import files into the sounds folder by copying them
    pub fn import_files(&mut self, files: Vec<PathBuf>) {
        let Some(sounds_folder) = self.config.sounds_folder.clone() else {
            eprintln!("Sounds folder not configured");
            return;
        };

        // Ensure directory exists
        if !sounds_folder.exists()
            && let Err(e) = std::fs::create_dir_all(&sounds_folder) {
                eprintln!("Failed to create sounds folder: {}", e);
                return;
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
                    eprintln!("Skipping file with invalid name: {}", file.display());
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
                        eprintln!(
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
                        println!("Imported: {}", validated_dest.display());
                    }
                    Err(e) => {
                        eprintln!("Failed to copy {}: {}", file.display(), e);
                        skipped += 1;
                    }
                }
            }
        }

        println!("Import complete: {} imported, {} skipped", imported, skipped);

        // Refresh file list if viewing "All Sounds" playlist
        if self.app_state.current_playlist.as_deref() == Some("All Sounds") {
            self.load_all_sounds();
        }
    }

    /// Generate unique path if file already exists
    fn get_unique_path(&self, path: &Path) -> PathBuf {
        if !path.exists() {
            return path.to_path_buf();
        }

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = path.extension().and_then(|e| e.to_str());
        let parent = path.parent().unwrap_or(std::path::Path::new("."));

        // Helper to build filename with optional extension
        let make_filename = |base: String| -> String {
            match ext {
                Some(e) => format!("{}.{}", base, e),
                None => base,
            }
        };

        // Try numbered suffixes first
        for i in 1..1000 {
            let new_name = make_filename(format!("{} ({})", stem, i));
            let new_path = parent.join(&new_name);
            if !new_path.exists() {
                return new_path;
            }
        }

        // Fallback: use timestamp with nanoseconds for uniqueness
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let timestamp = format!("{}_{}", duration.as_secs(), duration.subsec_nanos());

        let fallback_name = make_filename(format!("{}_{}", stem, timestamp));
        let fallback_path = parent.join(&fallback_name);

        // If even the timestamp-based name exists (very unlikely), add random suffix
        if fallback_path.exists() {
            let random_suffix: u32 = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| (d.as_nanos() % 100000) as u32)
                .unwrap_or(0);
            let final_name = make_filename(format!("{}_{}_{}", stem, timestamp, random_suffix));
            return parent.join(&final_name);
        }

        fallback_path
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
            std::fs::create_dir_all(&path).ok();
        }

        self.config.sounds_folder = Some(path.clone());
        self.config.save_to_file().ok();

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
