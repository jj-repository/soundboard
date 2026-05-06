//! CLI subcommand runtime. Was a separate `soundboard-cli` binary; now a
//! `cli ...` subcommand of the single `soundboard` exe.

use clap::{Parser, Subcommand};

use crate::{
    error::Result,
    types::socket::Request,
    utils::daemon::{make_request, wait_for_daemon},
};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "soundboard cli", version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Perform an action (ping, pause, resume, toggle-pause, stop, play)
    Action {
        #[clap(subcommand)]
        action: Actions,
    },
    /// Get information from the player
    Get {
        #[clap(subcommand)]
        parameter: GetCommands,
    },
    /// Set information in the player
    Set {
        #[clap(subcommand)]
        parameter: SetCommands,
    },
}

#[derive(Subcommand, Debug)]
enum Actions {
    /// Ping the daemon
    Ping,
    /// Pause audio playback
    Pause,
    /// Resume audio playback
    Resume,
    /// Toggle pause
    TogglePause,
    /// Stop audio playback and clear the queue
    Stop,
    /// Play a file (through virtual mic)
    Play { file_path: PathBuf },
    /// Preview a file (speakers only, not through virtual mic)
    Preview { file_path: PathBuf },
    /// Toggle loop
    ToggleLoop,
}

#[derive(Subcommand, Debug)]
enum GetCommands {
    IsPaused,
    Volume,
    Gain,
    MicGain,
    Position,
    Duration,
    State,
    CurrentFilePath,
    Input,
    Inputs,
    Loop,
}

#[derive(Subcommand, Debug)]
enum SetCommands {
    Volume { volume: f32 },
    Gain { gain: f32 },
    MicGain { mic_gain: f32 },
    Position { position: f32 },
    Input { name: String },
    Loop { enabled: String },
}

/// Entry point invoked from `main.rs` when argv[1] == "cli". `argv` is the
/// full process argv, including the program name; clap re-parses it (with
/// "cli" stripped) so its error messages reference `soundboard cli ...`.
pub async fn run(argv: Vec<String>) -> Result<()> {
    // Strip the leading "cli" subcommand token and feed clap a synthetic
    // argv[0] so generated help/usage reads `soundboard cli ...` instead of
    // `soundboard.exe ...`.
    let mut filtered: Vec<String> = Vec::with_capacity(argv.len());
    filtered.push("soundboard cli".to_string());
    for arg in argv.iter().skip(2) {
        filtered.push(arg.clone());
    }

    let cli = Cli::parse_from(filtered);

    wait_for_daemon().await?;

    let request = match cli.command {
        Commands::Action { action } => match action {
            Actions::Ping => Request::ping(),
            Actions::Pause => Request::pause(),
            Actions::Resume => Request::resume(),
            Actions::TogglePause => Request::toggle_pause(),
            Actions::Stop => Request::stop(),
            Actions::Play { file_path } => {
                let path_str = file_path
                    .to_str()
                    .ok_or("File path contains invalid UTF-8 characters")?;
                Request::play(path_str)
            }
            Actions::Preview { file_path } => {
                let path_str = file_path
                    .to_str()
                    .ok_or("File path contains invalid UTF-8 characters")?;
                Request::preview(path_str)
            }
            Actions::ToggleLoop => Request::toggle_loop(),
        },
        Commands::Get { parameter } => match parameter {
            GetCommands::IsPaused => Request::get_is_paused(),
            GetCommands::Volume => Request::get_volume(),
            GetCommands::Gain => Request::get_gain(),
            GetCommands::MicGain => Request::get_mic_gain(),
            GetCommands::Position => Request::get_position(),
            GetCommands::Duration => Request::get_duration(),
            GetCommands::State => Request::get_state(),
            GetCommands::CurrentFilePath => Request::get_current_file_path(),
            GetCommands::Input => Request::get_input(),
            GetCommands::Inputs => Request::get_inputs(),
            GetCommands::Loop => Request::get_loop(),
        },
        Commands::Set { parameter } => match parameter {
            SetCommands::Volume { volume } => Request::set_volume(volume),
            SetCommands::Gain { gain } => Request::set_gain(gain),
            SetCommands::MicGain { mic_gain } => Request::set_mic_gain(mic_gain),
            SetCommands::Position { position } => Request::seek(position),
            SetCommands::Input { name } => Request::set_input(&name),
            SetCommands::Loop { enabled } => Request::set_loop(&enabled),
        },
    };

    let response = make_request(request).await?;
    println!("{} : {}", response.status, response.message);

    Ok(())
}
