pub mod audio_player;
pub mod commands;
pub mod config;
pub mod gui;
#[cfg(target_os = "linux")]
pub mod pipewire;
pub mod socket;
