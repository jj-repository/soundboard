use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager,
    hotkey::{Code, HotKey, Modifiers},
};
use std::sync::mpsc;

pub enum HotkeyAction {
    PlayPause,
    Stop,
}

pub struct HotkeyManager {
    _manager: GlobalHotKeyManager,
    pub receiver: mpsc::Receiver<HotkeyAction>,
}

impl HotkeyManager {
    pub fn new() -> Option<Self> {
        let manager = match GlobalHotKeyManager::new() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to create hotkey manager: {}", e);
                return None;
            }
        };

        // Default hotkeys: Ctrl+Shift+P for Play/Pause, Ctrl+Shift+S for Stop
        let play_pause_hotkey = HotKey::new(
            Some(Modifiers::CONTROL | Modifiers::SHIFT),
            Code::KeyP,
        );
        let stop_hotkey = HotKey::new(
            Some(Modifiers::CONTROL | Modifiers::SHIFT),
            Code::KeyS,
        );

        let play_pause_id = play_pause_hotkey.id();
        let stop_id = stop_hotkey.id();

        if let Err(e) = manager.register(play_pause_hotkey) {
            eprintln!("Failed to register Play/Pause hotkey (Ctrl+Shift+P): {}", e);
        }

        if let Err(e) = manager.register(stop_hotkey) {
            eprintln!("Failed to register Stop hotkey (Ctrl+Shift+S): {}", e);
        }

        let (sender, receiver) = mpsc::channel();

        // Spawn a thread to listen for hotkey events
        std::thread::spawn(move || {
            let global_receiver = GlobalHotKeyEvent::receiver();
            loop {
                if let Ok(event) = global_receiver.recv() {
                    if event.state == global_hotkey::HotKeyState::Pressed {
                        if event.id == play_pause_id {
                            sender.send(HotkeyAction::PlayPause).ok();
                        } else if event.id == stop_id {
                            sender.send(HotkeyAction::Stop).ok();
                        }
                    }
                }
            }
        });

        Some(HotkeyManager {
            _manager: manager,
            receiver,
        })
    }
}
