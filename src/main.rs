// Suppress the console window on release Windows builds so launching the
// GUI from Explorer or a shortcut doesn't flash a black box.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod gui;

use soundboard::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    soundboard::utils::logging::init();

    let argv: Vec<String> = std::env::args().collect();

    match argv.get(1).map(String::as_str) {
        Some("cli") => soundboard::utils::cli_run::run(argv).await,
        Some("--headless") => soundboard::utils::daemon_run::run_headless().await,
        _ => gui::run().await,
    }
}
