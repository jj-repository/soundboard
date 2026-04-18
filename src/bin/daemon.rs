use soundboard::{
    types::{
        audio_player::PlayerState,
        socket::{Request, Response},
    },
    utils::{
        commands::parse_command,
        daemon::{
            create_runtime_dir, get_audio_player, get_daemon_config, get_runtime_dir,
            init_audio_player, is_daemon_running,
        },
    },
};
#[cfg(target_os = "linux")]
use soundboard::utils::{
    daemon::link_player_to_virtual_mic,
    pipewire::create_virtual_mic,
};
use std::{error::Error, fs, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::{sleep, timeout},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    soundboard::utils::logging::init();
    create_runtime_dir()?;

    if is_daemon_running()? {
        return Err("Another instance is already running.".into());
    }

    get_daemon_config();

    #[cfg(target_os = "linux")]
    create_virtual_mic()?;

    if let Err(e) = init_audio_player().await {
        tracing::error!("Failed to initialize audio player: {}", e);
        return Err(format!("Cannot start daemon: audio player initialization failed: {}", e).into());
    }

    #[cfg(target_os = "linux")]
    link_player_to_virtual_mic().await?;

    let runtime_dir = get_runtime_dir();

    let _lock_file = {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(runtime_dir.join("daemon.lock"))?
        }
        #[cfg(target_os = "windows")]
        {
            fs::File::create(runtime_dir.join("daemon.lock"))?
        }
    };
    _lock_file.lock()?;

    #[cfg(target_os = "linux")]
    let listener = {
        let socket_path = runtime_dir.join("daemon.sock");
        // Remove stale socket unconditionally (avoids TOCTOU race)
        let _ = fs::remove_file(&socket_path);

        let listener = tokio::net::UnixListener::bind(&socket_path)?;

        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;

        tracing::info!(
            "Daemon started. Listening on {}",
            socket_path.to_str().unwrap_or_default()
        );

        listener
    };

    #[cfg(target_os = "windows")]
    let listener = {
        use soundboard::utils::daemon::DAEMON_TCP_PORT;
        let addr = format!("127.0.0.1:{}", DAEMON_TCP_PORT);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("Daemon started. Listening on {}", addr);
        listener
    };

    let commands_loop_handle = tokio::spawn(async move {
        if let Err(e) = commands_loop(listener).await {
            tracing::error!("Commands loop error: {}", e);
        }
    });

    let player_loop_handle = tokio::spawn(async {
        player_loop().await;
    });

    tokio::select! {
        _ = commands_loop_handle => {
            tracing::error!("Commands loop was finished, stopping program...");
        }
        _ = player_loop_handle => {
            tracing::error!("Audio Player loop was finished, stopping program...");
        }
    }

    #[cfg(target_os = "linux")]
    {
        let socket_path = get_runtime_dir().join("daemon.sock");
        let _ = fs::remove_file(&socket_path);
    }

    Ok(())
}

// Legitimate requests are a few hundred bytes; the old 10 MiB cap let a single
// length prefix reserve 10 MiB per connection and sit on it forever.
const MAX_IPC_MESSAGE_SIZE: usize = 64 * 1024;
// Upper bound on how many args a single request may carry; parse_command
// never reads more than a handful, so 32 is generous.
const MAX_IPC_ARGS: usize = 32;
// Whole-request deadline: guards against clients that write the length prefix
// but then stall on the body.
const IPC_READ_TIMEOUT: Duration = Duration::from_secs(5);

async fn handle_connection(mut stream: impl AsyncRead + AsyncWrite + Unpin) {
    let mut len_bytes = [0u8; 4];
    match timeout(IPC_READ_TIMEOUT, stream.read_exact(&mut len_bytes)).await {
        Err(_) => {
            tracing::error!("IPC: timed out reading message length");
            return;
        }
        Ok(Err(_)) => {
            tracing::error!("Failed to read message length from client!");
            return;
        }
        Ok(Ok(_)) => {}
    }

    let request_len = u32::from_le_bytes(len_bytes) as usize;

    if request_len > MAX_IPC_MESSAGE_SIZE {
        tracing::error!("Rejected message: size {} exceeds maximum allowed {}", request_len, MAX_IPC_MESSAGE_SIZE);
        return;
    }

    let mut buffer = vec![0u8; request_len];
    match timeout(IPC_READ_TIMEOUT, stream.read_exact(&mut buffer)).await {
        Err(_) => {
            tracing::error!("IPC: timed out reading message body");
            return;
        }
        Ok(Err(_)) => {
            tracing::error!("Failed to read message from client!");
            return;
        }
        Ok(Ok(_)) => {}
    }

    let request: Request = match serde_json::from_slice(&buffer) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!("Failed to parse request JSON: {}", e);
            return;
        }
    };

    if request.args.len() > MAX_IPC_ARGS {
        tracing::error!(
            "Rejected request '{}': {} args exceeds limit of {}",
            request.name,
            request.args.len(),
            MAX_IPC_ARGS
        );
        return;
    }

    let command = parse_command(&request);
    let response: Response = match command {
        Some(cmd) => cmd.execute().await,
        None => Response::new(false, "Unknown command"),
    };

    let response_data = match serde_json::to_vec(&response) {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("Failed to serialize response: {}", e);
            return;
        }
    };
    let response_len = response_data.len() as u32;

    if stream.write_all(&response_len.to_le_bytes()).await.is_err() {
        tracing::error!("Failed to write response length to client!");
        return;
    }
    if stream.write_all(&response_data).await.is_err() {
        tracing::error!("Failed to write response to client!");
    }
}

#[cfg(target_os = "linux")]
async fn commands_loop(listener: tokio::net::UnixListener) -> Result<(), Box<dyn Error>> {
    loop {
        let (stream, _addr) = listener.accept().await?;
        tokio::spawn(handle_connection(stream));
    }
}

#[cfg(target_os = "windows")]
async fn commands_loop(listener: tokio::net::TcpListener) -> Result<(), Box<dyn Error>> {
    loop {
        let (stream, _addr) = listener.accept().await?;
        tokio::spawn(handle_connection(stream));
    }
}

async fn player_loop() {
    loop {
        let mut audio_player = get_audio_player().lock().await;

        if audio_player.get_state() == PlayerState::Stopped && audio_player.looped {
            if let Some(ref file_path) = audio_player.current_file_path.clone() {
                if let Err(e) = audio_player.play(file_path).await {
                    tracing::error!("Failed to play looped file: {}", e);
                }
            }
        }

        drop(audio_player);
        sleep(Duration::from_millis(100)).await;
    }
}
