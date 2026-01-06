use crate::gui::SoundpadGui;
use egui::{Context, Key};

impl SoundpadGui {
    pub fn handle_input(&mut self, ctx: &Context) {
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
