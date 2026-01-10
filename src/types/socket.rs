use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub name: String,
    pub args: HashMap<String, String>,
}

impl Request {
    pub fn new<T: AsRef<str>>(function_name: T, data: Vec<(T, T)>) -> Self {
        let hashmap_data: HashMap<String, String> = data
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_string(), value.as_ref().to_string()))
            .collect();

        Request {
            name: function_name.as_ref().to_string(),
            args: hashmap_data,
        }
    }

    pub fn ping() -> Self {
        Request::new("ping", vec![])
    }

    pub fn pause() -> Self {
        Request::new("pause", vec![])
    }

    pub fn resume() -> Self {
        Request::new("resume", vec![])
    }

    pub fn toggle_pause() -> Self {
        Request::new("toggle_pause", vec![])
    }

    pub fn stop() -> Self {
        Request::new("stop", vec![])
    }

    pub fn play(file_path: &str) -> Self {
        Request::new("play", vec![("file_path", file_path)])
    }

    pub fn preview(file_path: &str) -> Self {
        Request::new("preview", vec![("file_path", file_path)])
    }

    pub fn get_is_paused() -> Self {
        Request::new("is_paused", vec![])
    }

    pub fn get_volume() -> Self {
        Request::new("get_volume", vec![])
    }

    pub fn get_position() -> Self {
        Request::new("get_position", vec![])
    }

    pub fn get_duration() -> Self {
        Request::new("get_duration", vec![])
    }

    pub fn get_state() -> Self {
        Request::new("get_state", vec![])
    }

    pub fn get_current_file_path() -> Self {
        Request::new("get_current_file_path", vec![])
    }

    pub fn get_input() -> Self {
        Request::new("get_input", vec![])
    }

    pub fn get_inputs() -> Self {
        Request::new("get_inputs", vec![])
    }

    pub fn set_volume(volume: f32) -> Self {
        Request::new("set_volume", vec![("volume", &volume.to_string())])
    }

    pub fn get_gain() -> Self {
        Request::new("get_gain", vec![])
    }

    pub fn set_gain(gain: f32) -> Self {
        Request::new("set_gain", vec![("gain", &gain.to_string())])
    }

    pub fn get_mic_gain() -> Self {
        Request::new("get_mic_gain", vec![])
    }

    pub fn set_mic_gain(mic_gain: f32) -> Self {
        Request::new("set_mic_gain", vec![("mic_gain", &mic_gain.to_string())])
    }

    pub fn seek(position: f32) -> Self {
        Request::new("seek", vec![("position", &position.to_string())])
    }

    pub fn set_input(name: &str) -> Self {
        Request::new("set_input", vec![("input_name", name)])
    }

    pub fn get_output() -> Self {
        Request::new("get_output", vec![])
    }

    pub fn get_outputs() -> Self {
        Request::new("get_outputs", vec![])
    }

    pub fn set_output(name: &str) -> Self {
        Request::new("set_output", vec![("output_name", name)])
    }

    pub fn get_loop() -> Self {
        Request::new("get_loop", vec![])
    }

    pub fn set_loop(enabled: &str) -> Self {
        Request::new("set_loop", vec![("enabled", enabled)])
    }

    pub fn toggle_loop() -> Self {
        Request::new("toggle_loop", vec![])
    }

    // Layer commands
    pub fn play_on_layer(layer_index: usize, file_path: &str) -> Self {
        Request::new("play_on_layer", vec![
            ("layer_index", &layer_index.to_string()),
            ("file_path", file_path),
        ])
    }

    pub fn stop_layer(layer_index: usize) -> Self {
        Request::new("stop_layer", vec![("layer_index", &layer_index.to_string())])
    }

    pub fn stop_all_layers() -> Self {
        Request::new("stop_all_layers", vec![])
    }

    pub fn set_layer_volume(layer_index: usize, volume: f32) -> Self {
        Request::new("set_layer_volume", vec![
            ("layer_index", &layer_index.to_string()),
            ("volume", &volume.to_string()),
        ])
    }

    pub fn get_layers_info() -> Self {
        Request::new("get_layers_info", vec![])
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub status: bool,
    pub message: String,
}

impl Response {
    pub fn new<T: AsRef<str>>(status: bool, message: T) -> Self {
        Response {
            status,
            message: message.as_ref().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Request::new tests
    #[test]
    fn test_request_new_empty_args() {
        let request = Request::new("test_command", vec![]);
        assert_eq!(request.name, "test_command");
        assert!(request.args.is_empty());
    }

    #[test]
    fn test_request_new_with_args() {
        let request = Request::new("test_command", vec![("key1", "value1"), ("key2", "value2")]);
        assert_eq!(request.name, "test_command");
        assert_eq!(request.args.get("key1"), Some(&"value1".to_string()));
        assert_eq!(request.args.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_request_new_converts_to_string() {
        let request = Request::new(String::from("cmd"), vec![(String::from("k"), String::from("v"))]);
        assert_eq!(request.name, "cmd");
        assert_eq!(request.args.get("k"), Some(&"v".to_string()));
    }

    // Request helper method tests
    #[test]
    fn test_request_ping() {
        let request = Request::ping();
        assert_eq!(request.name, "ping");
        assert!(request.args.is_empty());
    }

    #[test]
    fn test_request_pause() {
        let request = Request::pause();
        assert_eq!(request.name, "pause");
        assert!(request.args.is_empty());
    }

    #[test]
    fn test_request_resume() {
        let request = Request::resume();
        assert_eq!(request.name, "resume");
        assert!(request.args.is_empty());
    }

    #[test]
    fn test_request_toggle_pause() {
        let request = Request::toggle_pause();
        assert_eq!(request.name, "toggle_pause");
        assert!(request.args.is_empty());
    }

    #[test]
    fn test_request_stop() {
        let request = Request::stop();
        assert_eq!(request.name, "stop");
        assert!(request.args.is_empty());
    }

    #[test]
    fn test_request_play() {
        let request = Request::play("/path/to/file.mp3");
        assert_eq!(request.name, "play");
        assert_eq!(request.args.get("file_path"), Some(&"/path/to/file.mp3".to_string()));
    }

    #[test]
    fn test_request_preview() {
        let request = Request::preview("/path/to/file.wav");
        assert_eq!(request.name, "preview");
        assert_eq!(request.args.get("file_path"), Some(&"/path/to/file.wav".to_string()));
    }

    #[test]
    fn test_request_get_is_paused() {
        let request = Request::get_is_paused();
        assert_eq!(request.name, "is_paused");
    }

    #[test]
    fn test_request_get_volume() {
        let request = Request::get_volume();
        assert_eq!(request.name, "get_volume");
    }

    #[test]
    fn test_request_set_volume() {
        let request = Request::set_volume(0.75);
        assert_eq!(request.name, "set_volume");
        assert_eq!(request.args.get("volume"), Some(&"0.75".to_string()));
    }

    #[test]
    fn test_request_get_gain() {
        let request = Request::get_gain();
        assert_eq!(request.name, "get_gain");
    }

    #[test]
    fn test_request_set_gain() {
        let request = Request::set_gain(1.5);
        assert_eq!(request.name, "set_gain");
        assert_eq!(request.args.get("gain"), Some(&"1.5".to_string()));
    }

    #[test]
    fn test_request_get_mic_gain() {
        let request = Request::get_mic_gain();
        assert_eq!(request.name, "get_mic_gain");
    }

    #[test]
    fn test_request_set_mic_gain() {
        let request = Request::set_mic_gain(2.0);
        assert_eq!(request.name, "set_mic_gain");
        assert_eq!(request.args.get("mic_gain"), Some(&"2".to_string()));
    }

    #[test]
    fn test_request_get_position() {
        let request = Request::get_position();
        assert_eq!(request.name, "get_position");
    }

    #[test]
    fn test_request_seek() {
        let request = Request::seek(30.5);
        assert_eq!(request.name, "seek");
        assert_eq!(request.args.get("position"), Some(&"30.5".to_string()));
    }

    #[test]
    fn test_request_get_duration() {
        let request = Request::get_duration();
        assert_eq!(request.name, "get_duration");
    }

    #[test]
    fn test_request_get_state() {
        let request = Request::get_state();
        assert_eq!(request.name, "get_state");
    }

    #[test]
    fn test_request_get_current_file_path() {
        let request = Request::get_current_file_path();
        assert_eq!(request.name, "get_current_file_path");
    }

    #[test]
    fn test_request_get_input() {
        let request = Request::get_input();
        assert_eq!(request.name, "get_input");
    }

    #[test]
    fn test_request_get_inputs() {
        let request = Request::get_inputs();
        assert_eq!(request.name, "get_inputs");
    }

    #[test]
    fn test_request_set_input() {
        let request = Request::set_input("mic1");
        assert_eq!(request.name, "set_input");
        assert_eq!(request.args.get("input_name"), Some(&"mic1".to_string()));
    }

    #[test]
    fn test_request_get_output() {
        let request = Request::get_output();
        assert_eq!(request.name, "get_output");
    }

    #[test]
    fn test_request_get_outputs() {
        let request = Request::get_outputs();
        assert_eq!(request.name, "get_outputs");
    }

    #[test]
    fn test_request_set_output() {
        let request = Request::set_output("speaker1");
        assert_eq!(request.name, "set_output");
        assert_eq!(request.args.get("output_name"), Some(&"speaker1".to_string()));
    }

    #[test]
    fn test_request_get_loop() {
        let request = Request::get_loop();
        assert_eq!(request.name, "get_loop");
    }

    #[test]
    fn test_request_set_loop() {
        let request = Request::set_loop("true");
        assert_eq!(request.name, "set_loop");
        assert_eq!(request.args.get("enabled"), Some(&"true".to_string()));
    }

    #[test]
    fn test_request_toggle_loop() {
        let request = Request::toggle_loop();
        assert_eq!(request.name, "toggle_loop");
    }

    // Layer command tests
    #[test]
    fn test_request_play_on_layer() {
        let request = Request::play_on_layer(0, "/path/to/file.ogg");
        assert_eq!(request.name, "play_on_layer");
        assert_eq!(request.args.get("layer_index"), Some(&"0".to_string()));
        assert_eq!(request.args.get("file_path"), Some(&"/path/to/file.ogg".to_string()));
    }

    #[test]
    fn test_request_stop_layer() {
        let request = Request::stop_layer(2);
        assert_eq!(request.name, "stop_layer");
        assert_eq!(request.args.get("layer_index"), Some(&"2".to_string()));
    }

    #[test]
    fn test_request_stop_all_layers() {
        let request = Request::stop_all_layers();
        assert_eq!(request.name, "stop_all_layers");
    }

    #[test]
    fn test_request_set_layer_volume() {
        let request = Request::set_layer_volume(1, 0.8);
        assert_eq!(request.name, "set_layer_volume");
        assert_eq!(request.args.get("layer_index"), Some(&"1".to_string()));
        assert_eq!(request.args.get("volume"), Some(&"0.8".to_string()));
    }

    #[test]
    fn test_request_get_layers_info() {
        let request = Request::get_layers_info();
        assert_eq!(request.name, "get_layers_info");
    }

    // Response tests
    #[test]
    fn test_response_new_success() {
        let response = Response::new(true, "Success message");
        assert!(response.status);
        assert_eq!(response.message, "Success message");
    }

    #[test]
    fn test_response_new_failure() {
        let response = Response::new(false, "Error message");
        assert!(!response.status);
        assert_eq!(response.message, "Error message");
    }

    #[test]
    fn test_response_new_with_string() {
        let response = Response::new(true, String::from("Test"));
        assert!(response.status);
        assert_eq!(response.message, "Test");
    }

    #[test]
    fn test_response_default() {
        let response = Response::default();
        assert!(!response.status);
        assert!(response.message.is_empty());
    }

    #[test]
    fn test_request_default() {
        let request = Request::default();
        assert!(request.name.is_empty());
        assert!(request.args.is_empty());
    }

    // Serialization tests
    #[test]
    fn test_request_serialization() {
        let request = Request::ping();
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"name\":\"ping\""));
    }

    #[test]
    fn test_request_deserialization() {
        let json = r#"{"name":"ping","args":{}}"#;
        let request: Request = serde_json::from_str(json).unwrap();
        assert_eq!(request.name, "ping");
        assert!(request.args.is_empty());
    }

    #[test]
    fn test_response_serialization() {
        let response = Response::new(true, "pong");
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":true"));
        assert!(json.contains("\"message\":\"pong\""));
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{"status":true,"message":"pong"}"#;
        let response: Response = serde_json::from_str(json).unwrap();
        assert!(response.status);
        assert_eq!(response.message, "pong");
    }
}
