//! First-run wizard prompting the user to install VB-Audio Virtual Cable.
//!
//! Logic side (`logic_vbcable_wizard`) runs every frame, ticks the channel,
//! and triggers the prompt the first time we discover VB-Cable is missing.
//! UI side (`draw_vbcable_wizard`) renders a centred, modal-style window.

use crate::gui::SoundpadGui;
use egui::{Align2, Context};
use soundboard::utils::vbcable_install::{self, InstallStatus};
use std::sync::mpsc;
use std::thread;

impl SoundpadGui {
    /// Per-frame: kick off the one-shot detection and drain the install channel.
    pub fn logic_vbcable_wizard(&mut self) {
        if !self.vbcable_checked {
            self.vbcable_checked = true;
            if !self.config.vbcable_install_skipped && !vbcable_install::is_installed() {
                self.vbcable_wizard_visible = true;
            }
        }

        if let Some(rx) = &self.vbcable_install_rx {
            while let Ok(status) = rx.try_recv() {
                self.vbcable_wizard_status = Some(status);
            }
        }

        // Once the device shows up the user clearly does not need the wizard.
        // Drop the receiver so the worker thread can exit cleanly on next send.
        if matches!(self.vbcable_wizard_status, Some(InstallStatus::Done)) {
            self.vbcable_install_rx = None;
        }
    }

    /// Spawn the install worker. Lifts `tokio::Runtime` per call (cheap enough
    /// for a one-shot install) so we don't have to plumb the GUI's runtime in.
    fn start_vbcable_install(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.vbcable_install_rx = Some(rx);
        self.vbcable_wizard_status = Some(InstallStatus::Downloading {
            downloaded: 0,
            total: 0,
        });

        thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(InstallStatus::Failed(format!("runtime: {e}")));
                    return;
                }
            };
            if let Err(e) = rt.block_on(vbcable_install::run_install(tx.clone())) {
                let _ = tx.send(InstallStatus::Failed(e.to_string()));
            }
        });
    }

    fn dismiss_wizard(&mut self, persist_skip: bool) {
        self.vbcable_wizard_visible = false;
        self.vbcable_wizard_status = None;
        self.vbcable_install_rx = None;
        if persist_skip {
            self.config.vbcable_install_skipped = true;
            if let Err(e) = self.config.save_to_file() {
                tracing::error!("Failed to save vbcable_install_skipped: {e}");
            }
        }
    }

    pub fn draw_vbcable_wizard(&mut self, ctx: &Context) {
        if !self.vbcable_wizard_visible {
            return;
        }

        let mut close_skip = false;
        let mut close_done = false;
        let mut start_install = false;
        let mut open_url = false;

        egui::Window::new("VB-Audio Virtual Cable required")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.spacing_mut().item_spacing.y = 8.0;

                match &self.vbcable_wizard_status {
                    None => {
                        ui.label(
                            "Soundboard plays audio through VB-Audio Virtual Cable so other apps \
                             (Discord, Zoom, etc.) hear it as your microphone.",
                        );
                        ui.label("VB-Cable is not detected on this system.");
                        ui.add_space(4.0);
                        ui.label(
                            "Click Install to download and run the official installer from \
                             vb-audio.com. This requires a UAC prompt and may need a reboot.",
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Install").clicked() {
                                start_install = true;
                            }
                            if ui.button("Open download page").clicked() {
                                open_url = true;
                            }
                            if ui.button("Skip (don't ask again)").clicked() {
                                close_skip = true;
                            }
                        });
                    }
                    Some(InstallStatus::Downloading { downloaded, total }) => {
                        ui.label("Downloading VB-Cable installer…");
                        if *total > 0 {
                            let pct = (*downloaded as f32 / *total as f32).clamp(0.0, 1.0);
                            ui.add(egui::ProgressBar::new(pct).show_percentage());
                        } else {
                            ui.spinner();
                        }
                    }
                    Some(InstallStatus::Extracting) => {
                        ui.label("Extracting installer…");
                        ui.spinner();
                    }
                    Some(InstallStatus::Verifying) => {
                        ui.label("Verifying VB-Audio signature…");
                        ui.spinner();
                    }
                    Some(InstallStatus::Launching) => {
                        ui.label("Running installer (accept the UAC prompt)…");
                        ui.spinner();
                    }
                    Some(InstallStatus::WaitingForDevice) => {
                        ui.label("Waiting for the virtual audio device to appear…");
                        ui.spinner();
                    }
                    Some(InstallStatus::Done) => {
                        ui.label("VB-Cable is installed and the device is available.");
                        ui.add_space(4.0);
                        if ui.button("Close").clicked() {
                            close_done = true;
                        }
                    }
                    Some(InstallStatus::NeedsReboot) => {
                        ui.label(
                            "Installer finished but the virtual device hasn't shown up yet. \
                             A reboot is usually required for kernel audio drivers.",
                        );
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("Close").clicked() {
                                close_done = true;
                            }
                        });
                    }
                    Some(InstallStatus::Failed(msg)) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, "Install failed.");
                        ui.label(msg);
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("Open download page").clicked() {
                                open_url = true;
                            }
                            if ui.button("Close").clicked() {
                                close_skip = true;
                            }
                        });
                    }
                }
            });

        if open_url {
            let _ = open::that("https://vb-audio.com/Cable/");
        }
        if start_install {
            self.start_vbcable_install();
        }
        if close_skip {
            self.dismiss_wizard(true);
        }
        if close_done {
            self.dismiss_wizard(false);
        }
    }
}
