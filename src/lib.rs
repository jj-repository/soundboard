pub mod types;
pub mod utils;

use std::sync::{Mutex, MutexGuard};

/// Extension trait for Mutex that handles poisoning gracefully
pub trait MutexExt<T> {
    /// Lock the mutex, recovering from poison if necessary.
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            eprintln!("Warning: Mutex was poisoned, recovering...");
            poisoned.into_inner()
        })
    }
}
