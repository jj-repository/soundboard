use crate::gui::{MutexExt, SoundpadGui};
use eframe::{App, Frame as EFrame};
use egui::Ui;
use soundboard::{
    types::socket::Request,
    utils::{
        daemon::{get_daemon_config, is_daemon_running},
        gui::make_request_sync,
    },
};

impl App for SoundpadGui {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut EFrame) {
        // Check for updates on startup if enabled
        if !self.startup_update_checked && self.config.auto_check_updates {
            self.startup_update_checked = true;
            self.check_for_updates();
        }

        // Poll for tray menu actions
        self.poll_tray_messages(ctx);
        // Poll for global hotkey actions
        self.poll_hotkey_messages();
        // Poll for update check results
        self.poll_update_status();
        // VB-Cable first-run detection + install progress channel
        self.logic_vbcable_wizard();
        // Drain yt-dlp grab status channel
        self.poll_url_grab_status();

        let old_scale_factor = self.config.scale_factor;
        let new_scale_factor = ctx.zoom_factor().clamp(0.5, 2.0);

        ctx.set_zoom_factor(new_scale_factor);
        self.config.scale_factor = new_scale_factor;

        if new_scale_factor != old_scale_factor && self.config.save_scale_factor {
            if let Err(e) = self.config.save_to_file() {
                tracing::error!("Failed to persist scale factor: {}", e);
            }
        }

        self.handle_input(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut EFrame) {
        // Take a fresh snapshot of daemon state at the start of each render
        // frame so draw code sees the most recent values. The snapshot is
        // taken here (not in logic()) to minimise the window between snapshot
        // and use — the poll thread cannot run between these two lines.
        {
            let guard = self.audio_player_state_shared.lock_or_recover();
            self.audio_player_state = guard.clone();
        }

        // VB-Cable wizard renders as a centred Window over whatever else is
        // visible; no early return so the rest of the UI stays interactive
        // when the wizard is hidden.
        self.draw_vbcable_wizard(ui.ctx());

        if !is_daemon_running().unwrap_or(false) {
            self.draw_waiting_for_daemon(ui);
            return;
        }

        // Show sounds folder setup wizard if needed
        if self.app_state.show_sounds_folder_setup {
            self.draw_sounds_folder_setup(ui);
            return;
        }

        if self.app_state.show_settings {
            self.draw_settings(ui);
            return;
        }

        self.draw(ui);

        if let Some(force_focus_id) = self.app_state.force_focus_id {
            ui.memory_mut(|mem| {
                mem.request_focus(force_focus_id);
            });
            self.app_state.force_focus_id = None;
        }

        if self.app_state.position_dragged {
            make_request_sync(Request::seek(self.app_state.position_slider_value)).ok();
            let mut guard = self.audio_player_state_shared.lock_or_recover();
            guard.new_position = Some(self.app_state.position_slider_value);
            guard.position = self.app_state.position_slider_value;
            self.app_state.position_dragged = false;
            self.app_state.position_drag_cooldown = 10;
        } else if self.app_state.position_drag_cooldown > 0 {
            self.app_state.position_drag_cooldown -= 1;
        } else {
            self.app_state.position_slider_value = self.audio_player_state.position;
        }

        if self.app_state.volume_dragged {
            let new_volume = self.app_state.volume_slider_value;
            make_request_sync(Request::set_volume(new_volume)).ok();
            let mut guard = self.audio_player_state_shared.lock_or_recover();
            guard.new_volume = Some(self.app_state.volume_slider_value);
            guard.volume = self.app_state.volume_slider_value;
            self.app_state.volume_dragged = false;
            self.app_state.volume_drag_cooldown = 10;
            if self.config.save_volume {
                let mut daemon_config = get_daemon_config();
                daemon_config.default_volume = Some(new_volume);
                if let Err(e) = daemon_config.save_to_file() {
                    tracing::error!("Failed to persist volume: {}", e);
                }
            }
        } else if self.app_state.volume_drag_cooldown > 0 {
            self.app_state.volume_drag_cooldown -= 1;
        } else {
            self.app_state.volume_slider_value = self.audio_player_state.volume;
        }

        if self.app_state.gain_dragged {
            let new_gain = self.app_state.gain_slider_value;
            make_request_sync(Request::set_gain(new_gain)).ok();
            let mut guard = self.audio_player_state_shared.lock_or_recover();
            guard.new_gain = Some(self.app_state.gain_slider_value);
            guard.gain = self.app_state.gain_slider_value;
            self.app_state.gain_dragged = false;
            self.app_state.gain_drag_cooldown = 10;
            if self.config.save_gain {
                let mut daemon_config = get_daemon_config();
                daemon_config.default_gain = Some(new_gain);
                if let Err(e) = daemon_config.save_to_file() {
                    tracing::error!("Failed to persist gain: {}", e);
                }
            }
        } else if self.app_state.gain_drag_cooldown > 0 {
            self.app_state.gain_drag_cooldown -= 1;
        } else {
            self.app_state.gain_slider_value = self.audio_player_state.gain;
        }

        if self.app_state.mic_gain_dragged {
            let new_mic_gain = self.app_state.mic_gain_slider_value;
            make_request_sync(Request::set_mic_gain(new_mic_gain)).ok();
            let mut guard = self.audio_player_state_shared.lock_or_recover();
            guard.new_mic_gain = Some(self.app_state.mic_gain_slider_value);
            guard.mic_gain = self.app_state.mic_gain_slider_value;
            self.app_state.mic_gain_dragged = false;
            self.app_state.mic_gain_drag_cooldown = 10;
            if self.config.save_mic_gain {
                let mut daemon_config = get_daemon_config();
                daemon_config.default_mic_gain = Some(new_mic_gain);
                if let Err(e) = daemon_config.save_to_file() {
                    tracing::error!("Failed to persist mic gain: {}", e);
                }
            }
        } else if self.app_state.mic_gain_drag_cooldown > 0 {
            self.app_state.mic_gain_drag_cooldown -= 1;
        } else {
            self.app_state.mic_gain_slider_value = self.audio_player_state.mic_gain;
        }

        ui.ctx().request_repaint_after_secs(1.0 / 60.0);
    }
}
