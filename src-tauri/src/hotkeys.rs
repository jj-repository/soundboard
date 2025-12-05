use anyhow::Result;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub struct HotkeyManager {
    manager: GlobalHotKeyManager,
    registered: Arc<RwLock<HashMap<String, HotKey>>>,
}

impl HotkeyManager {
    pub fn new() -> Result<Self> {
        let manager = GlobalHotKeyManager::new()?;
        Ok(Self {
            manager,
            registered: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn register(&self, id: String, hotkey_string: &str) -> Result<()> {
        // Unregister if already exists
        if let Some(old_hotkey) = self.registered.read().get(&id) {
            let _ = self.manager.unregister(*old_hotkey);
        }

        // Parse hotkey string (e.g., "Ctrl+Alt+A")
        let hotkey = parse_hotkey(hotkey_string)?;

        self.manager.register(hotkey)?;
        self.registered.write().insert(id, hotkey);

        Ok(())
    }

    pub fn unregister(&self, id: &str) -> Result<()> {
        if let Some(hotkey) = self.registered.write().remove(id) {
            self.manager.unregister(hotkey)?;
        }
        Ok(())
    }

    pub fn unregister_all(&self) -> Result<()> {
        let hotkeys: Vec<HotKey> = self.registered.read().values().copied().collect();
        for hotkey in hotkeys {
            let _ = self.manager.unregister(hotkey);
        }
        self.registered.write().clear();
        Ok(())
    }

    pub fn get_receiver() -> Result<crossbeam_channel::Receiver<GlobalHotKeyEvent>> {
        Ok(GlobalHotKeyEvent::receiver().clone())
    }
}

fn parse_hotkey(s: &str) -> Result<HotKey> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();

    if parts.is_empty() {
        anyhow::bail!("Empty hotkey string");
    }

    let mut modifiers = Modifiers::empty();
    let mut key_code = None;

    for part in parts {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            "super" | "meta" | "win" => modifiers |= Modifiers::SUPER,
            key => {
                if key_code.is_some() {
                    anyhow::bail!("Multiple key codes in hotkey: {}", s);
                }
                key_code = Some(parse_key_code(key)?);
            }
        }
    }

    let code = key_code.ok_or_else(|| anyhow::anyhow!("No key code in hotkey: {}", s))?;
    Ok(HotKey::new(Some(modifiers), code))
}

fn parse_key_code(s: &str) -> Result<Code> {
    let code = match s.to_lowercase().as_str() {
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "escape" | "esc" => Code::Escape,
        "backspace" => Code::Backspace,
        "tab" => Code::Tab,
        "up" => Code::ArrowUp,
        "down" => Code::ArrowDown,
        "left" => Code::ArrowLeft,
        "right" => Code::ArrowRight,
        _ => anyhow::bail!("Unknown key: {}", s),
    };

    Ok(code)
}
