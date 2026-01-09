use crate::{
    types::{
        audio_player::AudioPlayer,
        config::DaemonConfig,
        socket::{Request, Response},
    },
    utils::pipewire::{create_link, get_all_devices},
};
use std::path::PathBuf;
use std::{error::Error, fs};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{Mutex, OnceCell},
    time::{Duration, sleep},
};

static AUDIO_PLAYER: OnceCell<Mutex<AudioPlayer>> = OnceCell::const_new();

/// Initialize the audio player. Must be called before get_audio_player().
/// Returns an error if initialization fails.
pub async fn init_audio_player() -> Result<(), Box<dyn Error + Send + Sync>> {
    if AUDIO_PLAYER.get().is_some() {
        return Ok(()); // Already initialized
    }

    println!("Initializing audio player");
    let player = AudioPlayer::new()
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
    AUDIO_PLAYER
        .set(Mutex::new(player))
        .map_err(|_| -> Box<dyn Error + Send + Sync> { "Audio player already initialized".into() })?;
    Ok(())
}

/// Get the audio player. Panics if init_audio_player() was not called first.
/// Use try_get_audio_player() for a fallible version.
pub fn get_audio_player() -> &'static Mutex<AudioPlayer> {
    AUDIO_PLAYER
        .get()
        .expect("Audio player not initialized. Call init_audio_player() first.")
}

/// Try to get the audio player, returning None if not initialized.
pub fn try_get_audio_player() -> Option<&'static Mutex<AudioPlayer>> {
    AUDIO_PLAYER.get()
}

pub fn get_daemon_config() -> DaemonConfig {
    DaemonConfig::load_from_file().unwrap_or_else(|_| {
        let config = DaemonConfig::default();
        config.save_to_file().ok();
        config
    })
}

pub async fn link_player_to_virtual_mic() -> Result<(), Box<dyn Error>> {
    let (input_devices, output_devices) = get_all_devices().await?;

    let pwsp_daemon_output = match output_devices
        .into_iter()
        .find(|d| d.name == "alsa_playback.pwsp-daemon")
    {
        Some(device) => device,
        None => {
            println!("Could not find pwsp-daemon output device, skipping device linking");
            return Ok(());
        }
    };

    let pwsp_daemon_input = match input_devices
        .into_iter()
        .find(|d| d.name == "pwsp-virtual-mic")
    {
        Some(device) => device,
        None => {
            println!("Could not find pwsp-daemon input device, skipping device linking");
            return Ok(());
        }
    };

    // Check if all required ports are available
    match (
        &pwsp_daemon_output.output_fl,
        &pwsp_daemon_output.output_fr,
        &pwsp_daemon_input.input_fl,
        &pwsp_daemon_input.input_fr,
    ) {
        (Some(output_fl), Some(output_fr), Some(input_fl), Some(input_fr)) => {
            create_link(
                output_fl.clone(),
                output_fr.clone(),
                input_fl.clone(),
                input_fr.clone(),
            )?;
        }
        (out_fl, out_fr, in_fl, in_fr) => {
            eprintln!(
                "Required ports not available (output_fl: {}, output_fr: {}, input_fl: {}, input_fr: {}), skipping device linking",
                out_fl.is_some(),
                out_fr.is_some(),
                in_fl.is_some(),
                in_fr.is_some()
            );
        }
    }

    Ok(())
}

pub fn get_runtime_dir() -> PathBuf {
    dirs::runtime_dir().unwrap_or(PathBuf::from("/run/pwsp"))
}

pub fn create_runtime_dir() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let runtime_dir = get_runtime_dir();
    if !runtime_dir.exists() {
        fs::create_dir_all(&runtime_dir)?;
    }

    // Security: Ensure runtime directory is owner-only (0700)
    // This prevents other users from accessing socket and lock files
    fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700))?;

    Ok(())
}

pub fn is_daemon_running() -> Result<bool, Box<dyn Error>> {
    let lock_file = fs::File::create(get_runtime_dir().join("daemon.lock"))?;
    match lock_file.try_lock() {
        Ok(_) => Ok(false),
        Err(_) => Ok(true),
    }
}

pub async fn wait_for_daemon() -> Result<(), Box<dyn Error>> {
    if is_daemon_running()? {
        return Ok(());
    }

    println!("Daemon not found, waiting for it...");
    while !is_daemon_running()? {
        sleep(Duration::from_millis(100)).await;
    }

    println!("Found running daemon");

    Ok(())
}

pub async fn make_request(request: Request) -> Result<Response, Box<dyn Error + Send + Sync>> {
    use tokio::time::{timeout, Duration};

    let socket_path = get_runtime_dir().join("daemon.sock");

    // Add timeout for connection to prevent GUI freeze
    let mut stream = timeout(Duration::from_secs(2), UnixStream::connect(socket_path))
        .await
        .map_err(|_| "Connection timeout")??;

    // ---------- Send request (start) ----------
    let request_data = serde_json::to_vec(&request)?;
    let request_len = request_data.len() as u32;

    timeout(Duration::from_secs(2), stream.write_all(&request_len.to_le_bytes()))
        .await
        .map_err(|_| "Send timeout")?
        .map_err(|_| "Failed to send request length")?;

    timeout(Duration::from_secs(2), stream.write_all(&request_data))
        .await
        .map_err(|_| "Send timeout")?
        .map_err(|_| "Failed to send request")?;
    // ---------- Send request (end) ----------

    // ---------- Read response (start) ----------
    let mut len_bytes = [0u8; 4];
    timeout(Duration::from_secs(5), stream.read_exact(&mut len_bytes))
        .await
        .map_err(|_| "Read timeout")?
        .map_err(|_| "Failed to read response length")?;

    let response_len = u32::from_le_bytes(len_bytes) as usize;

    let mut buffer = vec![0u8; response_len];
    timeout(Duration::from_secs(5), stream.read_exact(&mut buffer))
        .await
        .map_err(|_| "Read timeout")?
        .map_err(|_| "Failed to read response")?;
    // ---------- Read response (end) ----------

    Ok(serde_json::from_slice(&buffer)?)
}
