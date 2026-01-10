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

    // Tests for parse_command function
    use crate::types::socket::Request;
    use std::collections::HashMap;

    #[test]
    fn test_parse_command_ping() {
        let request = Request { name: "ping".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "ping command should be parsed");
    }

    #[test]
    fn test_parse_command_pause() {
        let request = Request { name: "pause".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "pause command should be parsed");
    }

    #[test]
    fn test_parse_command_resume() {
        let request = Request { name: "resume".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "resume command should be parsed");
    }

    #[test]
    fn test_parse_command_toggle_pause() {
        let request = Request { name: "toggle_pause".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "toggle_pause command should be parsed");
    }

    #[test]
    fn test_parse_command_stop() {
        let request = Request { name: "stop".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "stop command should be parsed");
    }

    #[test]
    fn test_parse_command_is_paused() {
        let request = Request { name: "is_paused".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "is_paused command should be parsed");
    }

    #[test]
    fn test_parse_command_get_state() {
        let request = Request { name: "get_state".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_state command should be parsed");
    }

    #[test]
    fn test_parse_command_get_volume() {
        let request = Request { name: "get_volume".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_volume command should be parsed");
    }

    #[test]
    fn test_parse_command_set_volume_with_valid_args() {
        let mut args = HashMap::new();
        args.insert("volume".to_string(), "0.5".to_string());
        let request = Request { name: "set_volume".to_string(), args };
        let result = parse_command(&request);
        assert!(result.is_some(), "set_volume command should be parsed");
    }

    #[test]
    fn test_parse_command_set_volume_without_args() {
        let request = Request { name: "set_volume".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "set_volume command should be parsed even without args");
    }

    #[test]
    fn test_parse_command_get_gain() {
        let request = Request { name: "get_gain".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_gain command should be parsed");
    }

    #[test]
    fn test_parse_command_set_gain() {
        let mut args = HashMap::new();
        args.insert("gain".to_string(), "1.5".to_string());
        let request = Request { name: "set_gain".to_string(), args };
        let result = parse_command(&request);
        assert!(result.is_some(), "set_gain command should be parsed");
    }

    #[test]
    fn test_parse_command_get_mic_gain() {
        let request = Request { name: "get_mic_gain".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_mic_gain command should be parsed");
    }

    #[test]
    fn test_parse_command_set_mic_gain() {
        let mut args = HashMap::new();
        args.insert("mic_gain".to_string(), "2.0".to_string());
        let request = Request { name: "set_mic_gain".to_string(), args };
        let result = parse_command(&request);
        assert!(result.is_some(), "set_mic_gain command should be parsed");
    }

    #[test]
    fn test_parse_command_get_position() {
        let request = Request { name: "get_position".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_position command should be parsed");
    }

    #[test]
    fn test_parse_command_seek() {
        let mut args = HashMap::new();
        args.insert("position".to_string(), "30.5".to_string());
        let request = Request { name: "seek".to_string(), args };
        let result = parse_command(&request);
        assert!(result.is_some(), "seek command should be parsed");
    }

    #[test]
    fn test_parse_command_get_duration() {
        let request = Request { name: "get_duration".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_duration command should be parsed");
    }

    #[test]
    fn test_parse_command_get_current_file_path() {
        let request = Request { name: "get_current_file_path".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_current_file_path command should be parsed");
    }

    #[test]
    fn test_parse_command_get_input() {
        let request = Request { name: "get_input".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_input command should be parsed");
    }

    #[test]
    fn test_parse_command_get_inputs() {
        let request = Request { name: "get_inputs".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_inputs command should be parsed");
    }

    #[test]
    fn test_parse_command_set_input() {
        let mut args = HashMap::new();
        args.insert("input_name".to_string(), "mic1".to_string());
        let request = Request { name: "set_input".to_string(), args };
        let result = parse_command(&request);
        assert!(result.is_some(), "set_input command should be parsed");
    }

    #[test]
    fn test_parse_command_get_output() {
        let request = Request { name: "get_output".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_output command should be parsed");
    }

    #[test]
    fn test_parse_command_get_outputs() {
        let request = Request { name: "get_outputs".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_outputs command should be parsed");
    }

    #[test]
    fn test_parse_command_set_output() {
        let mut args = HashMap::new();
        args.insert("output_name".to_string(), "speaker1".to_string());
        let request = Request { name: "set_output".to_string(), args };
        let result = parse_command(&request);
        assert!(result.is_some(), "set_output command should be parsed");
    }

    #[test]
    fn test_parse_command_get_loop() {
        let request = Request { name: "get_loop".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_loop command should be parsed");
    }

    #[test]
    fn test_parse_command_set_loop() {
        let mut args = HashMap::new();
        args.insert("enabled".to_string(), "true".to_string());
        let request = Request { name: "set_loop".to_string(), args };
        let result = parse_command(&request);
        assert!(result.is_some(), "set_loop command should be parsed");
    }

    #[test]
    fn test_parse_command_toggle_loop() {
        let request = Request { name: "toggle_loop".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "toggle_loop command should be parsed");
    }

    #[test]
    fn test_parse_command_stop_layer() {
        let mut args = HashMap::new();
        args.insert("layer_index".to_string(), "0".to_string());
        let request = Request { name: "stop_layer".to_string(), args };
        let result = parse_command(&request);
        assert!(result.is_some(), "stop_layer command should be parsed");
    }

    #[test]
    fn test_parse_command_stop_all_layers() {
        let request = Request { name: "stop_all_layers".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "stop_all_layers command should be parsed");
    }

    #[test]
    fn test_parse_command_set_layer_volume() {
        let mut args = HashMap::new();
        args.insert("layer_index".to_string(), "1".to_string());
        args.insert("volume".to_string(), "0.8".to_string());
        let request = Request { name: "set_layer_volume".to_string(), args };
        let result = parse_command(&request);
        assert!(result.is_some(), "set_layer_volume command should be parsed");
    }

    #[test]
    fn test_parse_command_get_layers_info() {
        let request = Request { name: "get_layers_info".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_some(), "get_layers_info command should be parsed");
    }

    #[test]
    fn test_parse_command_unknown_returns_none() {
        let request = Request { name: "unknown_command".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_none(), "unknown command should return None");
    }

    #[test]
    fn test_parse_command_empty_name_returns_none() {
        let request = Request { name: "".to_string(), args: HashMap::new() };
        let result = parse_command(&request);
        assert!(result.is_none(), "empty command name should return None");
    }

    #[test]
    fn test_parse_command_invalid_volume_string() {
        let mut args = HashMap::new();
        args.insert("volume".to_string(), "not_a_number".to_string());
        let request = Request { name: "set_volume".to_string(), args };
        let result = parse_command(&request);
        // Command is still parsed, but volume will be None internally
        assert!(result.is_some(), "set_volume with invalid number should still parse");
    }
}
