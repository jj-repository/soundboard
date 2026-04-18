use crate::{
    types::{
        audio_player::AudioPlayer,
        config::DaemonConfig,
        socket::{Request, Response},
    },
};
#[cfg(target_os = "linux")]
use crate::{DAEMON_OUTPUT_NAME, VIRTUAL_MIC_NAME};
#[cfg(target_os = "linux")]
use crate::utils::pipewire::{create_link, get_all_devices};
use std::path::PathBuf;
use std::{error::Error, fs};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, OnceCell},
    time::{Duration, sleep},
};

/// TCP port used for daemon IPC on Windows
#[cfg(target_os = "windows")]
pub const DAEMON_TCP_PORT: u16 = 19735;

static AUDIO_PLAYER: OnceCell<Mutex<AudioPlayer>> = OnceCell::const_new();

/// Initialize the audio player. Must be called before get_audio_player().
pub async fn init_audio_player() -> Result<(), Box<dyn Error + Send + Sync>> {
    if AUDIO_PLAYER.get().is_some() {
        return Ok(());
    }

    let player = AudioPlayer::new()
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
    AUDIO_PLAYER
        .set(Mutex::new(player))
        .map_err(|_| -> Box<dyn Error + Send + Sync> { "Audio player already initialized".into() })?;
    Ok(())
}

/// Get the audio player. Panics if init_audio_player() was not called first.
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
    DaemonConfig::load_from_file().unwrap_or_else(|e| {
        tracing::error!("Failed to load daemon config ({}), using defaults", e);
        let config = DaemonConfig::default();
        config.save_to_file().ok();
        config
    })
}

#[cfg(target_os = "linux")]
pub async fn link_player_to_virtual_mic() -> Result<(), Box<dyn Error>> {
    let (input_devices, output_devices) = get_all_devices().await?;

    let pwsp_daemon_output = match output_devices
        .into_iter()
        .find(|d| d.name == DAEMON_OUTPUT_NAME)
    {
        Some(device) => device,
        None => {
            tracing::error!("Could not find pwsp-daemon output device, skipping device linking");
            return Ok(());
        }
    };

    let pwsp_daemon_input = match input_devices
        .into_iter()
        .find(|d| d.name == VIRTUAL_MIC_NAME)
    {
        Some(device) => device,
        None => {
            tracing::error!("Could not find pwsp-daemon input device, skipping device linking");
            return Ok(());
        }
    };

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
            tracing::error!(
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
    #[cfg(target_os = "linux")]
    {
        dirs::runtime_dir().unwrap_or(PathBuf::from("/run/pwsp"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
            .join("pwsp")
            .join("run")
    }
}

pub fn create_runtime_dir() -> Result<(), Box<dyn Error>> {
    let runtime_dir = get_runtime_dir();
    if !runtime_dir.exists() {
        fs::create_dir_all(&runtime_dir)?;
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700))?;
    }

    Ok(())
}

pub fn is_daemon_running() -> Result<bool, Box<dyn Error>> {
    // The advisory lock is what tells us whether the daemon is alive; the lock
    // file contents are irrelevant, so do not truncate and race the running
    // daemon's open handle.
    let lock_file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(get_runtime_dir().join("daemon.lock"))?;
    match lock_file.try_lock() {
        Ok(_) => Ok(false),
        Err(_) => Ok(true),
    }
}

pub async fn wait_for_daemon() -> Result<(), Box<dyn Error>> {
    if is_daemon_running()? {
        return Ok(());
    }

    while !is_daemon_running()? {
        sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

async fn send_and_receive(
    mut stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    request: Request,
) -> Result<Response, Box<dyn Error + Send + Sync>> {
    use tokio::time::{timeout, Duration};

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

    const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

    let mut len_bytes = [0u8; 4];
    timeout(Duration::from_secs(5), stream.read_exact(&mut len_bytes))
        .await
        .map_err(|_| "Read timeout")?
        .map_err(|_| "Failed to read response length")?;

    let response_len = u32::from_le_bytes(len_bytes) as usize;

    if response_len > MAX_RESPONSE_SIZE {
        return Err(format!(
            "Response too large: {} bytes (max {})",
            response_len, MAX_RESPONSE_SIZE
        )
        .into());
    }

    let mut buffer = vec![0u8; response_len];
    timeout(Duration::from_secs(5), stream.read_exact(&mut buffer))
        .await
        .map_err(|_| "Read timeout")?
        .map_err(|_| "Failed to read response")?;

    Ok(serde_json::from_slice(&buffer)?)
}

pub async fn make_request(request: Request) -> Result<Response, Box<dyn Error + Send + Sync>> {
    use tokio::time::timeout;

    #[cfg(target_os = "linux")]
    {
        let socket_path = get_runtime_dir().join("daemon.sock");
        let stream = timeout(Duration::from_secs(2), tokio::net::UnixStream::connect(socket_path))
            .await
            .map_err(|_| "Connection timeout")?
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
        send_and_receive(stream, request).await
    }
    #[cfg(target_os = "windows")]
    {
        let addr = format!("127.0.0.1:{}", DAEMON_TCP_PORT);
        let stream = timeout(Duration::from_secs(2), tokio::net::TcpStream::connect(&addr))
            .await
            .map_err(|_| "Connection timeout")?
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
        send_and_receive(stream, request).await
    }
}
