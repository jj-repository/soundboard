use crate::types::{commands::*, socket::Request};

use std::path::PathBuf;

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
                .unwrap_or(&String::new())
                .parse::<PathBuf>()
                .ok();
            Some(Box::new(PlayCommand { file_path }))
        }
        "preview" => {
            let file_path = request
                .args
                .get("file_path")
                .unwrap_or(&String::new())
                .parse::<PathBuf>()
                .ok();
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
                .unwrap_or(&String::new())
                .parse::<PathBuf>()
                .ok();
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
