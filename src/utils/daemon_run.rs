//! Embedded daemon runtime. Was a separate `soundboard-daemon` binary; now
//! runs in the same process as the GUI (or as a `--headless` invocation).

use crate::{
    error::Result,
    types::{
        audio_player::PlayerState,
        socket::{Request, Response},
    },
    utils::{
        commands::parse_command,
        daemon::{
            DAEMON_TCP_PORT, create_runtime_dir, get_audio_player, get_daemon_config,
            get_runtime_dir, init_audio_player, is_daemon_running,
        },
    },
};
use std::{
    fs,
    sync::OnceLock,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    task::JoinHandle,
    time::{sleep, timeout},
};

/// Holds the daemon lock file for the lifetime of the process.
static LOCK_FILE: OnceLock<fs::File> = OnceLock::new();

/// Initialise the embedded daemon: acquire the cross-process lock, init the
/// audio player, and start the TCP IPC listener + player loop. Returns the
/// background task handles; callers can either await them (`run_headless`)
/// or drop them to keep the daemon running for the rest of the process
/// lifetime (GUI embed).
async fn bootstrap() -> Result<(JoinHandle<()>, JoinHandle<()>)> {
    create_runtime_dir()?;

    if is_daemon_running()? {
        return Err("another instance is already running".into());
    }

    get_daemon_config();

    init_audio_player().await?;

    let runtime_dir = get_runtime_dir();
    let lock_file = fs::File::create(runtime_dir.join("daemon.lock"))?;
    lock_file.lock()?;
    let _ = LOCK_FILE.set(lock_file);

    let addr = format!("127.0.0.1:{}", DAEMON_TCP_PORT);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Daemon started. Listening on {}", addr);

    let commands_handle = tokio::spawn(async move {
        if let Err(e) = commands_loop(listener).await {
            tracing::error!("Commands loop error: {}", e);
        }
    });

    let player_handle = tokio::spawn(async {
        player_loop().await;
    });

    Ok((commands_handle, player_handle))
}

/// Run the daemon and return only when one of the loops exits. Used by
/// `--headless` mode.
pub async fn run_headless() -> Result<()> {
    let (commands_handle, player_handle) = bootstrap().await?;

    tokio::select! {
        _ = commands_handle => {
            tracing::error!("Commands loop was finished, stopping program...");
        }
        _ = player_handle => {
            tracing::error!("Audio Player loop was finished, stopping program...");
        }
    }

    Ok(())
}

/// Start the daemon as background tasks and return immediately. Used by the
/// GUI to embed the daemon in-process; the loops run for the rest of the
/// GUI's lifetime.
pub async fn start_in_process() -> Result<()> {
    let (_commands_handle, _player_handle) = bootstrap().await?;
    // Detach: the lock file in LOCK_FILE keeps the lock until process exit.
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
        tracing::error!(
            "Rejected message: size {} exceeds maximum allowed {}",
            request_len, MAX_IPC_MESSAGE_SIZE
        );
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
        None => Response::new(false, "Unknown or malformed command"),
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

async fn commands_loop(listener: tokio::net::TcpListener) -> Result<()> {
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
