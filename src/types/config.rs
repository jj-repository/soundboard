use crate::utils::config::get_config_path;
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, error::Error, fs, path::PathBuf};

/// Represents a configurable hotkey binding
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeyBinding {
    /// Key code (e.g., "KeyP", "KeyS", "Space")
    pub key: String,
    /// Whether Ctrl modifier is required
    pub ctrl: bool,
    /// Whether Shift modifier is required
    pub shift: bool,
    /// Whether Alt modifier is required
    pub alt: bool,
    /// Whether Super/Meta modifier is required
    pub super_key: bool,
}

impl HotkeyBinding {
    pub fn new(key: &str, ctrl: bool, shift: bool, alt: bool, super_key: bool) -> Self {
        Self {
            key: key.to_string(),
            ctrl,
            shift,
            alt,
            super_key,
        }
    }

    /// Format as human-readable string
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.super_key {
            parts.push("Super");
        }
        parts.push(&self.key);
        parts.join("+")
    }
}

impl Default for HotkeyBinding {
    fn default() -> Self {
        Self {
            key: String::new(),
            ctrl: false,
            shift: false,
            alt: false,
            super_key: false,
        }
    }
}

/// Hotkey configuration for all actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub play_pause: Option<HotkeyBinding>,
    pub stop: Option<HotkeyBinding>,
    pub enabled: bool,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            play_pause: Some(HotkeyBinding::new("KeyP", true, true, false, false)),
            stop: Some(HotkeyBinding::new("KeyS", true, true, false, false)),
            enabled: true,
        }
    }
}

/// A sound category/playlist containing a collection of sound files
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SoundCategory {
    /// Display name for the category
    pub name: String,
    /// Ordered list of sound file paths in this category
    pub sounds: Vec<PathBuf>,
}

impl SoundCategory {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            sounds: Vec::new(),
        }
    }

    pub fn add_sound(&mut self, path: PathBuf) {
        if !self.sounds.contains(&path) {
            self.sounds.push(path);
        }
    }

    pub fn remove_sound(&mut self, path: &PathBuf) {
        self.sounds.retain(|p| p != path);
    }

    pub fn contains(&self, path: &PathBuf) -> bool {
        self.sounds.contains(path)
    }
}

/// Metadata for a sound file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SoundMetadata {
    /// Custom display name (if different from filename)
    #[serde(default)]
    pub custom_name: Option<String>,
    /// Description or notes about the sound
    #[serde(default)]
    pub description: Option<String>,
    /// Tags for filtering/organizing
    #[serde(default)]
    pub tags: HashSet<String>,
    /// Individual volume for this sound (0.0 to 1.0, None = use global volume)
    #[serde(default)]
    pub volume: Option<f32>,
}

impl SoundMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_tag(&mut self, tag: &str) {
        let tag = tag.trim().to_lowercase();
        if !tag.is_empty() {
            self.tags.insert(tag);
        }
    }

    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.remove(&tag.to_lowercase());
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(&tag.to_lowercase())
    }

    pub fn is_empty(&self) -> bool {
        self.custom_name.is_none() && self.description.is_none() && self.tags.is_empty() && self.volume.is_none()
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub default_input_name: Option<String>,
    pub default_output_name: Option<String>,
    pub default_volume: Option<f32>,
    pub default_gain: Option<f32>,
    pub default_mic_gain: Option<f32>,
}

impl DaemonConfig {
    pub fn save_to_file(&self) -> Result<(), Box<dyn Error>> {
        let config_path = get_config_path()?.join("daemon.json");
        let config_dir = config_path
            .parent()
            .ok_or("Failed to get config parent directory")?;

        if !config_dir.exists() {
            fs::create_dir_all(config_dir)?;
        }

        let config_json = serde_json::to_string_pretty(self)?;
        fs::write(config_path, config_json.as_bytes())?;
        Ok(())
    }

    pub fn load_from_file() -> Result<DaemonConfig, Box<dyn Error>> {
        let config_path = get_config_path()?.join("daemon.json");
        let bytes = fs::read(config_path)?;
        Ok(serde_json::from_slice::<DaemonConfig>(&bytes)?)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GuiConfig {
    pub scale_factor: f32,

    pub save_volume: bool,
    pub save_gain: bool,
    pub save_mic_gain: bool,
    pub save_input: bool,
    pub save_scale_factor: bool,
    pub pause_on_exit: bool,

    pub dirs: HashSet<PathBuf>,
    #[serde(default)]
    pub favorites: HashSet<PathBuf>,
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
    #[serde(default)]
    pub categories: HashMap<String, SoundCategory>,
    #[serde(default)]
    pub sound_metadata: HashMap<PathBuf, SoundMetadata>,
    /// Path to the centralized sounds folder (None if not configured)
    #[serde(default)]
    pub sounds_folder: Option<PathBuf>,
}

impl Default for GuiConfig {
    fn default() -> Self {
        GuiConfig {
            scale_factor: 1.0,

            save_volume: false,
            save_gain: false,
            save_mic_gain: false,
            save_input: false,
            save_scale_factor: false,
            pause_on_exit: false,

            dirs: HashSet::default(),
            favorites: HashSet::default(),
            hotkeys: HotkeyConfig::default(),
            categories: HashMap::default(),
            sound_metadata: HashMap::default(),
            sounds_folder: None,
        }
    }
}

impl GuiConfig {
    pub fn save_to_file(&mut self) -> Result<(), Box<dyn Error>> {
        let config_path = get_config_path()?.join("gui.json");
        let config_dir = config_path
            .parent()
            .ok_or("Failed to get config parent directory")?;

        if !config_dir.exists() {
            fs::create_dir_all(config_dir)?;
        }

        // Do not save scale factor if user does not want to
        if !self.save_scale_factor {
            self.scale_factor = 1.0;
        }

        let config_json = serde_json::to_string_pretty(self)?;
        fs::write(config_path, config_json.as_bytes())?;
        Ok(())
    }

    pub fn load_from_file() -> Result<GuiConfig, Box<dyn Error>> {
        let config_path = get_config_path()?.join("gui.json");
        let bytes = fs::read(config_path)?;
        Ok(serde_json::from_slice::<GuiConfig>(&bytes)?)
    }
}
