use crate::gui::{SUPPORTED_EXTENSIONS, SoundpadGui};
use crate::gui::hotkeys::key_display_name;
use egui::{
    Align, AtomExt, Button, Color32, ComboBox, FontFamily, Key, Label, Layout, Modifiers, RichText,
    ScrollArea, Slider, TextEdit, Ui, Vec2,
};
use egui_material_icons::icons;
use pwsp::types::audio_player::PlayerState;
use pwsp::types::config::HotkeyBinding;
use pwsp::types::gui::{HotkeyRecording, UpdateStatus};
use pwsp::utils::gui::format_time_pair;
use pwsp::utils::updater::get_current_version;
use std::error::Error;

impl SoundpadGui {
    pub fn draw_waiting_for_daemon(&mut self, ui: &mut Ui) {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Waiting for PWSP daemon to start...")
                    .size(34.0)
                    .monospace(),
            );
        });
    }

    pub fn draw_settings(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 5.0;
            // --------- Back Button and Title ----------
            ui.horizontal_top(|ui| {
                let back_button = Button::new(icons::ICON_ARROW_BACK).frame(false);
                let back_button_response = ui.add(back_button);
                if back_button_response.clicked() {
                    self.app_state.show_settings = false;
                }

                ui.add_space(ui.available_width() / 2.0 - 40.0);

                ui.label(RichText::new("Settings").color(Color32::WHITE).monospace());
            });
            // --------------------------------

            ui.separator();
            ui.add_space(20.0);

            // --------- Checkboxes ----------
            let save_volume_response =
                ui.checkbox(&mut self.config.save_volume, "Always remember volume");
            let save_gain_response =
                ui.checkbox(&mut self.config.save_gain, "Always remember gain boost");
            let save_mic_gain_response =
                ui.checkbox(&mut self.config.save_mic_gain, "Always remember mic gain");
            let save_input_response =
                ui.checkbox(&mut self.config.save_input, "Always remember microphone");
            let save_scale_response = ui.checkbox(
                &mut self.config.save_scale_factor,
                "Always remember UI scale factor",
            );
            let pause_on_exit_response = ui.checkbox(
                &mut self.config.pause_on_exit,
                "Pause audio playback when the window is closed",
            );

            if save_volume_response.changed()
                || save_gain_response.changed()
                || save_mic_gain_response.changed()
                || save_input_response.changed()
                || save_scale_response.changed()
                || pause_on_exit_response.changed()
            {
                self.config.save_to_file().ok();
            }
            // --------------------------------

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            // --------- Output Device Selection ----------
            ui.label(RichText::new("Audio Output").color(Color32::WHITE).monospace());
            ui.add_space(5.0);

            let mut outputs: Vec<(&String, &String)> =
                self.audio_player_state.all_outputs.iter().collect();
            outputs.sort_by_key(|(k, _)| *k);

            let current_output = self.audio_player_state.current_output.clone();
            let mut selected_output = current_output.clone();

            ComboBox::from_label("Output device")
                .selected_text(if current_output.is_empty() {
                    "Default"
                } else {
                    &current_output
                })
                .show_ui(ui, |ui| {
                    for (name, _) in outputs {
                        ui.selectable_value(&mut selected_output, name.to_owned(), name);
                    }
                });

            if selected_output != current_output && !selected_output.is_empty() {
                self.set_output(selected_output);
            }

            ui.label(
                RichText::new("Note: Changing output device requires restarting the daemon")
                    .weak()
                    .size(11.0),
            );
            // --------------------------------

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            // --------- Global Hotkeys Section ----------
            ui.label(RichText::new("Global Hotkeys").color(Color32::WHITE).monospace());
            ui.add_space(5.0);

            let hotkeys_enabled_response =
                ui.checkbox(&mut self.config.hotkeys.enabled, "Enable global hotkeys");
            if hotkeys_enabled_response.changed() {
                self.config.save_to_file().ok();
                self.update_hotkeys();
            }

            ui.add_space(10.0);

            // Play/Pause hotkey
            ui.horizontal(|ui| {
                ui.label("Play/Pause:");
                ui.add_space(10.0);

                let is_recording_play_pause =
                    self.app_state.recording_hotkey == Some(HotkeyRecording::PlayPause);

                if is_recording_play_pause {
                    ui.label(RichText::new("Press keys...").color(Color32::YELLOW));
                    if ui.button("Cancel").clicked() {
                        self.app_state.recording_hotkey = None;
                    }
                } else {
                    let display_text = self
                        .config
                        .hotkeys
                        .play_pause
                        .as_ref()
                        .map(|b| format_hotkey_display(b))
                        .unwrap_or_else(|| "Not set".to_string());

                    ui.label(&display_text);
                    ui.add_space(10.0);

                    if ui.button("Record").clicked() {
                        self.app_state.recording_hotkey = Some(HotkeyRecording::PlayPause);
                    }
                    if self.config.hotkeys.play_pause.is_some() && ui.button("Clear").clicked() {
                        self.config.hotkeys.play_pause = None;
                        self.config.save_to_file().ok();
                        self.update_hotkeys();
                    }
                }
            });

            // Stop hotkey
            ui.horizontal(|ui| {
                ui.label("Stop:");
                ui.add_space(10.0);

                let is_recording_stop =
                    self.app_state.recording_hotkey == Some(HotkeyRecording::Stop);

                if is_recording_stop {
                    ui.label(RichText::new("Press keys...").color(Color32::YELLOW));
                    if ui.button("Cancel").clicked() {
                        self.app_state.recording_hotkey = None;
                    }
                } else {
                    let display_text = self
                        .config
                        .hotkeys
                        .stop
                        .as_ref()
                        .map(|b| format_hotkey_display(b))
                        .unwrap_or_else(|| "Not set".to_string());

                    ui.label(&display_text);
                    ui.add_space(10.0);

                    if ui.button("Record").clicked() {
                        self.app_state.recording_hotkey = Some(HotkeyRecording::Stop);
                    }
                    if self.config.hotkeys.stop.is_some() && ui.button("Clear").clicked() {
                        self.config.hotkeys.stop = None;
                        self.config.save_to_file().ok();
                        self.update_hotkeys();
                    }
                }
            });

            ui.add_space(5.0);
            ui.label(
                RichText::new("Click 'Record' then press your desired key combination")
                    .weak()
                    .size(11.0),
            );
            // --------------------------------

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            // --------- Updates Section ----------
            ui.label(RichText::new("Updates").color(Color32::WHITE).monospace());
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label(format!("Current version: v{}", get_current_version()));
            });

            match &self.app_state.update_status {
                UpdateStatus::NotChecked => {
                    if ui.button("Check for updates").clicked() {
                        self.check_for_updates();
                    }
                }
                UpdateStatus::Checking => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Checking for updates...");
                    });
                }
                UpdateStatus::UpToDate => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("You're up to date!").color(Color32::GREEN));
                        if ui.small_button("Check again").clicked() {
                            self.check_for_updates();
                        }
                    });
                }
                UpdateStatus::UpdateAvailable { latest_version, release_url, download_url } => {
                    let latest_version = latest_version.clone();
                    let release_url = release_url.clone();
                    let download_url = download_url.clone();

                    ui.label(
                        RichText::new(format!("New version available: v{}", latest_version))
                            .color(Color32::YELLOW),
                    );

                    let mut should_download = false;
                    let mut download_url_to_use = None;
                    let mut should_open_release = false;

                    ui.horizontal(|ui| {
                        if let Some(url) = &download_url {
                            if ui.button("Download update").clicked() {
                                should_download = true;
                                download_url_to_use = Some(url.clone());
                            }
                        }
                        if ui.button("View release").clicked() {
                            should_open_release = true;
                        }
                    });

                    if should_download {
                        if let Some(url) = download_url_to_use {
                            self.download_update(url);
                        }
                    }
                    if should_open_release {
                        let _ = open::that(&release_url);
                    }
                }
                UpdateStatus::Downloading { progress } => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Downloading... {:.0}%", progress * 100.0));
                    });
                }
                UpdateStatus::Downloaded { file_path } => {
                    let file_path = file_path.clone();
                    ui.label(RichText::new("Update downloaded!").color(Color32::GREEN));
                    ui.label(format!("Saved to: {}", file_path.display()));
                    if ui.button("Open download location").clicked() {
                        if let Some(parent) = file_path.parent() {
                            let _ = open::that(parent);
                        }
                    }
                }
                UpdateStatus::Error(msg) => {
                    let msg = msg.clone();
                    ui.label(RichText::new(format!("Error: {}", msg)).color(Color32::RED));
                    if ui.small_button("Try again").clicked() {
                        self.check_for_updates();
                    }
                }
            }
            // --------------------------------
        });
    }

    pub fn draw(&mut self, ui: &mut Ui) -> Result<(), Box<dyn Error>> {
        self.draw_header(ui);
        self.draw_body(ui);
        ui.separator();
        self.draw_footer(ui);
        Ok(())
    }

    fn draw_header(&mut self, ui: &mut Ui) {
        ui.vertical_centered_justified(|ui| {
            // Current file name
            ui.label(
                RichText::new(
                    self.audio_player_state
                        .current_file_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_str()
                        .unwrap_or_default(),
                )
                .color(Color32::WHITE)
                .family(FontFamily::Monospace),
            );
            // Media controls
            self.draw_controls(ui);
            // Layers panel (compact)
            self.draw_layers_panel(ui);
            ui.separator();
        });
    }

    fn draw_layers_panel(&mut self, ui: &mut Ui) {
        // Check if any layers are active
        let has_active_layers = self.audio_player_state.layers.iter().any(|l| l.is_playing || !l.is_empty);

        if !has_active_layers {
            return;
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} Layers:", icons::ICON_LAYERS)).size(12.0).weak());

            let layers = self.audio_player_state.layers.clone();
            for (i, layer) in layers.iter().enumerate() {
                if layer.is_empty && !layer.is_playing {
                    continue;
                }

                let file_name = layer.current_file.as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "...".to_string());

                let status_icon = if layer.is_playing {
                    icons::ICON_PLAY_ARROW
                } else if layer.is_paused {
                    icons::ICON_PAUSE
                } else {
                    icons::ICON_STOP
                };

                let layer_text = format!("{} {}:{}", status_icon, i + 1, truncate_string(&file_name, 12));
                let color = if layer.is_playing {
                    Color32::LIGHT_GREEN
                } else {
                    Color32::GRAY
                };

                let layer_btn = Button::new(RichText::new(&layer_text).size(11.0).color(color)).frame(false);
                let layer_response = ui.add(layer_btn);

                if layer_response.clicked() {
                    self.stop_layer(i);
                }
                if layer_response.hovered() {
                    layer_response.on_hover_text(format!("Click to stop layer {}", i + 1));
                }
            }

            // Stop all layers button
            let stop_all_btn = Button::new(
                RichText::new(format!("{} Stop All", icons::ICON_STOP)).size(11.0).color(Color32::LIGHT_RED)
            ).frame(false);
            let stop_all_response = ui.add(stop_all_btn);
            if stop_all_response.clicked() {
                self.stop_all_layers();
            }
        });
    }

    fn draw_controls(&mut self, ui: &mut Ui) {
        ui.horizontal_top(|ui| {
            // ---------- Play Button ----------
            let play_button = Button::new(match self.audio_player_state.state {
                PlayerState::Playing => icons::ICON_PAUSE,
                PlayerState::Paused | PlayerState::Stopped => icons::ICON_PLAY_ARROW,
            })
            .corner_radius(15.0);

            let play_button_response = ui.add_sized([30.0, 30.0], play_button);
            if play_button_response.clicked() {
                self.play_toggle();
            }
            // --------------------------------

            // ---------- Stop Button ----------
            let stop_button = Button::new(icons::ICON_STOP).corner_radius(15.0);
            let stop_button_response = ui.add_sized([30.0, 30.0], stop_button);
            if stop_button_response.clicked() {
                self.stop();
            }
            // --------------------------------

            // ---------- Loop Button ----------
            let loop_button = Button::new(
                RichText::new(match self.audio_player_state.looped {
                    true => icons::ICON_REPEAT_ONE,
                    false => icons::ICON_REPEAT,
                })
                .size(18.0),
            )
            .frame(false);

            let loop_button_response = ui.add_sized([15.0, 30.0], loop_button);
            if loop_button_response.clicked() {
                self.toggle_loop();
            }
            // --------------------------------

            // ---------- Position Slider ----------
            let position_slider = Slider::new(
                &mut self.app_state.position_slider_value,
                0.0..=self.audio_player_state.duration,
            )
            .show_value(false)
            .step_by(0.01);

            let default_slider_width = ui.spacing().slider_width;
            // Account for: stop button, time label, volume icon, volume slider, gain icon, gain slider, gain label
            let position_slider_width = ui.available_width()
                - (30.0 * 7.0)  // 7 controls at 30px each (added stop button)
                - default_slider_width  // volume slider
                - default_slider_width  // gain slider
                - (ui.spacing().item_spacing.x * 11.0);
            ui.spacing_mut().slider_width = position_slider_width.max(50.0);
            let position_slider_response = ui.add_sized([30.0, 30.0], position_slider);
            if position_slider_response.drag_stopped() {
                self.app_state.position_dragged = true;
            }
            // --------------------------------

            // ---------- Time Label ----------
            let time_label = Label::new(
                RichText::new(format_time_pair(
                    self.audio_player_state.position,
                    self.audio_player_state.duration,
                ))
                .monospace(),
            );
            ui.add_sized([30.0, 30.0], time_label);
            // --------------------------------

            // ---------- Volume Icon ----------
            let volume_icon = if self.audio_player_state.volume > 0.7 {
                icons::ICON_VOLUME_UP
            } else if self.audio_player_state.volume == 0.0 {
                icons::ICON_VOLUME_OFF
            } else if self.audio_player_state.volume < 0.3 {
                icons::ICON_VOLUME_MUTE
            } else {
                icons::ICON_VOLUME_DOWN
            };
            let volume_icon = Label::new(RichText::new(volume_icon).size(18.0));
            ui.add_sized([30.0, 30.0], volume_icon);
            // --------------------------------

            // ---------- Volume Slider ----------
            let volume_slider = Slider::new(&mut self.app_state.volume_slider_value, 0.0..=1.0)
                .show_value(false)
                .step_by(0.01);

            ui.spacing_mut().slider_width = default_slider_width;
            ui.spacing_mut().item_spacing.x = 0.0;

            let volume_slider_response = ui.add_sized([30.0, 30.0], volume_slider);
            if volume_slider_response.drag_stopped() {
                self.app_state.volume_dragged = true;
            }
            // --------------------------------

            ui.spacing_mut().item_spacing.x = 4.0;

            // ---------- Gain Icon ----------
            let gain_icon = Label::new(RichText::new(icons::ICON_GRAPHIC_EQ).size(18.0));
            ui.add_sized([30.0, 30.0], gain_icon);
            // --------------------------------

            // ---------- Gain Slider ----------
            let gain_slider = Slider::new(&mut self.app_state.gain_slider_value, 0.5..=3.0)
                .show_value(false)
                .step_by(0.1);

            ui.spacing_mut().item_spacing.x = 0.0;

            let gain_slider_response = ui.add_sized([30.0, 30.0], gain_slider);
            if gain_slider_response.drag_stopped() {
                self.app_state.gain_dragged = true;
            }

            // Gain label
            let gain_label = Label::new(
                RichText::new(format!("{:.1}x", self.audio_player_state.gain))
                    .monospace()
                    .size(12.0),
            );
            ui.add_sized([30.0, 30.0], gain_label);
            // --------------------------------
        });
    }

    fn draw_body(&mut self, ui: &mut Ui) {
        let dirs_size = Vec2::new(ui.available_width() / 4.0, ui.available_height() - 40.0);

        ui.horizontal(|ui| {
            self.draw_dirs(ui, dirs_size);
            ui.separator();

            let files_size = Vec2::new(ui.available_width(), ui.available_height() - 40.0);
            self.draw_files(ui, files_size);
        });
    }

    fn draw_dirs(&mut self, ui: &mut Ui, area_size: Vec2) {
        ui.vertical(|ui| {
            ui.set_min_width(area_size.x);
            ui.set_min_height(area_size.y);

            ScrollArea::vertical().id_salt(0).show(ui, |ui| {
                ui.set_min_width(area_size.x);

                // --------- Directories Section ----------
                ui.label(RichText::new("Folders").weak().size(11.0));
                ui.add_space(4.0);

                let mut dirs: Vec<_> = self.app_state.dirs.iter().cloned().collect();
                dirs.sort();
                for path in &dirs {
                    ui.horizontal(|ui| {
                        let name = path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.to_string_lossy().to_string());

                        let mut dir_button_text = RichText::new(name);
                        if self.app_state.current_category.is_none() {
                            if let Some(current_dir) = &self.app_state.current_dir {
                                if current_dir == path {
                                    dir_button_text = dir_button_text.color(Color32::WHITE);
                                }
                            }
                        }

                        let dir_button =
                            Button::new(dir_button_text.atom_max_width(area_size.x - 36.0)).frame(false);

                        let dir_button_response = ui.add(dir_button);
                        if dir_button_response.clicked() {
                            self.app_state.current_category = None;
                            self.open_dir(path);
                        }

                        let delete_dir_button = Button::new(icons::ICON_DELETE).frame(false);
                        let delete_dir_button_response =
                            ui.add_sized([18.0, 18.0], delete_dir_button);
                        if delete_dir_button_response.clicked() {
                            self.remove_dir(path);
                        }
                    });
                }

                ui.horizontal(|ui| {
                    let add_dirs_button = Button::new(icons::ICON_ADD).frame(false);
                    let add_dirs_button_response = ui.add_sized([18.0, 18.0], add_dirs_button);
                    if add_dirs_button_response.clicked() {
                        self.add_dirs();
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // --------- Categories Section ----------
                ui.label(RichText::new("Categories").weak().size(11.0));
                ui.add_space(4.0);

                let mut categories: Vec<_> = self.config.categories.keys().cloned().collect();
                categories.sort();

                for cat_name in &categories {
                    let is_editing = self.app_state.editing_category.as_ref() == Some(cat_name);

                    if is_editing {
                        // Show text input for renaming
                        ui.horizontal(|ui| {
                            let response = ui.add(
                                TextEdit::singleline(&mut self.app_state.new_category_name)
                                    .desired_width(area_size.x - 54.0),
                            );

                            if response.lost_focus() {
                                let old_name = cat_name.clone();
                                let new_name = self.app_state.new_category_name.clone();
                                if !new_name.is_empty() && new_name != old_name {
                                    self.rename_category(&old_name, &new_name);
                                }
                                self.app_state.editing_category = None;
                                self.app_state.new_category_name.clear();
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            let is_selected = self.app_state.current_category.as_ref() == Some(cat_name);

                            let mut cat_button_text = RichText::new(format!("{} {}", icons::ICON_FOLDER_SPECIAL, cat_name));
                            if is_selected {
                                cat_button_text = cat_button_text.color(Color32::LIGHT_BLUE);
                            }

                            let cat_button =
                                Button::new(cat_button_text.atom_max_width(area_size.x - 54.0)).frame(false);

                            let cat_button_response = ui.add(cat_button);
                            if cat_button_response.clicked() {
                                self.open_category(cat_name);
                            }

                            // Edit button
                            let edit_button = Button::new(
                                RichText::new(icons::ICON_EDIT).size(12.0)
                            ).frame(false);
                            let edit_response = ui.add_sized([18.0, 18.0], edit_button);
                            if edit_response.clicked() {
                                self.app_state.editing_category = Some(cat_name.clone());
                                self.app_state.new_category_name = cat_name.clone();
                            }

                            // Delete button
                            let delete_cat_button = Button::new(icons::ICON_DELETE).frame(false);
                            let delete_cat_response = ui.add_sized([18.0, 18.0], delete_cat_button);
                            if delete_cat_response.clicked() {
                                self.delete_category(cat_name);
                            }
                        });
                    }
                }

                // Add category button or input
                if self.app_state.show_new_category_dialog {
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            TextEdit::singleline(&mut self.app_state.new_category_name)
                                .hint_text("Category name")
                                .desired_width(area_size.x - 54.0),
                        );

                        if response.lost_focus() {
                            if !self.app_state.new_category_name.is_empty() {
                                let name = self.app_state.new_category_name.clone();
                                self.create_category(&name);
                            }
                            self.app_state.show_new_category_dialog = false;
                            self.app_state.new_category_name.clear();
                        }

                        // Request focus on first frame
                        if !response.has_focus() {
                            response.request_focus();
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        let add_cat_button = Button::new(icons::ICON_ADD).frame(false);
                        let add_cat_response = ui.add_sized([18.0, 18.0], add_cat_button);
                        if add_cat_response.clicked() {
                            self.app_state.show_new_category_dialog = true;
                            self.app_state.new_category_name.clear();
                        }
                    });
                }

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    let play_file_button = Button::new("Play file");
                    let play_file_button_response = ui.add(play_file_button);
                    if play_file_button_response.clicked() {
                        self.open_file();
                    }
                });
            });
        });
    }

    fn draw_files(&mut self, ui: &mut Ui, area_size: Vec2) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let search_width = ui.available_width() - 120.0;
                let search_field = ui.add_sized(
                    [search_width.max(100.0), 22.0],
                    TextEdit::singleline(&mut self.app_state.search_query).hint_text("Search..."),
                );
                self.app_state.search_field_id = Some(search_field.id);

                // Tag filter dropdown
                let all_tags = self.get_all_tags();
                if !all_tags.is_empty() {
                    let filter_text = self.app_state.filter_by_tag.clone()
                        .map(|t| format!("#{}", t))
                        .unwrap_or_else(|| "Filter by tag".to_string());

                    ComboBox::from_id_salt("tag_filter")
                        .selected_text(&filter_text)
                        .width(100.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(self.app_state.filter_by_tag.is_none(), "All sounds").clicked() {
                                self.app_state.filter_by_tag = None;
                            }
                            ui.separator();
                            for tag in &all_tags {
                                let is_selected = self.app_state.filter_by_tag.as_ref() == Some(tag);
                                if ui.selectable_label(is_selected, format!("#{}", tag)).clicked() {
                                    self.app_state.filter_by_tag = Some(tag.clone());
                                }
                            }
                        });
                }
            });

            ui.separator();

            // Show category header if viewing a category
            if let Some(ref cat_name) = self.app_state.current_category.clone() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} {}", icons::ICON_FOLDER_SPECIAL, cat_name))
                            .color(Color32::LIGHT_BLUE)
                            .monospace(),
                    );
                });
                ui.add_space(4.0);
            }

            ScrollArea::vertical().id_salt(1).show(ui, |ui| {
                ui.set_min_width(area_size.x);
                ui.set_min_height(area_size.y);

                ui.vertical(|ui| {
                    // Check if viewing a category
                    if let Some(ref cat_name) = self.app_state.current_category.clone() {
                        // Draw category contents
                        if let Some(category) = self.config.categories.get(cat_name) {
                            let sounds = category.sounds.clone();
                            let search_query = self.app_state.search_query.to_lowercase();
                            let search_query = search_query.trim();

                            if sounds.is_empty() {
                                ui.label(RichText::new("No sounds in this category").weak());
                                ui.label(RichText::new("Add sounds using the + button on files").weak().size(11.0));
                            } else {
                                for entry_path in &sounds {
                                    let file_name = entry_path
                                        .file_name()
                                        .map(|n| n.to_string_lossy())
                                        .unwrap_or_default();

                                    if !search_query.is_empty()
                                        && !file_name.to_lowercase().contains(search_query)
                                    {
                                        continue;
                                    }

                                    self.draw_category_file_row(ui, entry_path, &cat_name);
                                }
                            }
                        }
                    } else {
                        // Draw regular directory contents
                        let mut files: Vec<_> = self.app_state.files.iter().cloned().collect();
                        files.sort();

                        let search_query = self.app_state.search_query.to_lowercase();
                        let search_query = search_query.trim();

                        // Collect favorites from current directory
                        let favorites: Vec<_> = self.config.favorites.iter().cloned().collect();
                        let current_dir_favorites: Vec<_> = favorites
                            .iter()
                            .filter(|f| files.contains(f))
                            .cloned()
                            .collect();

                        // Draw favorites section if there are any in current directory
                        if !current_dir_favorites.is_empty() && search_query.is_empty() {
                            ui.label(RichText::new("Favorites").color(Color32::GOLD).monospace());
                            ui.add_space(4.0);

                            let mut sorted_favorites = current_dir_favorites.clone();
                            sorted_favorites.sort();

                            for entry_path in &sorted_favorites {
                                self.draw_file_row(ui, entry_path, true);
                            }

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(4.0);
                        }

                        // Draw all files
                        let tag_filter = self.app_state.filter_by_tag.clone();

                        for entry_path in &files {
                            if entry_path.is_dir() {
                                continue;
                            }

                            let extension = entry_path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or_default();

                            if !SUPPORTED_EXTENSIONS.contains(&extension) {
                                continue;
                            }

                            let file_name = entry_path
                                .file_name()
                                .map(|n| n.to_string_lossy())
                                .unwrap_or_default();

                            if !file_name.to_lowercase().contains(search_query) {
                                continue;
                            }

                            // Apply tag filter
                            if let Some(ref filter_tag) = tag_filter {
                                let has_tag = self.config.sound_metadata
                                    .get(entry_path)
                                    .map(|m| m.has_tag(filter_tag))
                                    .unwrap_or(false);
                                if !has_tag {
                                    continue;
                                }
                            }

                            self.draw_file_row(ui, entry_path, false);
                        }
                    }
                });
            });

            // Draw metadata editing popup
            self.draw_metadata_popup(ui);
        });
    }

    fn draw_metadata_popup(&mut self, ui: &mut Ui) {
        let editing_file = self.app_state.editing_metadata_file.clone();

        if let Some(file_path) = editing_file {
            let file_name = file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let metadata = self.config.sound_metadata.get(&file_path).cloned();
            let mut custom_name = metadata.as_ref()
                .and_then(|m| m.custom_name.clone())
                .unwrap_or_default();
            let tags: Vec<String> = metadata.as_ref()
                .map(|m| {
                    let mut t: Vec<_> = m.tags.iter().cloned().collect();
                    t.sort();
                    t
                })
                .unwrap_or_default();

            egui::Window::new("Edit Sound Metadata")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.set_min_width(300.0);

                    ui.label(RichText::new(&file_name).weak().size(11.0));
                    ui.add_space(8.0);

                    // Custom name field
                    ui.horizontal(|ui| {
                        ui.label("Display name:");
                        let response = ui.add(
                            TextEdit::singleline(&mut custom_name)
                                .hint_text(&file_name)
                                .desired_width(180.0)
                        );
                        if response.changed() {
                            let name = if custom_name.is_empty() { None } else { Some(custom_name.clone()) };
                            self.set_sound_custom_name(&file_path, name);
                        }
                    });

                    ui.add_space(8.0);

                    // Tags section
                    ui.label("Tags:");
                    ui.horizontal_wrapped(|ui| {
                        for tag in &tags {
                            let tag_text = format!("#{} {}", tag, icons::ICON_CLOSE);
                            if ui.button(RichText::new(&tag_text).size(11.0)).clicked() {
                                self.remove_sound_tag(&file_path, tag);
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        let response = ui.add(
                            TextEdit::singleline(&mut self.app_state.tag_input)
                                .hint_text("Add tag...")
                                .desired_width(150.0)
                        );
                        if (response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                            || ui.button(icons::ICON_ADD).clicked()
                        {
                            if !self.app_state.tag_input.is_empty() {
                                let tag = self.app_state.tag_input.clone();
                                self.add_sound_tag(&file_path, &tag);
                                self.app_state.tag_input.clear();
                            }
                        }
                    });

                    ui.add_space(8.0);

                    // Close button
                    ui.horizontal(|ui| {
                        if ui.button("Done").clicked() {
                            self.app_state.editing_metadata_file = None;
                            self.app_state.tag_input.clear();
                        }
                    });
                });
        }
    }

    fn draw_file_row(&mut self, ui: &mut Ui, entry_path: &std::path::PathBuf, in_favorites_section: bool) {
        let original_file_name = entry_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let is_favorite = self.is_favorite(entry_path);
        let categories: Vec<String> = self.config.categories.keys().cloned().collect();

        // Get metadata for this file
        let metadata = self.config.sound_metadata.get(entry_path).cloned();
        let display_name = metadata.as_ref()
            .and_then(|m| m.custom_name.clone())
            .unwrap_or_else(|| original_file_name.clone());
        let tags: Vec<String> = metadata.as_ref()
            .map(|m| m.tags.iter().cloned().collect())
            .unwrap_or_default();

        ui.horizontal(|ui| {
            // Favorite toggle button (star icon)
            let star_icon = if is_favorite {
                RichText::new(icons::ICON_STAR).size(14.0).color(Color32::GOLD)
            } else {
                RichText::new(icons::ICON_STAR_BORDER).size(14.0)
            };
            let star_button = Button::new(star_icon).frame(false);
            let star_response = ui.add_sized([18.0, 18.0], star_button);
            if star_response.clicked() {
                self.toggle_favorite(&entry_path.clone());
            }
            let hover_text = if is_favorite { "Remove from favorites" } else { "Add to favorites" };
            if star_response.hovered() {
                star_response.on_hover_text(hover_text);
            }

            // Preview button (speakers only) - hide in favorites section to avoid duplication
            if !in_favorites_section {
                let preview_button =
                    Button::new(RichText::new(icons::ICON_VOLUME_UP).size(14.0))
                        .frame(false);
                let preview_response = ui.add_sized([18.0, 18.0], preview_button);
                if preview_response.clicked() {
                    self.preview_file(entry_path);
                }
                if preview_response.hovered() {
                    preview_response.on_hover_text("Preview (speakers only)");
                }
            }

            // Add to category button (only show if there are categories)
            if !categories.is_empty() && !in_favorites_section {
                let add_to_cat_button =
                    Button::new(RichText::new(icons::ICON_PLAYLIST_ADD).size(14.0))
                        .frame(false);
                let add_to_cat_response = ui.add_sized([18.0, 18.0], add_to_cat_button);

                add_to_cat_response.context_menu(|ui| {
                    ui.label(RichText::new("Add to category:").weak().size(11.0));
                    ui.separator();
                    for cat_name in &categories {
                        let in_category = self.config.categories
                            .get(cat_name)
                            .map(|c| c.contains(entry_path))
                            .unwrap_or(false);

                        let label = if in_category {
                            format!("{} {} (added)", icons::ICON_CHECK, cat_name)
                        } else {
                            cat_name.clone()
                        };

                        if ui.button(&label).clicked() {
                            if in_category {
                                self.remove_from_category(cat_name, entry_path);
                            } else {
                                self.add_to_category(cat_name, entry_path);
                            }
                            ui.close();
                        }
                    }
                });

                if add_to_cat_response.hovered() {
                    add_to_cat_response.on_hover_text("Add to category (right-click)");
                }
            }

            // Edit metadata button
            if !in_favorites_section {
                let edit_button =
                    Button::new(RichText::new(icons::ICON_LABEL).size(14.0))
                        .frame(false);
                let edit_response = ui.add_sized([18.0, 18.0], edit_button);
                if edit_response.clicked() {
                    self.app_state.editing_metadata_file = Some(entry_path.clone());
                    self.app_state.tag_input.clear();
                }
                if edit_response.hovered() {
                    edit_response.on_hover_text("Edit tags & metadata");
                }
            }

            // Play on layer button
            if !in_favorites_section {
                let layer_button =
                    Button::new(RichText::new(icons::ICON_LAYERS).size(14.0))
                        .frame(false);
                let layer_response = ui.add_sized([18.0, 18.0], layer_button);

                layer_response.context_menu(|ui| {
                    ui.label(RichText::new("Play on layer:").weak().size(11.0));
                    ui.separator();
                    for i in 0..4 {
                        if ui.button(format!("Layer {}", i + 1)).clicked() {
                            self.play_on_layer(i, entry_path);
                            ui.close();
                        }
                    }
                });

                if layer_response.hovered() {
                    layer_response.on_hover_text("Play on layer (right-click)");
                }
            }

            // File name button (play through virtual mic)
            let mut file_button_text = RichText::new(&display_name);
            if let Some(current_file) = &self.app_state.selected_file {
                if current_file == entry_path {
                    file_button_text = file_button_text.color(Color32::WHITE);
                }
            }

            let file_button = Button::new(file_button_text).frame(false);
            let file_button_response = ui.add(file_button);
            if file_button_response.clicked() {
                self.play_file(entry_path);
                self.app_state.selected_file = Some(entry_path.clone());
            }

            // Show tags inline
            if !tags.is_empty() {
                for tag in tags.iter().take(3) {
                    ui.label(
                        RichText::new(format!("#{}", tag))
                            .size(10.0)
                            .color(Color32::LIGHT_BLUE)
                    );
                }
                if tags.len() > 3 {
                    ui.label(
                        RichText::new(format!("+{}", tags.len() - 3))
                            .size(10.0)
                            .weak()
                    );
                }
            }
        });
    }

    fn draw_category_file_row(&mut self, ui: &mut Ui, entry_path: &std::path::PathBuf, category_name: &str) {
        let file_name = entry_path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();

        let is_favorite = self.is_favorite(entry_path);

        ui.horizontal(|ui| {
            // Favorite toggle button (star icon)
            let star_icon = if is_favorite {
                RichText::new(icons::ICON_STAR).size(14.0).color(Color32::GOLD)
            } else {
                RichText::new(icons::ICON_STAR_BORDER).size(14.0)
            };
            let star_button = Button::new(star_icon).frame(false);
            let star_response = ui.add_sized([18.0, 18.0], star_button);
            if star_response.clicked() {
                self.toggle_favorite(&entry_path.clone());
            }
            let hover_text = if is_favorite { "Remove from favorites" } else { "Add to favorites" };
            if star_response.hovered() {
                star_response.on_hover_text(hover_text);
            }

            // Preview button (speakers only)
            let preview_button =
                Button::new(RichText::new(icons::ICON_VOLUME_UP).size(14.0))
                    .frame(false);
            let preview_response = ui.add_sized([18.0, 18.0], preview_button);
            if preview_response.clicked() {
                self.preview_file(entry_path);
            }
            if preview_response.hovered() {
                preview_response.on_hover_text("Preview (speakers only)");
            }

            // Remove from category button
            let remove_button =
                Button::new(RichText::new(icons::ICON_REMOVE_CIRCLE_OUTLINE).size(14.0).color(Color32::LIGHT_RED))
                    .frame(false);
            let remove_response = ui.add_sized([18.0, 18.0], remove_button);
            if remove_response.clicked() {
                self.remove_from_category(category_name, entry_path);
            }
            if remove_response.hovered() {
                remove_response.on_hover_text("Remove from category");
            }

            // Play on layer button
            let layer_button =
                Button::new(RichText::new(icons::ICON_LAYERS).size(14.0))
                    .frame(false);
            let layer_response = ui.add_sized([18.0, 18.0], layer_button);

            layer_response.context_menu(|ui| {
                ui.label(RichText::new("Play on layer:").weak().size(11.0));
                ui.separator();
                for i in 0..4 {
                    if ui.button(format!("Layer {}", i + 1)).clicked() {
                        self.play_on_layer(i, entry_path);
                        ui.close();
                    }
                }
            });

            if layer_response.hovered() {
                layer_response.on_hover_text("Play on layer (right-click)");
            }

            // File name button (play through virtual mic)
            let mut file_button_text = RichText::new(file_name.to_string());
            if let Some(current_file) = &self.app_state.selected_file {
                if current_file == entry_path {
                    file_button_text = file_button_text.color(Color32::WHITE);
                }
            }

            // Show warning if file doesn't exist
            if !entry_path.exists() {
                file_button_text = file_button_text.color(Color32::DARK_RED).strikethrough();
            }

            let file_button = Button::new(file_button_text).frame(false);
            let file_button_response = ui.add(file_button);
            if file_button_response.clicked() && entry_path.exists() {
                self.play_file(entry_path);
                self.app_state.selected_file = Some(entry_path.clone());
            }
        });
    }

    fn draw_footer(&mut self, ui: &mut Ui) {
        ui.add_space(5.0);
        ui.horizontal_top(|ui| {
            // ---------- Microphone selection ----------
            let mut mics: Vec<(&String, &String)> =
                self.audio_player_state.all_inputs.iter().collect();
            mics.sort_by_key(|(k, _)| *k);

            let mut selected_input = self.audio_player_state.current_input.to_owned();
            let prev_input = selected_input.to_owned();
            ComboBox::from_label("Choose microphone")
                .selected_text(
                    self.audio_player_state
                        .all_inputs
                        .get(&selected_input)
                        .unwrap_or(&String::new()),
                )
                .show_ui(ui, |ui| {
                    for (name, nick) in mics {
                        ui.selectable_value(&mut selected_input, name.to_owned(), nick);
                    }
                });

            if selected_input != prev_input {
                self.set_input(selected_input);
            }
            // --------------------------------

            ui.add_space(10.0);

            // ---------- Mic Gain ----------
            ui.label(RichText::new("Mic:").monospace().size(12.0));
            let mic_gain_slider = Slider::new(&mut self.app_state.mic_gain_slider_value, 0.5..=3.0)
                .show_value(false)
                .step_by(0.1);
            let mic_gain_slider_response = ui.add_sized([60.0, 18.0], mic_gain_slider);
            if mic_gain_slider_response.drag_stopped() {
                self.app_state.mic_gain_dragged = true;
            }
            ui.label(
                RichText::new(format!("{:.1}x", self.audio_player_state.mic_gain))
                    .monospace()
                    .size(12.0),
            );
            // --------------------------------

            ui.add_space(ui.available_width() - 18.0 - ui.spacing().item_spacing.x);

            // ---------- Settings button ----------
            let settings_button = Button::new(icons::ICON_SETTINGS).frame(false);
            let settings_button_response = ui.add_sized([18.0, 18.0], settings_button);
            if settings_button_response.clicked() {
                self.app_state.show_settings = true;
            }
            // --------------------------------
        });
    }
}

/// Truncate a string to a maximum length
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Format a HotkeyBinding for display in the UI
fn format_hotkey_display(binding: &HotkeyBinding) -> String {
    let mut parts = Vec::new();
    if binding.ctrl {
        parts.push("Ctrl".to_string());
    }
    if binding.shift {
        parts.push("Shift".to_string());
    }
    if binding.alt {
        parts.push("Alt".to_string());
    }
    if binding.super_key {
        parts.push("Super".to_string());
    }
    parts.push(key_display_name(&binding.key));
    parts.join(" + ")
}

/// Convert egui Key to key code string
fn egui_key_to_string(key: Key) -> Option<String> {
    match key {
        // Letters
        Key::A => Some("KeyA".to_string()),
        Key::B => Some("KeyB".to_string()),
        Key::C => Some("KeyC".to_string()),
        Key::D => Some("KeyD".to_string()),
        Key::E => Some("KeyE".to_string()),
        Key::F => Some("KeyF".to_string()),
        Key::G => Some("KeyG".to_string()),
        Key::H => Some("KeyH".to_string()),
        Key::I => Some("KeyI".to_string()),
        Key::J => Some("KeyJ".to_string()),
        Key::K => Some("KeyK".to_string()),
        Key::L => Some("KeyL".to_string()),
        Key::M => Some("KeyM".to_string()),
        Key::N => Some("KeyN".to_string()),
        Key::O => Some("KeyO".to_string()),
        Key::P => Some("KeyP".to_string()),
        Key::Q => Some("KeyQ".to_string()),
        Key::R => Some("KeyR".to_string()),
        Key::S => Some("KeyS".to_string()),
        Key::T => Some("KeyT".to_string()),
        Key::U => Some("KeyU".to_string()),
        Key::V => Some("KeyV".to_string()),
        Key::W => Some("KeyW".to_string()),
        Key::X => Some("KeyX".to_string()),
        Key::Y => Some("KeyY".to_string()),
        Key::Z => Some("KeyZ".to_string()),

        // Numbers
        Key::Num0 => Some("Digit0".to_string()),
        Key::Num1 => Some("Digit1".to_string()),
        Key::Num2 => Some("Digit2".to_string()),
        Key::Num3 => Some("Digit3".to_string()),
        Key::Num4 => Some("Digit4".to_string()),
        Key::Num5 => Some("Digit5".to_string()),
        Key::Num6 => Some("Digit6".to_string()),
        Key::Num7 => Some("Digit7".to_string()),
        Key::Num8 => Some("Digit8".to_string()),
        Key::Num9 => Some("Digit9".to_string()),

        // Function keys
        Key::F1 => Some("F1".to_string()),
        Key::F2 => Some("F2".to_string()),
        Key::F3 => Some("F3".to_string()),
        Key::F4 => Some("F4".to_string()),
        Key::F5 => Some("F5".to_string()),
        Key::F6 => Some("F6".to_string()),
        Key::F7 => Some("F7".to_string()),
        Key::F8 => Some("F8".to_string()),
        Key::F9 => Some("F9".to_string()),
        Key::F10 => Some("F10".to_string()),
        Key::F11 => Some("F11".to_string()),
        Key::F12 => Some("F12".to_string()),

        // Special keys
        Key::Space => Some("Space".to_string()),
        Key::Enter => Some("Enter".to_string()),
        Key::Escape => Some("Escape".to_string()),
        Key::Backspace => Some("Backspace".to_string()),
        Key::Tab => Some("Tab".to_string()),
        Key::Delete => Some("Delete".to_string()),
        Key::Insert => Some("Insert".to_string()),
        Key::Home => Some("Home".to_string()),
        Key::End => Some("End".to_string()),
        Key::PageUp => Some("PageUp".to_string()),
        Key::PageDown => Some("PageDown".to_string()),

        // Arrow keys
        Key::ArrowUp => Some("ArrowUp".to_string()),
        Key::ArrowDown => Some("ArrowDown".to_string()),
        Key::ArrowLeft => Some("ArrowLeft".to_string()),
        Key::ArrowRight => Some("ArrowRight".to_string()),

        // Punctuation
        Key::Minus => Some("Minus".to_string()),
        Key::Equals => Some("Equal".to_string()),
        Key::OpenBracket => Some("BracketLeft".to_string()),
        Key::CloseBracket => Some("BracketRight".to_string()),
        Key::Backslash => Some("Backslash".to_string()),
        Key::Semicolon => Some("Semicolon".to_string()),
        Key::Quote => Some("Quote".to_string()),
        Key::Comma => Some("Comma".to_string()),
        Key::Period => Some("Period".to_string()),
        Key::Slash => Some("Slash".to_string()),
        Key::Backtick => Some("Backquote".to_string()),

        _ => None,
    }
}

/// Create a HotkeyBinding from egui key and modifiers
pub fn create_hotkey_binding(key: Key, modifiers: Modifiers) -> Option<HotkeyBinding> {
    let key_str = egui_key_to_string(key)?;
    Some(HotkeyBinding::new(
        &key_str,
        modifiers.ctrl,
        modifiers.shift,
        modifiers.alt,
        modifiers.command, // Super/Meta key
    ))
}
