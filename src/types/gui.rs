use crate::types::audio_player::{LayerInfo, PlayerState};

use egui::Id;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[derive(Debug, Clone, Default)]
pub enum UpdateStatus {
    #[default]
    NotChecked,
    Checking,
    UpToDate,
    UpdateAvailable {
        latest_version: String,
        release_url: String,
        download_url: Option<String>,
    },
    Downloading {
        progress: f32,
    },
    Downloaded {
        file_path: PathBuf,
    },
    Error(String),
}

/// Which hotkey is currently being recorded
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyRecording {
    PlayPause,
    Stop,
}

#[derive(Default, Debug)]
pub struct AppState {
    pub search_query: String,

    pub position_slider_value: f32,
    pub volume_slider_value: f32,
    pub gain_slider_value: f32,
    pub mic_gain_slider_value: f32,

    pub position_dragged: bool,
    pub volume_dragged: bool,
    pub gain_dragged: bool,
    pub mic_gain_dragged: bool,

    pub show_settings: bool,

    pub current_dir: Option<PathBuf>,
    pub dirs: HashSet<PathBuf>,

    pub selected_file: Option<PathBuf>,
    pub files: HashSet<PathBuf>,

    pub search_field_id: Option<Id>,
    pub force_focus_id: Option<Id>,

    pub update_status: UpdateStatus,

    /// Currently recording hotkey (if any)
    pub recording_hotkey: Option<HotkeyRecording>,

    /// Currently selected category (if viewing a category instead of a directory)
    pub current_category: Option<String>,
    /// Whether the "new category" dialog is open
    pub show_new_category_dialog: bool,
    /// Input text for new category name
    pub new_category_name: String,
    /// Category being edited (for rename)
    pub editing_category: Option<String>,

    /// File being edited for metadata (shows popup)
    pub editing_metadata_file: Option<PathBuf>,
    /// Current tag input text
    pub tag_input: String,
    /// Search filter by tag (when set, only show files with this tag)
    pub filter_by_tag: Option<String>,
}

#[derive(Default, Debug, Clone)]
pub struct AudioPlayerState {
    pub state: PlayerState,
    pub new_state: Option<PlayerState>,
    pub current_file_path: PathBuf,

    pub is_paused: bool,
    pub looped: bool,

    pub volume: f32,
    pub new_volume: Option<f32>,
    pub gain: f32,
    pub new_gain: Option<f32>,
    pub mic_gain: f32,
    pub new_mic_gain: Option<f32>,
    pub position: f32,
    pub new_position: Option<f32>,
    pub duration: f32,

    pub current_input: String,
    pub all_inputs: HashMap<String, String>,
    pub current_output: String,
    pub all_outputs: HashMap<String, String>,

    pub layers: Vec<LayerInfo>,
}
