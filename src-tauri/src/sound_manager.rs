use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sound {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub volume: f32,
    pub hotkey: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundboardState {
    pub sounds: Vec<Sound>,
    pub current_folder: Option<PathBuf>,
    pub master_volume: f32,
    pub system_audio_routing_enabled: bool,
}

pub struct SoundManager {
    state: Arc<RwLock<SoundboardState>>,
}

impl SoundManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(SoundboardState {
                sounds: Vec::new(),
                current_folder: None,
                master_volume: 1.0,
                system_audio_routing_enabled: false,
            })),
        }
    }

    pub fn load_folder(&self, folder: PathBuf) -> anyhow::Result<()> {
        let mut sounds = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        let ext = ext.to_string_lossy().to_lowercase();
                        if matches!(
                            ext.as_str(),
                            "mp3" | "wav" | "ogg" | "flac" | "m4a" | "aac"
                        ) {
                            let name = path
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            let id = uuid::Uuid::new_v4().to_string();

                            sounds.push(Sound {
                                id,
                                name,
                                path: path.clone(),
                                volume: 1.0,
                                hotkey: None,
                            });
                        }
                    }
                }
            }
        }

        let mut state = self.state.write();
        state.sounds = sounds;
        state.current_folder = Some(folder);

        Ok(())
    }

    pub fn add_sound(&self, path: PathBuf) -> anyhow::Result<Sound> {
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let id = uuid::Uuid::new_v4().to_string();

        let sound = Sound {
            id,
            name,
            path,
            volume: 1.0,
            hotkey: None,
        };

        self.state.write().sounds.push(sound.clone());
        Ok(sound)
    }

    pub fn remove_sound(&self, id: &str) -> anyhow::Result<()> {
        self.state.write().sounds.retain(|s| s.id != id);
        Ok(())
    }

    pub fn update_sound(&self, id: &str, name: Option<String>, volume: Option<f32>, hotkey: Option<String>) -> anyhow::Result<()> {
        let mut state = self.state.write();
        if let Some(sound) = state.sounds.iter_mut().find(|s| s.id == id) {
            if let Some(name) = name {
                sound.name = name;
            }
            if let Some(volume) = volume {
                sound.volume = volume.clamp(0.0, 2.0);
            }
            if let Some(hotkey) = hotkey {
                sound.hotkey = if hotkey.is_empty() { None } else { Some(hotkey) };
            }
        }
        Ok(())
    }

    pub fn get_sound(&self, id: &str) -> Option<Sound> {
        self.state.read().sounds.iter().find(|s| s.id == id).cloned()
    }

    pub fn get_all_sounds(&self) -> Vec<Sound> {
        self.state.read().sounds.clone()
    }

    pub fn get_state(&self) -> SoundboardState {
        self.state.read().clone()
    }

    pub fn set_master_volume(&self, volume: f32) {
        self.state.write().master_volume = volume.clamp(0.0, 2.0);
    }

    pub fn set_system_audio_routing(&self, enabled: bool) {
        self.state.write().system_audio_routing_enabled = enabled;
    }

    pub fn find_sound_by_hotkey(&self, hotkey: &str) -> Option<Sound> {
        self.state
            .read()
            .sounds
            .iter()
            .find(|s| s.hotkey.as_deref() == Some(hotkey))
            .cloned()
    }
}
