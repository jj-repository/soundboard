use crate::error::{Result, SoundboardError};
use crate::utils::config::get_config_path;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

/// Represents a configurable hotkey binding
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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

/// Hotkey configuration for all actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub play_pause: Option<HotkeyBinding>,
    pub stop: Option<HotkeyBinding>,
    pub enabled: bool,
    /// MIDI input bindings (separate from keyboard hotkeys).
    #[serde(default)]
    pub midi: MidiBindings,
    /// Gamepad input bindings (separate from keyboard hotkeys).
    #[serde(default)]
    pub gamepad: GamepadBindings,
    /// Hotkey that toggles recording (start a clip if idle, stop+save if
    /// recording). The capture source comes from the GUI session selector.
    #[serde(default)]
    pub record_toggle: Option<HotkeyBinding>,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            play_pause: Some(HotkeyBinding::new("KeyP", true, true, false, false)),
            stop: Some(HotkeyBinding::new("KeyS", true, true, false, false)),
            enabled: true,
            midi: MidiBindings::default(),
            gamepad: GamepadBindings::default(),
            record_toggle: None,
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
    /// 0xRRGGBB packed colour for the button background. `None` = theme default.
    #[serde(default)]
    pub color_rgb: Option<u32>,
    /// Material-icon name (e.g. "music_note", "favorite"). `None` = no icon.
    #[serde(default)]
    pub icon: Option<String>,
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
        self.custom_name.is_none()
            && self.description.is_none()
            && self.tags.is_empty()
            && self.volume.is_none()
            && self.color_rgb.is_none()
            && self.icon.is_none()
    }
}

/// MIDI input bindings. Off by default; enable to listen on all detected
/// MIDI input ports for note-on events that map to Play/Pause and Stop.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MidiBindings {
    #[serde(default)]
    pub enabled: bool,
    /// MIDI note number (0..=127) that triggers Play/Pause. e.g. 60 = C4.
    #[serde(default)]
    pub play_pause_note: Option<u8>,
    /// MIDI note number (0..=127) that triggers Stop.
    #[serde(default)]
    pub stop_note: Option<u8>,
}

/// Gamepad input bindings. Off by default; enable to map a controller
/// button to Play/Pause and Stop. Button names follow gilrs's enum
/// (e.g. "south"/"a", "east"/"b", "start").
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GamepadBindings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub play_pause_button: Option<String>,
    #[serde(default)]
    pub stop_button: Option<String>,
}

/// Auto Push-to-Talk: simulate holding a key while audio plays so PTT-gated
/// voice-chat apps actually transmit the soundboard's virtual-mic output.
/// Off by default — synthetic input is dual-use and may be flagged by some
/// kernel-level anti-cheats. Document this in the UI before enabling.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoPttConfig {
    /// Master toggle.
    #[serde(default)]
    pub enabled: bool,
    /// Key code to hold while playing, in the same vocabulary as
    /// `HotkeyBinding::key` (e.g. `"KeyV"`, `"F12"`, `"Space"`). `None`
    /// disables the press regardless of `enabled`.
    #[serde(default)]
    pub key: Option<String>,
}

impl AutoPttConfig {
    /// Const-constructible default for use in `static` initialization.
    pub const DISABLED: Self = Self {
        enabled: false,
        key: None,
    };
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub default_input_name: Option<String>,
    pub default_output_name: Option<String>,
    pub default_volume: Option<f32>,
    pub default_gain: Option<f32>,
    pub default_mic_gain: Option<f32>,
    #[serde(default)]
    pub auto_ptt: AutoPttConfig,
}

impl DaemonConfig {
    pub fn save_to_file(&self) -> Result<()> {
        self.save_to_path(&get_config_path()?.join("daemon.json"))
    }

    pub fn load_from_file() -> Result<DaemonConfig> {
        Self::load_from_path(&get_config_path()?.join("daemon.json"))
    }

    /// FS-injected save: takes the destination path so unit tests can hit a
    /// tempdir instead of the user's real config directory.
    pub fn save_to_path(&self, config_path: &Path) -> Result<()> {
        let config_dir = config_path.parent().ok_or_else(|| {
            SoundboardError::Config(format!(
                "failed to get parent for config path: {}",
                config_path.display()
            ))
        })?;
        if !config_dir.exists() {
            fs::create_dir_all(config_dir)?;
        }
        let config_json = serde_json::to_string_pretty(self)?;
        fs::write(config_path, config_json.as_bytes())?;
        Ok(())
    }

    pub fn load_from_path(config_path: &Path) -> Result<DaemonConfig> {
        let bytes = fs::read(config_path)?;
        Ok(serde_json::from_slice::<DaemonConfig>(&bytes)?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Width of the sidebar (playlists panel) in pixels
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
    /// Order of playlists (by name)
    #[serde(default)]
    pub playlist_order: Vec<String>,
    /// Whether to automatically check for updates on startup
    #[serde(default = "default_auto_check_updates")]
    pub auto_check_updates: bool,
    /// User dismissed the VB-Cable install prompt; do not nag again on launch.
    #[serde(default)]
    pub vbcable_install_skipped: bool,
    /// File-list view mode: list (default) or grid (cart-wall buttons).
    #[serde(default)]
    pub view_mode: ViewMode,
}

/// File-list rendering style. `List` = compact one-row-per-sound (default),
/// `Grid` = wrapped colour-coded buttons (cart-wall style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

fn default_auto_check_updates() -> bool {
    false
}

fn default_sidebar_width() -> f32 {
    200.0
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
            sidebar_width: default_sidebar_width(),
            playlist_order: Vec::new(),
            auto_check_updates: default_auto_check_updates(),
            vbcable_install_skipped: false,
            view_mode: ViewMode::default(),
        }
    }
}

impl GuiConfig {
    pub fn save_to_file(&mut self) -> Result<()> {
        self.save_to_path(&get_config_path()?.join("gui.json"))
    }

    pub fn load_from_file() -> Result<GuiConfig> {
        Self::load_from_path(&get_config_path()?.join("gui.json"))
    }

    /// FS-injected save: takes the destination path so unit tests can hit a
    /// tempdir instead of the user's real config directory.
    pub fn save_to_path(&mut self, config_path: &Path) -> Result<()> {
        let config_dir = config_path.parent().ok_or_else(|| {
            SoundboardError::Config(format!(
                "failed to get parent for config path: {}",
                config_path.display()
            ))
        })?;
        if !config_dir.exists() {
            fs::create_dir_all(config_dir)?;
        }
        if !self.save_scale_factor {
            self.scale_factor = 1.0;
        }
        let config_json = serde_json::to_string_pretty(self)?;
        fs::write(config_path, config_json.as_bytes())?;
        Ok(())
    }

    pub fn load_from_path(config_path: &Path) -> Result<GuiConfig> {
        let bytes = fs::read(config_path)?;
        Ok(serde_json::from_slice::<GuiConfig>(&bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SoundCategory tests (TEST-04) ---

    #[test]
    fn test_sound_category_add_prevents_duplicates() {
        let mut cat = SoundCategory::new("test");
        let path = PathBuf::from("/sounds/beep.mp3");
        cat.add_sound(path.clone());
        cat.add_sound(path.clone());
        assert_eq!(cat.sounds.len(), 1, "duplicate should be prevented");
    }

    #[test]
    fn test_sound_category_remove_existing() {
        let mut cat = SoundCategory::new("test");
        let path = PathBuf::from("/sounds/beep.mp3");
        cat.add_sound(path.clone());
        cat.remove_sound(&path);
        assert!(cat.sounds.is_empty());
    }

    #[test]
    fn test_sound_category_remove_nonexistent() {
        let mut cat = SoundCategory::new("test");
        cat.add_sound(PathBuf::from("/sounds/a.mp3"));
        cat.remove_sound(&PathBuf::from("/sounds/b.mp3"));
        assert_eq!(cat.sounds.len(), 1, "removing nonexistent should be no-op");
    }

    #[test]
    fn test_sound_category_contains() {
        let mut cat = SoundCategory::new("test");
        let path = PathBuf::from("/sounds/beep.mp3");
        assert!(!cat.contains(&path));
        cat.add_sound(path.clone());
        assert!(cat.contains(&path));
    }

    #[test]
    fn test_sound_category_preserves_order() {
        let mut cat = SoundCategory::new("test");
        let paths: Vec<PathBuf> = (0..5)
            .map(|i| PathBuf::from(format!("/sounds/{}.mp3", i)))
            .collect();
        for p in &paths {
            cat.add_sound(p.clone());
        }
        assert_eq!(cat.sounds, paths);
    }

    // --- SoundMetadata tag tests (TEST-08) ---

    #[test]
    fn test_metadata_tag_case_insensitive() {
        let mut meta = SoundMetadata::new();
        meta.add_tag("FUNNY");
        assert!(meta.has_tag("funny"));
        assert!(meta.has_tag("FUNNY"));
        assert!(meta.has_tag("Funny"));
    }

    #[test]
    fn test_metadata_tag_trimming() {
        let mut meta = SoundMetadata::new();
        meta.add_tag("  foo  ");
        assert!(meta.has_tag("foo"));
        assert_eq!(meta.tags.len(), 1);
    }

    #[test]
    fn test_metadata_tag_empty_rejected() {
        let mut meta = SoundMetadata::new();
        meta.add_tag("");
        meta.add_tag("   ");
        assert!(meta.tags.is_empty());
    }

    #[test]
    fn test_metadata_tag_duplicate_prevented() {
        let mut meta = SoundMetadata::new();
        meta.add_tag("meme");
        meta.add_tag("Meme");
        meta.add_tag("MEME");
        assert_eq!(meta.tags.len(), 1);
    }

    #[test]
    fn test_metadata_remove_tag() {
        let mut meta = SoundMetadata::new();
        meta.add_tag("test");
        meta.remove_tag("TEST");
        assert!(meta.tags.is_empty());
    }

    #[test]
    fn test_metadata_is_empty() {
        let meta = SoundMetadata::new();
        assert!(meta.is_empty());

        let mut meta2 = SoundMetadata::new();
        meta2.add_tag("tag");
        assert!(!meta2.is_empty());
    }

    // --- Config serialization roundtrip tests (TEST-03) ---

    #[test]
    fn test_daemon_config_roundtrip() {
        let config = DaemonConfig {
            default_input_name: Some("mic1".to_string()),
            default_output_name: Some("speakers".to_string()),
            default_volume: Some(0.75),
            default_gain: Some(1.5),
            default_mic_gain: Some(2.0),
            auto_ptt: AutoPttConfig::default(),
        };

        let json = serde_json::to_string(&config).expect("serialize");
        let loaded: DaemonConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(loaded.default_input_name, config.default_input_name);
        assert_eq!(loaded.default_output_name, config.default_output_name);
        assert_eq!(loaded.default_volume, config.default_volume);
        assert_eq!(loaded.default_gain, config.default_gain);
        assert_eq!(loaded.default_mic_gain, config.default_mic_gain);
    }

    #[test]
    fn test_daemon_config_missing_fields_use_defaults() {
        let json = "{}";
        let config: DaemonConfig = serde_json::from_str(json).expect("deserialize empty");
        assert!(config.default_input_name.is_none());
        assert!(config.default_volume.is_none());
    }

    #[test]
    fn test_gui_config_roundtrip() {
        let config = GuiConfig {
            save_volume: true,
            scale_factor: 1.5,
            sounds_folder: Some(PathBuf::from("/home/user/sounds")),
            sidebar_width: 250.0,
            ..Default::default()
        };

        let json = serde_json::to_string(&config).expect("serialize");
        let loaded: GuiConfig = serde_json::from_str(&json).expect("deserialize");

        assert!(loaded.save_volume);
        assert_eq!(loaded.scale_factor, 1.5);
        assert_eq!(
            loaded.sounds_folder,
            Some(PathBuf::from("/home/user/sounds"))
        );
        assert_eq!(loaded.sidebar_width, 250.0);
    }

    #[test]
    fn test_gui_config_missing_fields_use_defaults() {
        // Minimal JSON with only required fields
        let json = r#"{"scale_factor":1.0,"save_volume":false,"save_gain":false,"save_mic_gain":false,"save_input":false,"save_scale_factor":false,"pause_on_exit":false,"dirs":[]}"#;
        let config: GuiConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.sidebar_width, default_sidebar_width());
        assert_eq!(config.auto_check_updates, default_auto_check_updates());
        assert!(config.categories.is_empty());
        assert!(config.sounds_folder.is_none());
    }

    #[test]
    fn test_gui_config_corrupt_json_fails() {
        let result = serde_json::from_str::<GuiConfig>("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_hotkey_binding_display_all_modifiers() {
        let binding = HotkeyBinding::new("KeyP", true, true, true, true);
        assert_eq!(binding.display(), "Ctrl+Shift+Alt+Super+KeyP");
    }

    #[test]
    fn test_hotkey_binding_display_no_modifiers() {
        let binding = HotkeyBinding::new("Space", false, false, false, false);
        assert_eq!(binding.display(), "Space");
    }

    #[test]
    fn test_hotkey_config_default() {
        let config = HotkeyConfig::default();
        assert!(config.enabled);
        assert!(config.play_pause.is_some());
        assert!(config.stop.is_some());
    }

    #[test]
    fn test_sound_category_serialization_roundtrip() {
        let mut cat = SoundCategory::new("My Playlist");
        cat.add_sound(PathBuf::from("/sounds/a.mp3"));
        cat.add_sound(PathBuf::from("/sounds/b.wav"));

        let json = serde_json::to_string(&cat).expect("serialize");
        let loaded: SoundCategory = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(loaded.name, "My Playlist");
        assert_eq!(loaded.sounds.len(), 2);
        assert_eq!(loaded.sounds[0], PathBuf::from("/sounds/a.mp3"));
    }

    // --- Config validation tests (ARCH-20) ---

    #[test]
    fn test_gui_config_default_is_valid() {
        let config = GuiConfig::default();
        assert!(config.scale_factor > 0.0);
        assert!(config.sidebar_width > 0.0);
        assert!(config.categories.is_empty());
        assert!(config.playlist_order.is_empty());
    }

    #[test]
    fn test_daemon_config_default_all_none() {
        let config = DaemonConfig::default();
        assert!(config.default_input_name.is_none());
        assert!(config.default_output_name.is_none());
        assert!(config.default_volume.is_none());
        assert!(config.default_gain.is_none());
        assert!(config.default_mic_gain.is_none());
    }

    #[test]
    fn test_gui_config_scale_factor_clamping_values() {
        // Verify serde accepts edge values
        let json = r#"{"scale_factor":0.5,"save_volume":false,"save_gain":false,"save_mic_gain":false,"save_input":false,"save_scale_factor":false,"pause_on_exit":false,"dirs":[]}"#;
        let config: GuiConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.scale_factor, 0.5);
    }

    #[test]
    fn test_sound_metadata_volume_serialization() {
        let mut meta = SoundMetadata::new();
        meta.volume = Some(0.75);
        let json = serde_json::to_string(&meta).unwrap();
        let loaded: SoundMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.volume, Some(0.75));
    }

    #[test]
    fn test_hotkey_binding_serialization_roundtrip() {
        let binding = HotkeyBinding::new("KeyP", true, true, false, false);
        let json = serde_json::to_string(&binding).unwrap();
        let loaded: HotkeyBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, binding);
    }

    #[test]
    fn test_hotkey_config_serialization_with_none() {
        let config = HotkeyConfig {
            play_pause: None,
            stop: None,
            enabled: false,
            midi: MidiBindings::default(),
            gamepad: GamepadBindings::default(),
            record_toggle: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        let loaded: HotkeyConfig = serde_json::from_str(&json).unwrap();
        assert!(loaded.play_pause.is_none());
        assert!(!loaded.enabled);
    }

    // --- Real-filesystem roundtrip (TEST-03) ---

    #[test]
    fn daemon_config_real_fs_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.json");
        let original = DaemonConfig {
            default_input_name: Some("HyperX".to_string()),
            default_output_name: Some("VB-Cable".to_string()),
            default_volume: Some(0.6),
            default_gain: Some(1.2),
            default_mic_gain: Some(1.5),
            auto_ptt: AutoPttConfig::default(),
        };
        original.save_to_path(&path).unwrap();
        let loaded = DaemonConfig::load_from_path(&path).unwrap();
        assert_eq!(loaded.default_input_name, original.default_input_name);
        assert_eq!(loaded.default_volume, original.default_volume);
        assert_eq!(loaded.default_gain, original.default_gain);
    }

    #[test]
    fn daemon_config_save_creates_missing_parent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/created/daemon.json");
        DaemonConfig::default().save_to_path(&path).unwrap();
        assert!(path.is_file());
    }

    #[test]
    fn daemon_config_load_missing_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.json");
        let err = DaemonConfig::load_from_path(&path).unwrap_err();
        assert!(matches!(err, SoundboardError::Io(_)));
    }

    #[test]
    fn gui_config_real_fs_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.json");
        let mut original = GuiConfig {
            save_scale_factor: true,
            scale_factor: 1.25,
            sounds_folder: Some(PathBuf::from("/tmp/sounds")),
            ..Default::default()
        };
        original.save_to_path(&path).unwrap();
        let loaded = GuiConfig::load_from_path(&path).unwrap();
        assert_eq!(loaded.scale_factor, 1.25);
        assert_eq!(loaded.sounds_folder, Some(PathBuf::from("/tmp/sounds")));
    }

    #[test]
    fn gui_config_save_resets_scale_when_save_scale_factor_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.json");
        let mut config = GuiConfig {
            save_scale_factor: false,
            scale_factor: 2.0,
            ..Default::default()
        };
        config.save_to_path(&path).unwrap();
        assert_eq!(
            config.scale_factor, 1.0,
            "scale must be reset on save when save_scale_factor=false"
        );
        let loaded = GuiConfig::load_from_path(&path).unwrap();
        assert_eq!(loaded.scale_factor, 1.0);
    }

    #[test]
    fn gui_config_load_corrupt_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.json");
        std::fs::write(&path, b"{ not valid json").unwrap();
        let err = GuiConfig::load_from_path(&path).unwrap_err();
        assert!(matches!(err, SoundboardError::Json(_)));
    }
}
