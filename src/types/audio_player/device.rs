use rodio::cpal;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use std::collections::HashMap;

/// Get all available output devices
pub fn get_output_devices() -> HashMap<String, String> {
    let mut devices = HashMap::new();
    let host = cpal::default_host();
    if let Ok(output_devices) = host.output_devices() {
        for device in output_devices {
            if let Ok(desc) = device.description() {
                let name = desc.name().to_string();
                devices.insert(name.clone(), name);
            }
        }
    }
    devices
}

/// Get the default output device name
pub fn get_default_output_device() -> Option<String> {
    let host = cpal::default_host();
    host.default_output_device()
        .and_then(|d| d.description().ok())
        .map(|desc| desc.name().to_string())
}

/// Get all available input devices (for mic selection)
pub fn get_input_devices() -> HashMap<String, String> {
    let mut devices = HashMap::new();
    let host = cpal::default_host();
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(desc) = device.description() {
                let name = desc.name().to_string();
                devices.insert(name.clone(), name);
            }
        }
    }
    devices
}
