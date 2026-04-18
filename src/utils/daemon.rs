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

    let soundboard_daemon_output = match output_devices
        .into_iter()
        .find(|d| d.name == DAEMON_OUTPUT_NAME)
    {
        Some(device) => device,
        None => {
            tracing::error!("Could not find soundboard-daemon output device, skipping device linking");
            return Ok(());
        }
    };

    let soundboard_daemon_input = match input_devices
        .into_iter()
        .find(|d| d.name == VIRTUAL_MIC_NAME)
    {
        Some(device) => device,
        None => {
            tracing::error!("Could not find soundboard-daemon input device, skipping device linking");
            return Ok(());
        }
    };

    match (
        &soundboard_daemon_output.output_fl,
        &soundboard_daemon_output.output_fr,
        &soundboard_daemon_input.input_fl,
        &soundboard_daemon_input.input_fr,
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
        dirs::runtime_dir().unwrap_or(PathBuf::from("/run/soundboard"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
            .join("soundboard")
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

/// If no daemon is currently running, spawn `soundboard-daemon` from the same
/// directory as the current executable. Detached so closing the GUI does not
/// tear the daemon down.
///
/// Returns Ok(()) if a daemon is already running or was spawned successfully.
/// Returns Err if the daemon binary can't be located or the spawn itself
/// fails; callers can still proceed — the GUI has a `wait_for_daemon` poll
/// loop that tolerates a delayed startup.
pub fn spawn_daemon_if_not_running() -> Result<(), Box<dyn Error>> {
    if is_daemon_running().unwrap_or(false) {
        return Ok(());
    }

    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or("could not resolve current exe directory")?;

    let daemon_name = if cfg!(target_os = "windows") {
        "soundboard-daemon.exe"
    } else {
        "soundboard-daemon"
    };
    let daemon_path = exe_dir.join(daemon_name);

    if !daemon_path.is_file() {
        return Err(format!(
            "soundboard-daemon not found next to GUI at {}",
            daemon_path.display()
        )
        .into());
    }

    let mut cmd = std::process::Command::new(&daemon_path);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS: child has no console attached and survives
        // parent exit. CREATE_NEW_PROCESS_GROUP: child is not killed when
        // the GUI's process group receives Ctrl+C.
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid() is async-signal-safe and the only non-trivial
        // operation performed between fork and exec. It detaches the child
        // from the parent's controlling terminal/session so the daemon
        // isn't killed by SIGHUP when the GUI (or its terminal) exits.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    cmd.spawn()?;
    tracing::info!("Spawned soundboard-daemon from {}", daemon_path.display());
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
