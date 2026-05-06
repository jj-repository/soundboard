//! IPC parser. Translates a wire-level [`Request`] into a typed
//! [`DaemonCommand`]. Returns `None` for an unknown command name OR for a
//! known command whose required argument is missing/unparseable — the daemon
//! responds with a generic error rather than constructing a "command with
//! Option<f32> args" that has to validate again at execute time.

use crate::types::{commands::DaemonCommand, socket::Request};
use crate::utils::path::validate_audio_path;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

fn arg<'a>(args: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    args.get(key).map(String::as_str)
}

fn arg_parse<T: FromStr>(args: &HashMap<String, String>, key: &str) -> Option<T> {
    arg(args, key)?.parse::<T>().ok()
}

fn arg_audio_path(args: &HashMap<String, String>) -> Option<PathBuf> {
    validate_audio_path(arg(args, "file_path")?)
}

pub fn parse_command(request: &Request) -> Option<DaemonCommand> {
    use DaemonCommand::*;

    let a = &request.args;
    Some(match request.name.as_str() {
        "ping" => Ping,
        "pause" => Pause,
        "resume" => Resume,
        "toggle_pause" => TogglePause,
        "stop" => Stop,

        "is_paused" => IsPaused,
        "get_state" => GetState,
        "get_volume" => GetVolume,
        "set_volume" => SetVolume(arg_parse(a, "volume")?),
        "get_gain" => GetGain,
        "set_gain" => SetGain(arg_parse(a, "gain")?),
        "get_mic_gain" => GetMicGain,
        "set_mic_gain" => SetMicGain(arg_parse(a, "mic_gain")?),
        "get_position" => GetPosition,
        "seek" => Seek(arg_parse(a, "position")?),
        "get_duration" => GetDuration,
        "play" => Play(arg_audio_path(a)?),
        "preview" => Preview(arg_audio_path(a)?),

        "get_current_file_path" => GetCurrentFilePath,
        "get_input" => GetCurrentInput,
        "get_inputs" => GetAllInputs,
        "set_input" => SetCurrentInput(arg(a, "input_name")?.to_string()),
        "get_output" => GetCurrentOutput,
        "get_outputs" => GetAllOutputs,
        "set_output" => SetCurrentOutput(arg(a, "output_name")?.to_string()),

        "get_loop" => GetLoop,
        "set_loop" => SetLoop(arg_parse(a, "enabled")?),
        "toggle_loop" => ToggleLoop,

        "play_on_layer" => PlayOnLayer {
            index: arg_parse(a, "layer_index")?,
            file_path: arg_audio_path(a)?,
        },
        "stop_layer" => StopLayer(arg_parse(a, "layer_index")?),
        "stop_all_layers" => StopAllLayers,
        "set_layer_volume" => SetLayerVolume {
            index: arg_parse(a, "layer_index")?,
            volume: arg_parse(a, "volume")?,
        },
        "get_layers_info" => GetLayersInfo,

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::path::SUPPORTED_AUDIO_EXTENSIONS;
    use std::fs::{self, File};
    use tempfile::TempDir;

    fn req(name: &str) -> Request {
        Request { name: name.to_string(), args: HashMap::new() }
    }

    fn req_with(name: &str, args: &[(&str, &str)]) -> Request {
        let mut map = HashMap::new();
        for (k, v) in args {
            map.insert((*k).to_string(), (*v).to_string());
        }
        Request { name: name.to_string(), args: map }
    }

    // --- validate_audio_path tests preserved from the prior commands.rs ---

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
            assert!(
                validate_audio_path(file_path.to_str().unwrap()).is_some(),
                "Extension {} should be valid", ext
            );
        }
    }

    #[test]
    fn test_validate_audio_path_invalid_extension() {
        let temp_dir = TempDir::new().unwrap();
        for ext in ["txt", "exe", "sh", "py", "jpg", "png"] {
            let file_path = temp_dir.path().join(format!("test.{}", ext));
            File::create(&file_path).unwrap();
            assert!(
                validate_audio_path(file_path.to_str().unwrap()).is_none(),
                "Extension {} should be rejected", ext
            );
        }
    }

    #[test]
    fn test_validate_audio_path_directory_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("subdir.mp3");
        fs::create_dir(&dir_path).unwrap();
        assert!(validate_audio_path(dir_path.to_str().unwrap()).is_none());
    }

    #[test]
    fn test_validate_audio_path_case_insensitive_extension() {
        let temp_dir = TempDir::new().unwrap();
        for ext in ["MP3", "WaV", "OGG", "FLAC"] {
            let file_path = temp_dir.path().join(format!("test.{}", ext));
            File::create(&file_path).unwrap();
            assert!(validate_audio_path(file_path.to_str().unwrap()).is_some());
        }
    }

    #[test]
    fn test_validate_audio_path_returns_canonical() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("test.mp3")).unwrap();
        let redundant = temp_dir.path().join("./test.mp3");
        let result = validate_audio_path(redundant.to_str().unwrap());
        assert!(result.is_some());
        assert!(result.unwrap().is_absolute());
    }

    #[test]
    fn test_validate_audio_path_no_extension() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("noextension");
        File::create(&path).unwrap();
        assert!(validate_audio_path(path.to_str().unwrap()).is_none());
    }

    #[test]
    fn test_validate_audio_path_traversal_rejected() {
        assert!(validate_audio_path("../../../etc/passwd").is_none());
        assert!(validate_audio_path("/tmp/../etc/shadow.mp3").is_none());
    }

    #[test]
    fn test_validate_audio_path_double_extension_non_audio() {
        let temp_dir = TempDir::new().unwrap();
        let file = temp_dir.path().join("file.exe.mp3");
        File::create(&file).unwrap();
        assert!(validate_audio_path(file.to_str().unwrap()).is_some());
    }

    #[test]
    fn test_validate_audio_path_hidden_file_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let file = temp_dir.path().join(".hidden.mp3");
        File::create(&file).unwrap();
        assert!(validate_audio_path(file.to_str().unwrap()).is_some());
    }

    #[test]
    fn test_validate_audio_path_whitespace_only() {
        assert!(validate_audio_path("   ").is_none());
    }

    // --- parse_command argless commands ---

    #[test]
    fn parse_argless_commands() {
        for name in [
            "ping", "pause", "resume", "toggle_pause", "stop", "is_paused",
            "get_state", "get_volume", "get_gain", "get_mic_gain", "get_position",
            "get_duration", "get_current_file_path", "get_input", "get_inputs",
            "get_output", "get_outputs", "get_loop", "toggle_loop",
            "stop_all_layers", "get_layers_info",
        ] {
            assert!(parse_command(&req(name)).is_some(), "{name} should parse");
        }
    }

    #[test]
    fn parse_unknown_command_is_none() {
        assert!(parse_command(&req("not_a_real_command")).is_none());
        assert!(parse_command(&req("")).is_none());
    }

    // --- Strict-arg parsing: invalid args now reject at parse time ---

    #[test]
    fn parse_set_volume_strict() {
        assert!(matches!(
            parse_command(&req_with("set_volume", &[("volume", "0.5")])),
            Some(DaemonCommand::SetVolume(v)) if v == 0.5
        ));
        assert!(parse_command(&req("set_volume")).is_none(), "missing arg rejected");
        assert!(
            parse_command(&req_with("set_volume", &[("volume", "abc")])).is_none(),
            "non-numeric arg rejected"
        );
    }

    #[test]
    fn parse_set_loop_strict() {
        assert!(matches!(
            parse_command(&req_with("set_loop", &[("enabled", "true")])),
            Some(DaemonCommand::SetLoop(true))
        ));
        assert!(
            parse_command(&req_with("set_loop", &[("enabled", "maybe")])).is_none()
        );
    }

    #[test]
    fn parse_seek_strict() {
        assert!(matches!(
            parse_command(&req_with("seek", &[("position", "30.5")])),
            Some(DaemonCommand::Seek(p)) if p == 30.5
        ));
        // Negative still parses; clamping happens at exec time via player.
        assert!(matches!(
            parse_command(&req_with("seek", &[("position", "-10.0")])),
            Some(DaemonCommand::Seek(_))
        ));
    }

    #[test]
    fn parse_set_layer_volume_strict() {
        let ok = req_with("set_layer_volume", &[("layer_index", "1"), ("volume", "0.8")]);
        assert!(matches!(
            parse_command(&ok),
            Some(DaemonCommand::SetLayerVolume { index: 1, volume: 0.8 })
        ));
        let missing = req_with("set_layer_volume", &[("volume", "0.5")]);
        assert!(parse_command(&missing).is_none());
        let bad_idx = req_with("set_layer_volume", &[("layer_index", "abc"), ("volume", "0.5")]);
        assert!(parse_command(&bad_idx).is_none());
    }

    #[test]
    fn parse_set_input_strict() {
        assert!(matches!(
            parse_command(&req_with("set_input", &[("input_name", "mic1")])),
            Some(DaemonCommand::SetCurrentInput(s)) if s == "mic1"
        ));
        assert!(parse_command(&req("set_input")).is_none());
    }

    #[test]
    fn parse_set_output_strict() {
        assert!(matches!(
            parse_command(&req_with("set_output", &[("output_name", "spk1")])),
            Some(DaemonCommand::SetCurrentOutput(s)) if s == "spk1"
        ));
        assert!(parse_command(&req("set_output")).is_none());
    }

    #[test]
    fn parse_play_requires_valid_audio_path() {
        // Valid audio file
        let temp_dir = TempDir::new().unwrap();
        let file = temp_dir.path().join("song.mp3");
        File::create(&file).unwrap();
        let ok = req_with("play", &[("file_path", file.to_str().unwrap())]);
        assert!(matches!(parse_command(&ok), Some(DaemonCommand::Play(_))));

        // Missing path
        assert!(parse_command(&req("play")).is_none());
        // Non-audio extension
        let bad = req_with("play", &[("file_path", "/etc/passwd")]);
        assert!(parse_command(&bad).is_none());
    }
}
