use crate::types::{commands::*, socket::Request};

use std::path::PathBuf;

/// Supported audio file extensions
const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "flac", "m4a", "aac", "opus"];

/// Validates that a file path is safe and points to a valid audio file.
/// Returns None if the path is invalid or potentially malicious.
fn validate_audio_path(path_str: &str) -> Option<PathBuf> {
    if path_str.is_empty() {
        return None;
    }

    // Security: Reject paths with null bytes
    if path_str.contains('\0') {
        return None;
    }

    let path = PathBuf::from(path_str);

    // Security: Canonicalize to resolve symlinks and ../ sequences
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => return None, // File doesn't exist or permission denied
    };

    // Verify it's a file, not a directory
    if !canonical.is_file() {
        return None;
    }

    // Verify it has a supported audio extension
    if let Some(ext) = canonical.extension() {
        let ext_lower = ext.to_string_lossy().to_lowercase();
        if SUPPORTED_AUDIO_EXTENSIONS.contains(&ext_lower.as_str()) {
            return Some(canonical);
        }
    }

    None
}

pub fn parse_command(request: &Request) -> Option<Box<dyn Executable + Send>> {
    match request.name.as_str() {
        "ping" => Some(Box::new(PingCommand {})),
        "pause" => Some(Box::new(PauseCommand {})),
        "resume" => Some(Box::new(ResumeCommand {})),
        "toggle_pause" => Some(Box::new(TogglePauseCommand {})),
        "stop" => Some(Box::new(StopCommand {})),
        "is_paused" => Some(Box::new(IsPausedCommand {})),
        "get_state" => Some(Box::new(GetStateCommand {})),
        "get_volume" => Some(Box::new(GetVolumeCommand {})),
        "set_volume" => {
            let volume = request
                .args
                .get("volume")
                .unwrap_or(&String::new())
                .parse::<f32>()
                .ok();
            Some(Box::new(SetVolumeCommand { volume }))
        }
        "get_gain" => Some(Box::new(GetGainCommand {})),
        "set_gain" => {
            let gain = request
                .args
                .get("gain")
                .unwrap_or(&String::new())
                .parse::<f32>()
                .ok();
            Some(Box::new(SetGainCommand { gain }))
        }
        "get_mic_gain" => Some(Box::new(GetMicGainCommand {})),
        "set_mic_gain" => {
            let mic_gain = request
                .args
                .get("mic_gain")
                .unwrap_or(&String::new())
                .parse::<f32>()
                .ok();
            Some(Box::new(SetMicGainCommand { mic_gain }))
        }
        "get_position" => Some(Box::new(GetPositionCommand {})),
        "seek" => {
            let position = request
                .args
                .get("position")
                .unwrap_or(&String::new())
                .parse::<f32>()
                .ok();
            Some(Box::new(SeekCommand { position }))
        }
        "get_duration" => Some(Box::new(GetDurationCommand {})),
        "play" => {
            let file_path = request
                .args
                .get("file_path")
                .and_then(|s| validate_audio_path(s));
            Some(Box::new(PlayCommand { file_path }))
        }
        "preview" => {
            let file_path = request
                .args
                .get("file_path")
                .and_then(|s| validate_audio_path(s));
            Some(Box::new(PreviewCommand { file_path }))
        }
        "get_current_file_path" => Some(Box::new(GetCurrentFilePathCommand {})),
        "get_input" => Some(Box::new(GetCurrentInputCommand {})),
        "get_inputs" => Some(Box::new(GetAllInputsCommand {})),
        "set_input" => {
            let name = Some(request.args.get("input_name").unwrap_or(&String::new())).cloned();
            Some(Box::new(SetCurrentInputCommand { name }))
        }
        "get_output" => Some(Box::new(GetCurrentOutputCommand {})),
        "get_outputs" => Some(Box::new(GetAllOutputsCommand {})),
        "set_output" => {
            let name = Some(request.args.get("output_name").unwrap_or(&String::new())).cloned();
            Some(Box::new(SetCurrentOutputCommand { name }))
        }
        "get_loop" => Some(Box::new(GetLoopCommand {})),
        "set_loop" => {
            let enabled = request
                .args
                .get("enabled")
                .unwrap_or(&String::new())
                .parse::<bool>()
                .ok();
            Some(Box::new(SetLoopCommand { enabled }))
        }
        "toggle_loop" => Some(Box::new(ToggleLoopCommand {})),
        // Layer commands
        "play_on_layer" => {
            let layer_index = request
                .args
                .get("layer_index")
                .unwrap_or(&String::new())
                .parse::<usize>()
                .ok();
            let file_path = request
                .args
                .get("file_path")
                .and_then(|s| validate_audio_path(s));
            Some(Box::new(PlayOnLayerCommand { layer_index, file_path }))
        }
        "stop_layer" => {
            let layer_index = request
                .args
                .get("layer_index")
                .unwrap_or(&String::new())
                .parse::<usize>()
                .ok();
            Some(Box::new(StopLayerCommand { layer_index }))
        }
        "stop_all_layers" => Some(Box::new(StopAllLayersCommand {})),
        "set_layer_volume" => {
            let layer_index = request
                .args
                .get("layer_index")
                .unwrap_or(&String::new())
                .parse::<usize>()
                .ok();
            let volume = request
                .args
                .get("volume")
                .unwrap_or(&String::new())
                .parse::<f32>()
                .ok();
            Some(Box::new(SetLayerVolumeCommand { layer_index, volume }))
        }
        "get_layers_info" => Some(Box::new(GetLayersInfoCommand {})),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::TempDir;

    #[test]
    fn test_validate_audio_path_empty_string() {
        assert!(validate_audio_path("").is_none());
    }

    #[test]
    fn test_validate_audio_path_null_byte() {
        assert!(validate_audio_path("/path/to/file\0.mp3").is_none());
        assert!(validate_audio_path("file\0name.wav").is_none());
    }

    #[test]
    fn test_validate_audio_path_nonexistent_file() {
        assert!(validate_audio_path("/nonexistent/path/to/audio.mp3").is_none());
    }

    #[test]
    fn test_validate_audio_path_valid_extensions() {
        let temp_dir = TempDir::new().unwrap();

        for ext in SUPPORTED_AUDIO_EXTENSIONS {
            let file_path = temp_dir.path().join(format!("test.{}", ext));
            File::create(&file_path).unwrap();

            let result = validate_audio_path(file_path.to_str().unwrap());
            assert!(result.is_some(), "Extension {} should be valid", ext);
        }
    }

    #[test]
    fn test_validate_audio_path_invalid_extension() {
        let temp_dir = TempDir::new().unwrap();

        // Create files with invalid extensions
        let invalid_exts = ["txt", "exe", "sh", "py", "jpg", "png"];
        for ext in invalid_exts {
            let file_path = temp_dir.path().join(format!("test.{}", ext));
            File::create(&file_path).unwrap();

            let result = validate_audio_path(file_path.to_str().unwrap());
            assert!(result.is_none(), "Extension {} should be rejected", ext);
        }
    }

    #[test]
    fn test_validate_audio_path_directory_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("subdir.mp3"); // Directory with audio-like name
        fs::create_dir(&dir_path).unwrap();

        let result = validate_audio_path(dir_path.to_str().unwrap());
        assert!(result.is_none(), "Directories should be rejected even with audio extension");
    }

    #[test]
    fn test_validate_audio_path_case_insensitive_extension() {
        let temp_dir = TempDir::new().unwrap();

        let mixed_case_exts = ["MP3", "WaV", "OGG", "FLAC"];
        for ext in mixed_case_exts {
            let file_path = temp_dir.path().join(format!("test.{}", ext));
            File::create(&file_path).unwrap();

            let result = validate_audio_path(file_path.to_str().unwrap());
            assert!(result.is_some(), "Extension {} should be valid (case insensitive)", ext);
        }
    }

    #[test]
    fn test_validate_audio_path_returns_canonical() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.mp3");
        File::create(&file_path).unwrap();

        // Use a path with redundant components
        let redundant_path = temp_dir.path().join("./test.mp3");
        let result = validate_audio_path(redundant_path.to_str().unwrap());

        assert!(result.is_some());
        // The result should be canonicalized (absolute path)
        assert!(result.unwrap().is_absolute());
    }

    #[test]
    fn test_validate_audio_path_no_extension() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("noextension");
        File::create(&file_path).unwrap();

        let result = validate_audio_path(file_path.to_str().unwrap());
        assert!(result.is_none(), "Files without extension should be rejected");
    }
}
