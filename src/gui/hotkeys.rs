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
                        let ids = ids_clone.read().unwrap();
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
        let mut ids = self.ids.write().unwrap();

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
        let mut ids = self.ids.write().unwrap();

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

/// Convert a keyboard event Code to a string representation
#[allow(dead_code)]
pub fn code_to_string(code: Code) -> Option<String> {
    match code {
        // Letters
        Code::KeyA => Some("KeyA".to_string()),
        Code::KeyB => Some("KeyB".to_string()),
        Code::KeyC => Some("KeyC".to_string()),
        Code::KeyD => Some("KeyD".to_string()),
        Code::KeyE => Some("KeyE".to_string()),
        Code::KeyF => Some("KeyF".to_string()),
        Code::KeyG => Some("KeyG".to_string()),
        Code::KeyH => Some("KeyH".to_string()),
        Code::KeyI => Some("KeyI".to_string()),
        Code::KeyJ => Some("KeyJ".to_string()),
        Code::KeyK => Some("KeyK".to_string()),
        Code::KeyL => Some("KeyL".to_string()),
        Code::KeyM => Some("KeyM".to_string()),
        Code::KeyN => Some("KeyN".to_string()),
        Code::KeyO => Some("KeyO".to_string()),
        Code::KeyP => Some("KeyP".to_string()),
        Code::KeyQ => Some("KeyQ".to_string()),
        Code::KeyR => Some("KeyR".to_string()),
        Code::KeyS => Some("KeyS".to_string()),
        Code::KeyT => Some("KeyT".to_string()),
        Code::KeyU => Some("KeyU".to_string()),
        Code::KeyV => Some("KeyV".to_string()),
        Code::KeyW => Some("KeyW".to_string()),
        Code::KeyX => Some("KeyX".to_string()),
        Code::KeyY => Some("KeyY".to_string()),
        Code::KeyZ => Some("KeyZ".to_string()),

        // Numbers
        Code::Digit0 => Some("Digit0".to_string()),
        Code::Digit1 => Some("Digit1".to_string()),
        Code::Digit2 => Some("Digit2".to_string()),
        Code::Digit3 => Some("Digit3".to_string()),
        Code::Digit4 => Some("Digit4".to_string()),
        Code::Digit5 => Some("Digit5".to_string()),
        Code::Digit6 => Some("Digit6".to_string()),
        Code::Digit7 => Some("Digit7".to_string()),
        Code::Digit8 => Some("Digit8".to_string()),
        Code::Digit9 => Some("Digit9".to_string()),

        // Function keys
        Code::F1 => Some("F1".to_string()),
        Code::F2 => Some("F2".to_string()),
        Code::F3 => Some("F3".to_string()),
        Code::F4 => Some("F4".to_string()),
        Code::F5 => Some("F5".to_string()),
        Code::F6 => Some("F6".to_string()),
        Code::F7 => Some("F7".to_string()),
        Code::F8 => Some("F8".to_string()),
        Code::F9 => Some("F9".to_string()),
        Code::F10 => Some("F10".to_string()),
        Code::F11 => Some("F11".to_string()),
        Code::F12 => Some("F12".to_string()),

        // Special keys
        Code::Space => Some("Space".to_string()),
        Code::Enter => Some("Enter".to_string()),
        Code::Escape => Some("Escape".to_string()),
        Code::Backspace => Some("Backspace".to_string()),
        Code::Tab => Some("Tab".to_string()),
        Code::Delete => Some("Delete".to_string()),
        Code::Insert => Some("Insert".to_string()),
        Code::Home => Some("Home".to_string()),
        Code::End => Some("End".to_string()),
        Code::PageUp => Some("PageUp".to_string()),
        Code::PageDown => Some("PageDown".to_string()),

        // Arrow keys
        Code::ArrowUp => Some("ArrowUp".to_string()),
        Code::ArrowDown => Some("ArrowDown".to_string()),
        Code::ArrowLeft => Some("ArrowLeft".to_string()),
        Code::ArrowRight => Some("ArrowRight".to_string()),

        // Numpad
        Code::Numpad0 => Some("Numpad0".to_string()),
        Code::Numpad1 => Some("Numpad1".to_string()),
        Code::Numpad2 => Some("Numpad2".to_string()),
        Code::Numpad3 => Some("Numpad3".to_string()),
        Code::Numpad4 => Some("Numpad4".to_string()),
        Code::Numpad5 => Some("Numpad5".to_string()),
        Code::Numpad6 => Some("Numpad6".to_string()),
        Code::Numpad7 => Some("Numpad7".to_string()),
        Code::Numpad8 => Some("Numpad8".to_string()),
        Code::Numpad9 => Some("Numpad9".to_string()),
        Code::NumpadAdd => Some("NumpadAdd".to_string()),
        Code::NumpadSubtract => Some("NumpadSubtract".to_string()),
        Code::NumpadMultiply => Some("NumpadMultiply".to_string()),
        Code::NumpadDivide => Some("NumpadDivide".to_string()),
        Code::NumpadEnter => Some("NumpadEnter".to_string()),
        Code::NumpadDecimal => Some("NumpadDecimal".to_string()),

        // Punctuation
        Code::Minus => Some("Minus".to_string()),
        Code::Equal => Some("Equal".to_string()),
        Code::BracketLeft => Some("BracketLeft".to_string()),
        Code::BracketRight => Some("BracketRight".to_string()),
        Code::Backslash => Some("Backslash".to_string()),
        Code::Semicolon => Some("Semicolon".to_string()),
        Code::Quote => Some("Quote".to_string()),
        Code::Comma => Some("Comma".to_string()),
        Code::Period => Some("Period".to_string()),
        Code::Slash => Some("Slash".to_string()),
        Code::Backquote => Some("Backquote".to_string()),

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
