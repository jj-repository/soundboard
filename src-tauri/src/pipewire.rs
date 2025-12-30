use anyhow::{Context, Result};
use std::process::Command;

pub struct PipeWireManager {
    virtual_mic_name: String,
    sink_name: String,
}

impl PipeWireManager {
    pub fn new() -> Self {
        Self {
            virtual_mic_name: "SoundboardMic".to_string(),
            sink_name: "Soundboard_Mix".to_string(),
        }
    }

    pub fn check_virtual_mic_exists(&self) -> Result<bool> {
        let output = Command::new("pactl")
            .args(["list", "sources", "short"])
            .output()
            .context("Failed to list sources")?;

        let sources = String::from_utf8_lossy(&output.stdout);
        Ok(sources.contains(&self.virtual_mic_name))
    }

    pub fn setup_virtual_microphone(&self) -> Result<()> {
        // First, clean up any existing virtual devices
        self.cleanup()?;

        // Step 1: Create a null sink where we'll mix audio
        let _output = Command::new("pactl")
            .args([
                "load-module",
                "module-null-sink",
                &format!("sink_name={}", self.sink_name),
                "sink_properties=device.description=\"Soundboard Mix\"",
            ])
            .output()
            .context("Failed to create mix sink")?;

        // Step 2: Create a virtual source from the sink's monitor
        // This is what Discord will see as a microphone
        // CRITICAL: media.class=Audio/Source is required for Discord/apps to detect it
        let monitor_source = format!("{}.monitor", self.sink_name);
        let output = Command::new("pactl")
            .args([
                "load-module",
                "module-remap-source",
                &format!("source_name={}", self.virtual_mic_name),
                &format!("master={}", monitor_source),
                "source_properties=device.description=\"Soundboard Virtual Microphone\" media.class=Audio/Source",
            ])
            .output()
            .context("Failed to create virtual microphone")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create virtual mic: {}", stderr);
        }

        // Step 3: Create loopback from real microphone to the mix sink
        let default_source = self.get_default_source()?;
        let _output = Command::new("pactl")
            .args([
                "load-module",
                "module-loopback",
                &format!("source={}", default_source),
                &format!("sink={}", self.sink_name),
                "latency_msec=1",
            ])
            .output()
            .context("Failed to create mic loopback")?;

        Ok(())
    }

    pub fn cleanup(&self) -> Result<()> {
        // Remove all modules related to our virtual mic
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "pactl list modules short | grep -E '({}|{})' | awk '{{print $1}}'",
                self.virtual_mic_name, self.sink_name
            ))
            .output()
            .context("Failed to list modules")?;

        let module_ids = String::from_utf8_lossy(&output.stdout);
        for module_id in module_ids.lines() {
            if !module_id.trim().is_empty() {
                let _ = Command::new("pactl")
                    .args(["unload-module", module_id.trim()])
                    .output();
            }
        }

        // Also remove loopback modules
        self.remove_all_loopbacks()?;

        Ok(())
    }

    pub fn create_loopback(&self, source: &str, sink: &str) -> Result<()> {
        let output = Command::new("pactl")
            .args([
                "load-module",
                "module-loopback",
                &format!("source={}", source),
                &format!("sink={}", sink),
                "latency_msec=1",
            ])
            .output()
            .context("Failed to create loopback")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create loopback: {}", stderr);
        }

        Ok(())
    }

    pub fn route_system_audio_to_mic(&self, enable: bool) -> Result<()> {
        if enable {
            // Check if virtual mic exists
            if !self.check_virtual_mic_exists()? {
                anyhow::bail!("Virtual microphone not set up. Please set it up first.");
            }

            // Create a loopback from system audio monitor to the mix sink
            self.create_loopback("@DEFAULT_MONITOR@", &self.sink_name)?;
        } else {
            // Remove system audio loopbacks only
            // This is simplified - ideally we'd track specific module IDs
            self.remove_all_loopbacks()?;

            // Recreate the mic loopback if virtual mic exists
            if self.check_virtual_mic_exists()? {
                let default_source = self.get_default_source()?;
                let _ = self.create_loopback(&default_source, &self.sink_name);
            }
        }
        Ok(())
    }

    pub fn remove_all_loopbacks(&self) -> Result<()> {
        // Get all loopback modules
        let output = Command::new("sh")
            .arg("-c")
            .arg("pactl list modules short | grep module-loopback | awk '{print $1}'")
            .output()
            .context("Failed to list loopback modules")?;

        let module_ids = String::from_utf8_lossy(&output.stdout);
        for module_id in module_ids.lines() {
            if !module_id.trim().is_empty() {
                let _ = Command::new("pactl")
                    .args(["unload-module", module_id.trim()])
                    .output();
            }
        }

        Ok(())
    }

    pub fn list_sources(&self) -> Result<Vec<String>> {
        let output = Command::new("pactl")
            .args(["list", "sources", "short"])
            .output()
            .context("Failed to list audio sources")?;

        let sources = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(sources)
    }

    pub fn list_sinks(&self) -> Result<Vec<String>> {
        let output = Command::new("pactl")
            .args(["list", "sinks", "short"])
            .output()
            .context("Failed to list audio sinks")?;

        let sinks = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(sinks)
    }

    pub fn get_default_source(&self) -> Result<String> {
        let output = Command::new("pactl")
            .args(["get-default-source"])
            .output()
            .context("Failed to get default source")?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn set_default_source(&self, source: &str) -> Result<()> {
        let output = Command::new("pactl")
            .args(["set-default-source", source])
            .output()
            .context("Failed to set default source")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to set default source: {}", stderr);
        }

        Ok(())
    }


    pub fn route_all_app_audio_to_sink(&self) -> Result<()> {
        // Move all current soundboard/tauri-app sink inputs to Soundboard_Mix
        // This handles the case where audio is already playing
        let output = Command::new("sh")
            .arg("-c")
            .arg("pactl list sink-inputs | awk '/^Sink Input #/{id=$3} /application.name.*(soundboard|tauri-app)/{print id}' | sort -u | sed 's/#//'")
            .output()
            .context("Failed to list sink inputs")?;

        let sink_inputs = String::from_utf8_lossy(&output.stdout);
        for sink_input_id in sink_inputs.lines() {
            if !sink_input_id.trim().is_empty() {
                let _ = Command::new("pactl")
                    .args(["move-sink-input", sink_input_id.trim(), &self.sink_name])
                    .output();
            }
        }

        Ok(())
    }
}
