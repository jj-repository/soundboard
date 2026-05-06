//! Gamepad input listener: polls `gilrs` events on a background thread
//! and dispatches button presses into the shared `HotkeyAction` channel.
//!
//! Connections to gamepads are managed by gilrs internally; we just keep
//! its event loop alive. Drop sets the stop flag and joins the thread.

use crate::gui::hotkeys::HotkeyAction;
use gilrs::{Button, EventType, Gilrs};
use soundboard::types::config::GamepadBindings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct GamepadManager {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl GamepadManager {
    /// Initialise gamepad input. Returns `None` if gilrs cannot init or the
    /// feature is disabled in config.
    pub fn new(sender: Sender<HotkeyAction>, bindings: &GamepadBindings) -> Option<Self> {
        if !bindings.enabled {
            return None;
        }
        let gilrs = match Gilrs::new() {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!("Gamepad: failed to initialise gilrs: {}", e);
                return None;
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let bindings = bindings.clone();
        let handle = std::thread::spawn(move || {
            run_loop(gilrs, sender, bindings, stop_clone);
        });
        Some(GamepadManager {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for GamepadManager {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            // Best-effort: thread loops on a 50ms poll, so this is fast.
            if let Err(e) = handle.join() {
                tracing::error!("gamepad listener thread panicked on join: {:?}", e);
            }
        }
    }
}

fn run_loop(
    mut gilrs: Gilrs,
    sender: Sender<HotkeyAction>,
    bindings: GamepadBindings,
    stop: Arc<AtomicBool>,
) {
    let play_pause = bindings.play_pause_button.as_deref().and_then(parse_button);
    let stop_btn = bindings.stop_button.as_deref().and_then(parse_button);
    while !stop.load(Ordering::Acquire) {
        // Drain pending events; sleep briefly when idle to avoid busy-loop.
        let mut had_event = false;
        while let Some(event) = gilrs.next_event() {
            had_event = true;
            if let EventType::ButtonPressed(button, _) = event.event {
                if Some(button) == play_pause {
                    sender.send(HotkeyAction::PlayPause).ok();
                } else if Some(button) == stop_btn {
                    sender.send(HotkeyAction::Stop).ok();
                }
            }
        }
        if !had_event {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

/// Map a config string to a `gilrs::Button`. Returns `None` for unknown
/// names so config typos don't panic.
pub fn parse_button(name: &str) -> Option<Button> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "south" | "a" => Button::South,
        "east" | "b" => Button::East,
        "north" | "y" => Button::North,
        "west" | "x" => Button::West,
        "select" | "back" => Button::Select,
        "start" => Button::Start,
        "lefttrigger" | "lt" | "leftshoulder" | "lb" => Button::LeftTrigger,
        "righttrigger" | "rt" | "rightshoulder" | "rb" => Button::RightTrigger,
        "dpadup" => Button::DPadUp,
        "dpaddown" => Button::DPadDown,
        "dpadleft" => Button::DPadLeft,
        "dpadright" => Button::DPadRight,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_button_known_aliases() {
        assert_eq!(parse_button("south"), Some(Button::South));
        assert_eq!(parse_button("A"), Some(Button::South));
        assert_eq!(parse_button("east"), Some(Button::East));
        assert_eq!(parse_button("b"), Some(Button::East));
        assert_eq!(parse_button("Start"), Some(Button::Start));
    }

    #[test]
    fn parse_button_unknown_returns_none() {
        assert_eq!(parse_button(""), None);
        assert_eq!(parse_button("bogus"), None);
        assert_eq!(parse_button("xbox"), None);
    }
}
