//! Auto Push-to-Talk: while a clip plays, simulate holding the user's
//! configured PTT key so voice-chat apps gated on PTT actually transmit
//! the soundboard's virtual-mic output.
//!
//! Driven by polling: callers periodically pass the current "is any audio
//! playing?" boolean to [`tick`], which presses or releases the key on
//! transitions. This is simpler than refcounted explicit start/end calls
//! and tolerates unbalanced begin/end events. The 100 ms poll cadence in
//! `daemon_run::player_loop` is well below voice-chat human-perception
//! thresholds.
//!
//! Off by default. The synthetic input is generated via Win32 `SendInput`;
//! it is dual-use technology and may be flagged by some kernel-level
//! anti-cheats. Surface a UI warning before users enable it.

use crate::types::config::AutoPttConfig;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

struct AutoPttState {
    config: Mutex<AutoPttConfig>,
    /// Whether `SendInput` keydown is currently outstanding (waiting for keyup).
    key_pressed: AtomicBool,
    /// VK code we sent the most recent keydown for, so we know which VK to
    /// release. `None` if no keydown outstanding.
    pressed_vk: Mutex<Option<u16>>,
}

static STATE: AutoPttState = AutoPttState {
    config: Mutex::new(AutoPttConfig::DISABLED),
    key_pressed: AtomicBool::new(false),
    pressed_vk: Mutex::new(None),
};

/// Replace the active config. If a key is currently pressed for the old
/// config, releases it before switching.
pub fn set_config(new_cfg: AutoPttConfig) {
    release_pressed();
    let mut guard = match STATE.config.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    *guard = new_cfg;
}

/// Snapshot of the active config.
pub fn get_config() -> AutoPttConfig {
    match STATE.config.lock() {
        Ok(g) => g.clone(),
        Err(p) => p.into_inner().clone(),
    }
}

/// Driver: pass `any_playing == true` while audio is producing samples and
/// `false` once it stops. Press/release is edge-triggered, so calling this
/// every poll iteration is cheap.
pub fn tick(any_playing: bool) {
    let pressed = STATE.key_pressed.load(Ordering::Acquire);
    if any_playing && !pressed {
        if let Some(vk) = active_vk() {
            send_keydown(vk);
        }
    } else if !any_playing && pressed {
        release_pressed();
    }
}

/// Force-release any outstanding keypress. Call from shutdown / panic /
/// Ctrl-C handlers to avoid leaving a key stuck down for the user.
pub fn release_all() {
    release_pressed();
}

fn active_vk() -> Option<u16> {
    let cfg = get_config();
    if !cfg.enabled {
        return None;
    }
    cfg.key.as_deref().and_then(key_string_to_vk)
}

fn release_pressed() {
    if STATE.key_pressed.swap(false, Ordering::AcqRel) {
        let vk_opt = {
            let mut guard = match STATE.pressed_vk.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            guard.take()
        };
        if let Some(vk) = vk_opt {
            send_keyup(vk);
        }
    }
}

#[cfg(target_os = "windows")]
fn send_keydown(vk: u16) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, VIRTUAL_KEY,
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: KEYBD_EVENT_FLAGS(0),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if sent != 1 {
        tracing::error!(
            "auto_ptt: SendInput keydown failed (returned {}); skipping press",
            sent
        );
        return;
    }
    {
        let mut guard = match STATE.pressed_vk.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *guard = Some(vk);
    }
    STATE.key_pressed.store(true, Ordering::Release);
    tracing::debug!("auto_ptt: keydown vk=0x{:02X}", vk);
}

#[cfg(target_os = "windows")]
fn send_keyup(vk: u16) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if sent != 1 {
        tracing::error!(
            "auto_ptt: SendInput keyup failed (returned {}); key may stay pressed",
            sent
        );
    } else {
        tracing::debug!("auto_ptt: keyup vk=0x{:02X}", vk);
    }
}

#[cfg(not(target_os = "windows"))]
fn send_keydown(_vk: u16) {}

#[cfg(not(target_os = "windows"))]
fn send_keyup(_vk: u16) {}

/// Map a string-form key code (same vocabulary as
/// `crate::types::config::HotkeyBinding::key`) to a Win32 virtual-key code.
/// Returns `None` for unmapped strings.
pub fn key_string_to_vk(key: &str) -> Option<u16> {
    let vk = match key {
        // Letters: VK code = ASCII uppercase byte.
        k if k.starts_with("Key") && k.len() == 4 => {
            let c = k.as_bytes()[3];
            if c.is_ascii_uppercase() {
                c as u16
            } else {
                return None;
            }
        }
        // Top-row digits: VK code = '0'..='9'.
        k if k.starts_with("Digit") && k.len() == 6 => {
            let c = k.as_bytes()[5];
            if c.is_ascii_digit() {
                c as u16
            } else {
                return None;
            }
        }
        "F1" => 0x70,
        "F2" => 0x71,
        "F3" => 0x72,
        "F4" => 0x73,
        "F5" => 0x74,
        "F6" => 0x75,
        "F7" => 0x76,
        "F8" => 0x77,
        "F9" => 0x78,
        "F10" => 0x79,
        "F11" => 0x7A,
        "F12" => 0x7B,

        "Space" => 0x20,
        "Enter" => 0x0D,
        "Escape" => 0x1B,
        "Backspace" => 0x08,
        "Tab" => 0x09,
        "Delete" => 0x2E,
        "Insert" => 0x2D,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,

        "ArrowUp" => 0x26,
        "ArrowDown" => 0x28,
        "ArrowLeft" => 0x25,
        "ArrowRight" => 0x27,

        "Numpad0" => 0x60,
        "Numpad1" => 0x61,
        "Numpad2" => 0x62,
        "Numpad3" => 0x63,
        "Numpad4" => 0x64,
        "Numpad5" => 0x65,
        "Numpad6" => 0x66,
        "Numpad7" => 0x67,
        "Numpad8" => 0x68,
        "Numpad9" => 0x69,
        "NumpadMultiply" => 0x6A,
        "NumpadAdd" => 0x6B,
        "NumpadSubtract" => 0x6D,
        "NumpadDecimal" => 0x6E,
        "NumpadDivide" => 0x6F,
        "NumpadEnter" => 0x0D,

        "Minus" => 0xBD,
        "Equal" => 0xBB,
        "BracketLeft" => 0xDB,
        "BracketRight" => 0xDD,
        "Backslash" => 0xDC,
        "Semicolon" => 0xBA,
        "Quote" => 0xDE,
        "Comma" => 0xBC,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "Backquote" => 0xC0,

        _ => return None,
    };
    Some(vk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_string_to_vk_letters() {
        assert_eq!(key_string_to_vk("KeyA"), Some(0x41));
        assert_eq!(key_string_to_vk("KeyZ"), Some(0x5A));
    }

    #[test]
    fn key_string_to_vk_digits() {
        assert_eq!(key_string_to_vk("Digit0"), Some(0x30));
        assert_eq!(key_string_to_vk("Digit9"), Some(0x39));
    }

    #[test]
    fn key_string_to_vk_function_special_arrow() {
        assert_eq!(key_string_to_vk("F1"), Some(0x70));
        assert_eq!(key_string_to_vk("F12"), Some(0x7B));
        assert_eq!(key_string_to_vk("Space"), Some(0x20));
        assert_eq!(key_string_to_vk("Escape"), Some(0x1B));
        assert_eq!(key_string_to_vk("ArrowLeft"), Some(0x25));
    }

    #[test]
    fn key_string_to_vk_numpad_punct() {
        assert_eq!(key_string_to_vk("Numpad5"), Some(0x65));
        assert_eq!(key_string_to_vk("NumpadEnter"), Some(0x0D));
        assert_eq!(key_string_to_vk("Backquote"), Some(0xC0));
        assert_eq!(key_string_to_vk("Slash"), Some(0xBF));
    }

    #[test]
    fn key_string_to_vk_unknown_returns_none() {
        assert_eq!(key_string_to_vk(""), None);
        assert_eq!(key_string_to_vk("Bogus"), None);
        assert_eq!(key_string_to_vk("KeyAB"), None);
        assert_eq!(key_string_to_vk("F13"), None);
    }

    /// Disabled config: tick() is a no-op even with playing=true.
    /// Verifies the early-exit path; SendInput is never reached when the
    /// VK lookup returns None.
    #[test]
    fn tick_disabled_does_nothing() {
        set_config(AutoPttConfig {
            enabled: false,
            key: Some("KeyV".to_string()),
        });
        tick(true);
        assert!(!STATE.key_pressed.load(Ordering::Acquire));
        tick(false);
        assert!(!STATE.key_pressed.load(Ordering::Acquire));
    }

    /// Enabled with unmapped key: tick() is a no-op (no VK found).
    #[test]
    fn tick_enabled_but_no_vk_does_nothing() {
        set_config(AutoPttConfig {
            enabled: true,
            key: Some("Bogus".to_string()),
        });
        tick(true);
        assert!(!STATE.key_pressed.load(Ordering::Acquire));
    }

    #[test]
    fn set_config_round_trips() {
        let cfg = AutoPttConfig {
            enabled: true,
            key: Some("KeyV".to_string()),
        };
        set_config(cfg.clone());
        assert_eq!(get_config(), cfg);
        set_config(AutoPttConfig::DISABLED);
        assert_eq!(get_config(), AutoPttConfig::DISABLED);
    }
}
