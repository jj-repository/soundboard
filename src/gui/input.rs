use crate::gui::SoundpadGui;
use crate::gui::draw::create_hotkey_binding;
use egui::{Context, Key};
use pwsp::types::gui::HotkeyRecording;

impl SoundpadGui {
    pub fn handle_input(&mut self, ctx: &Context) {
        // Handle hotkey recording first (captures all key input when recording)
        if self.app_state.recording_hotkey.is_some() {
            let mut recorded_binding = None;
            let mut cancel_recording = false;

            ctx.input(|i| {
                // Cancel on Escape
                if i.key_pressed(Key::Escape) {
                    cancel_recording = true;
                    return;
                }

                // Look for a key press (non-modifier)
                for key in [
                    Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I,
                    Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R,
                    Key::S, Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z,
                    Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4,
                    Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9,
                    Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6,
                    Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12,
                    Key::Space, Key::Enter, Key::Backspace, Key::Tab, Key::Delete,
                    Key::Insert, Key::Home, Key::End, Key::PageUp, Key::PageDown,
                    Key::ArrowUp, Key::ArrowDown, Key::ArrowLeft, Key::ArrowRight,
                    Key::Minus, Key::Equals, Key::OpenBracket, Key::CloseBracket,
                    Key::Backslash, Key::Semicolon, Key::Quote, Key::Comma,
                    Key::Period, Key::Slash, Key::Backtick,
                ] {
                    if i.key_pressed(key) {
                        recorded_binding = create_hotkey_binding(key, i.modifiers);
                        break;
                    }
                }
            });

            if cancel_recording {
                self.app_state.recording_hotkey = None;
                return;
            }

            if let Some(binding) = recorded_binding {
                match self.app_state.recording_hotkey {
                    Some(HotkeyRecording::PlayPause) => {
                        self.config.hotkeys.play_pause = Some(binding);
                    }
                    Some(HotkeyRecording::Stop) => {
                        self.config.hotkeys.stop = Some(binding);
                    }
                    None => {}
                }
                self.app_state.recording_hotkey = None;
                self.config.save_to_file().ok();
                self.update_hotkeys();
            }

            return; // Don't process other input while recording
        }

        if ctx.memory(|reader| reader.focused().is_some()) {
            return;
        }

        ctx.input(|i| {
            // Close app on escape
            if i.key_pressed(Key::Escape) {
                std::process::exit(0);
            }

            // Open/close settings
            if i.key_pressed(Key::I) {
                self.app_state.show_settings = !self.app_state.show_settings;
            }

            if i.key_pressed(Key::Enter) {
                if let Some(selected_file) = self.app_state.selected_file.clone() {
                    self.play_file(&selected_file);
                }
            }

            if !self.app_state.show_settings {
                // Pause / resume audio on space
                if i.key_pressed(Key::Space) {
                    self.play_toggle();
                }

                // Focus search field
                if i.key_pressed(Key::Slash) {
                    self.app_state.force_focus_id = self.app_state.search_field_id;
                }

                // Navigate through dirs/files with Ctrl+Arrow keys
                if i.modifiers.ctrl {
                    let arrow_up = i.key_pressed(Key::ArrowUp);
                    let arrow_down = i.key_pressed(Key::ArrowDown);

                    if arrow_up || arrow_down {
                        let delta = if arrow_down { 1isize } else { -1 };

                        if i.modifiers.shift && !self.app_state.dirs.is_empty() {
                            // Navigate directories
                            let mut dirs: Vec<_> = self.app_state.dirs.iter().cloned().collect();
                            dirs.sort();

                            let current_idx = self
                                .app_state
                                .current_dir
                                .as_ref()
                                .and_then(|d| dirs.iter().position(|x| x == d))
                                .map(|i| i as isize)
                                .unwrap_or(-1);

                            let new_idx = wrap_index(current_idx + delta, dirs.len());
                            self.open_dir(&dirs[new_idx]);
                        } else if self.app_state.current_dir.is_some() {
                            // Navigate files
                            let mut files: Vec<_> = self.app_state.files.iter().cloned().collect();
                            files.sort();

                            if !files.is_empty() {
                                let current_idx = self
                                    .app_state
                                    .selected_file
                                    .as_ref()
                                    .and_then(|f| files.iter().position(|x| x == f))
                                    .map(|i| i as isize)
                                    .unwrap_or(-1);

                                let new_idx = wrap_index(current_idx + delta, files.len());
                                self.app_state.selected_file = Some(files[new_idx].clone());
                            }
                        }
                    }
                }
            }
        });
    }
}

/// Wraps an index to stay within bounds [0, len)
fn wrap_index(idx: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = len as isize;
    ((idx % len + len) % len) as usize
}
