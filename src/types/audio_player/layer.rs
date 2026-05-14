use rodio::Player;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Number of audio layers available for mixing
pub const NUM_AUDIO_LAYERS: usize = 4;

/// Represents a single audio layer that can play sounds independently
pub struct AudioLayer {
    pub sink: Player,
    pub volume: f32,
    pub current_file_path: Option<PathBuf>,
    pub duration: Option<f32>,
}

impl AudioLayer {
    pub fn new(mixer: &rodio::mixer::Mixer) -> Self {
        let sink = Player::connect_new(mixer);
        sink.set_volume(1.0);
        Self {
            sink,
            volume: 1.0,
            current_file_path: None,
            duration: None,
        }
    }

    pub fn is_playing(&self) -> bool {
        !self.sink.empty() && !self.sink.is_paused()
    }

    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }
}

/// Information about an audio layer (serialisable snapshot)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerInfo {
    pub index: usize,
    pub is_playing: bool,
    pub is_paused: bool,
    pub is_empty: bool,
    pub volume: f32,
    pub current_file: Option<PathBuf>,
    pub position: f32,
    pub duration: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_audio_layers_const() {
        assert_eq!(NUM_AUDIO_LAYERS, 4);
    }

    #[test]
    fn layer_info_default_is_inactive() {
        let info = LayerInfo::default();
        assert_eq!(info.index, 0);
        assert!(!info.is_playing);
        assert!(!info.is_paused);
        assert!(!info.is_empty);
        assert_eq!(info.volume, 0.0);
        assert!(info.current_file.is_none());
        assert_eq!(info.position, 0.0);
        assert!(info.duration.is_none());
    }

    #[test]
    fn layer_info_serde_roundtrip() {
        let original = LayerInfo {
            index: 2,
            is_playing: true,
            is_paused: false,
            is_empty: false,
            volume: 0.42,
            current_file: Some(PathBuf::from("/sounds/a.mp3")),
            position: 1.5,
            duration: Some(3.0),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: LayerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.index, original.index);
        assert!(parsed.is_playing);
        assert_eq!(parsed.volume, 0.42);
        assert_eq!(parsed.current_file, original.current_file);
    }
}
