use pwsp::{
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
use pwsp::utils::{
    daemon::link_player_to_virtual_mic,
    pipewire::create_virtual_mic,
};
use std::{error::Error, fs, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::sleep,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    create_runtime_dir()?;

    if is_daemon_running()? {
        return Err("Another instance is already running.".into());
    }

    get_daemon_config();

    #[cfg(target_os = "linux")]
    create_virtual_mic()?;

    if let Err(e) = init_audio_player().await {
        eprintln!("Failed to initialize audio player: {}", e);
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

        println!(
            "Daemon started. Listening on {}",
            socket_path.to_str().unwrap_or_default()
        );

        listener
    };

    #[cfg(target_os = "windows")]
    let listener = {
        use pwsp::utils::daemon::DAEMON_TCP_PORT;
        let addr = format!("127.0.0.1:{}", DAEMON_TCP_PORT);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        println!("Daemon started. Listening on {}", addr);
        listener
    };

    let commands_loop_handle = tokio::spawn(async move {
        if let Err(e) = commands_loop(listener).await {
            eprintln!("Commands loop error: {}", e);
        }
    });

    let player_loop_handle = tokio::spawn(async {
        player_loop().await;
    });

    tokio::select! {
        _ = commands_loop_handle => {
            eprintln!("Commands loop was finished, stopping program...");
        }
        _ = player_loop_handle => {
            eprintln!("Audio Player loop was finished, stopping program...");
        }
    }

    #[cfg(target_os = "linux")]
    {
        let socket_path = get_runtime_dir().join("daemon.sock");
        let _ = fs::remove_file(&socket_path);
    }

    Ok(())
}

// 10MB limit prevents DoS via excessive memory allocation
const MAX_IPC_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

async fn handle_connection(mut stream: impl AsyncRead + AsyncWrite + Unpin) {
    let mut len_bytes = [0u8; 4];
    if stream.read_exact(&mut len_bytes).await.is_err() {
        eprintln!("Failed to read message length from client!");
        return;
    }

    let request_len = u32::from_le_bytes(len_bytes) as usize;

    if request_len > MAX_IPC_MESSAGE_SIZE {
        eprintln!("Rejected message: size {} exceeds maximum allowed {}", request_len, MAX_IPC_MESSAGE_SIZE);
        return;
    }

    let mut buffer = vec![0u8; request_len];
    if stream.read_exact(&mut buffer).await.is_err() {
        eprintln!("Failed to read message from client!");
        return;
    }

    let request: Request = match serde_json::from_slice(&buffer) {
        Ok(req) => req,
        Err(e) => {
            eprintln!("Failed to parse request JSON: {}", e);
            return;
        }
    };

    let command = parse_command(&request);
    let response: Response = match command {
        Some(cmd) => cmd.execute().await,
        None => Response::new(false, "Unknown command"),
    };

    let response_data = match serde_json::to_vec(&response) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to serialize response: {}", e);
            return;
        }
    };
    let response_len = response_data.len() as u32;

    if stream.write_all(&response_len.to_le_bytes()).await.is_err() {
        eprintln!("Failed to write response length to client!");
        return;
    }
    if stream.write_all(&response_data).await.is_err() {
        eprintln!("Failed to write response to client!");
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
                    eprintln!("Failed to play looped file: {}", e);
                }
            }
        }

        drop(audio_player);
        sleep(Duration::from_millis(100)).await;
    }
}
