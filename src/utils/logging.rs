use tracing_subscriber::EnvFilter;

/// Install a process-wide tracing subscriber.
///
/// Reads `PWSP_LOG` (falling back to `RUST_LOG`), defaulting to `info`.
/// Safe to call from a binary's `main` before any logging happens. Subsequent
/// calls are no-ops thanks to `try_init`.
pub fn init() {
    let filter = EnvFilter::try_from_env("PWSP_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
