pub mod types;
pub mod utils;

/// PipeWire node name for the virtual microphone
pub const VIRTUAL_MIC_NAME: &str = "pwsp-virtual-mic";
/// PipeWire node name for the daemon's audio output
pub const DAEMON_OUTPUT_NAME: &str = "alsa_playback.pwsp-daemon";

use std::sync::{Mutex, MutexGuard};

/// Extension trait for Mutex that handles poisoning gracefully
pub trait MutexExt<T> {
    /// Lock the mutex, recovering from poison if necessary.
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            tracing::error!("Warning: Mutex was poisoned, recovering...");
            poisoned.into_inner()
        })
    }
}
