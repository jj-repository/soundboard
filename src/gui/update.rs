use crate::gui::{MutexExt, SoundpadGui};
use eframe::{App, Frame as EFrame};
use egui::{CentralPanel, Context};
use pwsp::{
    types::socket::Request,
    utils::{
        daemon::{get_daemon_config, is_daemon_running},
        gui::make_request_sync,
    },
};

impl App for SoundpadGui {
    fn update(&mut self, ctx: &Context, _frame: &mut EFrame) {
        // Poll for tray menu actions
        self.poll_tray_messages(ctx);
        // Poll for global hotkey actions
        self.poll_hotkey_messages();
        // Poll for update check results
        self.poll_update_status();

        {
            let guard = self.audio_player_state_shared.lock_or_recover();
            self.audio_player_state = guard.clone();
        }

        let old_scale_factor = self.config.scale_factor;
        let new_scale_factor = ctx.zoom_factor().clamp(0.5, 2.0);

        ctx.set_zoom_factor(new_scale_factor);
        self.config.scale_factor = new_scale_factor;

        if new_scale_factor != old_scale_factor && self.config.save_scale_factor {
            self.config.save_to_file().ok();
        }

        self.handle_input(ctx);

        CentralPanel::default().show(ctx, |ui| {
            if !is_daemon_running().unwrap() {
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

            self.draw(ui).ok();

            if let Some(force_focus_id) = self.app_state.force_focus_id {
                ui.memory_mut(|reder| {
                    reder.request_focus(force_focus_id);
                });
                self.app_state.force_focus_id = None;
            }
        });

        if self.app_state.position_dragged {
            make_request_sync(Request::seek(self.app_state.position_slider_value)).ok();
            let mut guard = self.audio_player_state_shared.lock_or_recover();
            guard.new_position = Some(self.app_state.position_slider_value);
            guard.position = self.app_state.position_slider_value;
            self.app_state.position_dragged = false;
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

            if self.config.save_volume {
                let mut daemon_config = get_daemon_config();
                daemon_config.default_volume = Some(new_volume);
                daemon_config.save_to_file().ok();
            }
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

            if self.config.save_gain {
                let mut daemon_config = get_daemon_config();
                daemon_config.default_gain = Some(new_gain);
                daemon_config.save_to_file().ok();
            }
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

            if self.config.save_mic_gain {
                let mut daemon_config = get_daemon_config();
                daemon_config.default_mic_gain = Some(new_mic_gain);
                daemon_config.save_to_file().ok();
            }
        } else {
            self.app_state.mic_gain_slider_value = self.audio_player_state.mic_gain;
        }

        ctx.request_repaint_after_secs(1.0 / 60.0);
    }
}
