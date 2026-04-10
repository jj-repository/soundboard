use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager,
    hotkey::{Code, HotKey, Modifiers},
};
use pwsp::types::config::{HotkeyBinding, HotkeyConfig};
use std::sync::{mpsc, Arc, RwLock};

pub enum HotkeyAction {
    PlayPause,
    Stop,
}

/// Shared state for hotkey IDs that can be updated at runtime
#[derive(Default)]
struct HotkeyIds {
    play_pause_id: Option<u32>,
    stop_id: Option<u32>,
}

pub struct HotkeyManager {
    manager: GlobalHotKeyManager,
    pub receiver: mpsc::Receiver<HotkeyAction>,
    sender: mpsc::Sender<HotkeyAction>,
    ids: Arc<RwLock<HotkeyIds>>,
    play_pause_binding: Option<HotkeyBinding>,
    stop_binding: Option<HotkeyBinding>,
}

impl HotkeyManager {
    pub fn new(config: &HotkeyConfig) -> Option<Self> {
        let manager = match GlobalHotKeyManager::new() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to create hotkey manager: {}", e);
                return None;
            }
        };

        let (sender, receiver) = mpsc::channel();
        let ids = Arc::new(RwLock::new(HotkeyIds::default()));

        let mut hotkey_manager = HotkeyManager {
            manager,
            receiver,
            sender,
            ids: ids.clone(),
            play_pause_binding: None,
            stop_binding: None,
        };

        if config.enabled {
            hotkey_manager.register_hotkeys(config);
        }

        // Start the event listener thread
        let sender_clone = hotkey_manager.sender.clone();
        let ids_clone = ids;

        std::thread::spawn(move || {
            let global_receiver = GlobalHotKeyEvent::receiver();
            loop {
                if let Ok(event) = global_receiver.recv() {
                    if event.state == global_hotkey::HotKeyState::Pressed {
                        // Use unwrap_or_else to handle poisoned lock gracefully
                        let ids = match ids_clone.read() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                eprintln!("Warning: Hotkey IDs lock was poisoned, recovering...");
                                poisoned.into_inner()
                            }
                        };
                        if Some(event.id) == ids.play_pause_id {
                            sender_clone.send(HotkeyAction::PlayPause).ok();
                        } else if Some(event.id) == ids.stop_id {
                            sender_clone.send(HotkeyAction::Stop).ok();
                        }
                    }
                }
            }
        });

        Some(hotkey_manager)
    }

    fn register_hotkeys(&mut self, config: &HotkeyConfig) {
        let mut ids = match self.ids.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Warning: Hotkey IDs lock was poisoned during register, recovering...");
                poisoned.into_inner()
            }
        };

        // Register Play/Pause hotkey
        if let Some(ref binding) = config.play_pause {
            if let Some(hotkey) = binding_to_hotkey(binding) {
                ids.play_pause_id = Some(hotkey.id());
                self.play_pause_binding = Some(binding.clone());
                if let Err(e) = self.manager.register(hotkey) {
                    eprintln!(
                        "Failed to register Play/Pause hotkey ({}): {}",
                        binding.display(),
                        e
                    );
                    ids.play_pause_id = None;
                    self.play_pause_binding = None;
                }
            }
        }

        // Register Stop hotkey
        if let Some(ref binding) = config.stop {
            if let Some(hotkey) = binding_to_hotkey(binding) {
                ids.stop_id = Some(hotkey.id());
                self.stop_binding = Some(binding.clone());
                if let Err(e) = self.manager.register(hotkey) {
                    eprintln!(
                        "Failed to register Stop hotkey ({}): {}",
                        binding.display(),
                        e
                    );
                    ids.stop_id = None;
                    self.stop_binding = None;
                }
            }
        }
    }

    fn unregister_hotkeys(&mut self) {
        let mut ids = match self.ids.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Warning: Hotkey IDs lock was poisoned during unregister, recovering...");
                poisoned.into_inner()
            }
        };

        // Unregister Play/Pause hotkey
        if let Some(ref binding) = self.play_pause_binding.take() {
            if let Some(hotkey) = binding_to_hotkey(binding) {
                let _ = self.manager.unregister(hotkey);
            }
        }
        ids.play_pause_id = None;

        // Unregister Stop hotkey
        if let Some(ref binding) = self.stop_binding.take() {
            if let Some(hotkey) = binding_to_hotkey(binding) {
                let _ = self.manager.unregister(hotkey);
            }
        }
        ids.stop_id = None;
    }

    pub fn update_hotkeys(&mut self, config: &HotkeyConfig) {
        // Unregister existing hotkeys
        self.unregister_hotkeys();

        // Re-register if enabled
        if config.enabled {
            self.register_hotkeys(config);
        }
    }
}

/// Convert a HotkeyBinding to a global_hotkey HotKey
fn binding_to_hotkey(binding: &HotkeyBinding) -> Option<HotKey> {
    let code = string_to_code(&binding.key)?;
    let mut modifiers = Modifiers::empty();

    if binding.ctrl {
        modifiers |= Modifiers::CONTROL;
    }
    if binding.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if binding.alt {
        modifiers |= Modifiers::ALT;
    }
    if binding.super_key {
        modifiers |= Modifiers::SUPER;
    }

    let mods = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };

    Some(HotKey::new(mods, code))
}

/// Convert a string key name to a Code
fn string_to_code(key: &str) -> Option<Code> {
    match key {
        // Letters
        "KeyA" => Some(Code::KeyA),
        "KeyB" => Some(Code::KeyB),
        "KeyC" => Some(Code::KeyC),
        "KeyD" => Some(Code::KeyD),
        "KeyE" => Some(Code::KeyE),
        "KeyF" => Some(Code::KeyF),
        "KeyG" => Some(Code::KeyG),
        "KeyH" => Some(Code::KeyH),
        "KeyI" => Some(Code::KeyI),
        "KeyJ" => Some(Code::KeyJ),
        "KeyK" => Some(Code::KeyK),
        "KeyL" => Some(Code::KeyL),
        "KeyM" => Some(Code::KeyM),
        "KeyN" => Some(Code::KeyN),
        "KeyO" => Some(Code::KeyO),
        "KeyP" => Some(Code::KeyP),
        "KeyQ" => Some(Code::KeyQ),
        "KeyR" => Some(Code::KeyR),
        "KeyS" => Some(Code::KeyS),
        "KeyT" => Some(Code::KeyT),
        "KeyU" => Some(Code::KeyU),
        "KeyV" => Some(Code::KeyV),
        "KeyW" => Some(Code::KeyW),
        "KeyX" => Some(Code::KeyX),
        "KeyY" => Some(Code::KeyY),
        "KeyZ" => Some(Code::KeyZ),

        // Numbers
        "Digit0" => Some(Code::Digit0),
        "Digit1" => Some(Code::Digit1),
        "Digit2" => Some(Code::Digit2),
        "Digit3" => Some(Code::Digit3),
        "Digit4" => Some(Code::Digit4),
        "Digit5" => Some(Code::Digit5),
        "Digit6" => Some(Code::Digit6),
        "Digit7" => Some(Code::Digit7),
        "Digit8" => Some(Code::Digit8),
        "Digit9" => Some(Code::Digit9),

        // Function keys
        "F1" => Some(Code::F1),
        "F2" => Some(Code::F2),
        "F3" => Some(Code::F3),
        "F4" => Some(Code::F4),
        "F5" => Some(Code::F5),
        "F6" => Some(Code::F6),
        "F7" => Some(Code::F7),
        "F8" => Some(Code::F8),
        "F9" => Some(Code::F9),
        "F10" => Some(Code::F10),
        "F11" => Some(Code::F11),
        "F12" => Some(Code::F12),

        // Special keys
        "Space" => Some(Code::Space),
        "Enter" => Some(Code::Enter),
        "Escape" => Some(Code::Escape),
        "Backspace" => Some(Code::Backspace),
        "Tab" => Some(Code::Tab),
        "Delete" => Some(Code::Delete),
        "Insert" => Some(Code::Insert),
        "Home" => Some(Code::Home),
        "End" => Some(Code::End),
        "PageUp" => Some(Code::PageUp),
        "PageDown" => Some(Code::PageDown),

        // Arrow keys
        "ArrowUp" => Some(Code::ArrowUp),
        "ArrowDown" => Some(Code::ArrowDown),
        "ArrowLeft" => Some(Code::ArrowLeft),
        "ArrowRight" => Some(Code::ArrowRight),

        // Numpad
        "Numpad0" => Some(Code::Numpad0),
        "Numpad1" => Some(Code::Numpad1),
        "Numpad2" => Some(Code::Numpad2),
        "Numpad3" => Some(Code::Numpad3),
        "Numpad4" => Some(Code::Numpad4),
        "Numpad5" => Some(Code::Numpad5),
        "Numpad6" => Some(Code::Numpad6),
        "Numpad7" => Some(Code::Numpad7),
        "Numpad8" => Some(Code::Numpad8),
        "Numpad9" => Some(Code::Numpad9),
        "NumpadAdd" => Some(Code::NumpadAdd),
        "NumpadSubtract" => Some(Code::NumpadSubtract),
        "NumpadMultiply" => Some(Code::NumpadMultiply),
        "NumpadDivide" => Some(Code::NumpadDivide),
        "NumpadEnter" => Some(Code::NumpadEnter),
        "NumpadDecimal" => Some(Code::NumpadDecimal),

        // Punctuation
        "Minus" => Some(Code::Minus),
        "Equal" => Some(Code::Equal),
        "BracketLeft" => Some(Code::BracketLeft),
        "BracketRight" => Some(Code::BracketRight),
        "Backslash" => Some(Code::Backslash),
        "Semicolon" => Some(Code::Semicolon),
        "Quote" => Some(Code::Quote),
        "Comma" => Some(Code::Comma),
        "Period" => Some(Code::Period),
        "Slash" => Some(Code::Slash),
        "Backquote" => Some(Code::Backquote),

        _ => None,
    }
}

/// Convert a key name to a human-readable display name
pub fn key_display_name(key: &str) -> String {
    match key {
        // Letters - just show the letter
        k if k.starts_with("Key") => k[3..].to_string(),

        // Numbers
        k if k.starts_with("Digit") => k[5..].to_string(),

        // Numpad
        k if k.starts_with("Numpad") => format!("Num {}", &k[6..]),

        // Function keys
        k if k.starts_with('F') && k.len() <= 3 => k.to_string(),

        // Special keys
        "Space" => "Space".to_string(),
        "Enter" => "Enter".to_string(),
        "Escape" => "Esc".to_string(),
        "Backspace" => "Backspace".to_string(),
        "Tab" => "Tab".to_string(),
        "Delete" => "Del".to_string(),
        "Insert" => "Ins".to_string(),
        "Home" => "Home".to_string(),
        "End" => "End".to_string(),
        "PageUp" => "PgUp".to_string(),
        "PageDown" => "PgDn".to_string(),

        // Arrow keys
        "ArrowUp" => "Up".to_string(),
        "ArrowDown" => "Down".to_string(),
        "ArrowLeft" => "Left".to_string(),
        "ArrowRight" => "Right".to_string(),

        // Punctuation
        "Minus" => "-".to_string(),
        "Equal" => "=".to_string(),
        "BracketLeft" => "[".to_string(),
        "BracketRight" => "]".to_string(),
        "Backslash" => "\\".to_string(),
        "Semicolon" => ";".to_string(),
        "Quote" => "'".to_string(),
        "Comma" => ",".to_string(),
        "Period" => ".".to_string(),
        "Slash" => "/".to_string(),
        "Backquote" => "`".to_string(),

        _ => key.to_string(),
    }
}
