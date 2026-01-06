use crate::types::audio_player::PlayerState;

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
}
