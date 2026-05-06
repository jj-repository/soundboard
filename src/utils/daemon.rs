use crate::{
    error::{Result, SoundboardError},
    types::{
        audio_player::AudioPlayer,
        config::DaemonConfig,
        socket::{Request, Response},
    },
};
use std::fs;
use std::path::PathBuf;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, OnceCell},
    time::{sleep, Duration},
};

/// TCP port used for daemon IPC. Localhost only.
pub const DAEMON_TCP_PORT: u16 = 19735;

static AUDIO_PLAYER: OnceCell<Mutex<AudioPlayer>> = OnceCell::const_new();

/// Initialize the audio player. Must be called before get_audio_player().
pub async fn init_audio_player() -> Result<()> {
    if AUDIO_PLAYER.get().is_some() {
        return Ok(());
    }

    let player = AudioPlayer::new().await?;
    AUDIO_PLAYER
        .set(Mutex::new(player))
        .map_err(|_| SoundboardError::AudioPlayerAlreadyInit)?;
    Ok(())
}

/// Get the audio player. Panics if `init_audio_player()` was not called first
/// — by construction, the daemon's `bootstrap()` guarantees init before the
/// IPC listener accepts, so any reachable caller has already paid the init.
/// Direct callers from outside the daemon path should use
/// `try_get_audio_player()` instead.
#[track_caller]
pub fn get_audio_player() -> &'static Mutex<AudioPlayer> {
    match AUDIO_PLAYER.get() {
        Some(p) => p,
        None => {
            tracing::error!(
                "audio player accessed before init_audio_player(); this is a bug — \
                 the daemon should have initialised the player during bootstrap"
            );
            panic!("audio player not initialised; call init_audio_player() first");
        }
    }
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

pub fn get_runtime_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
        .join("soundboard")
        .join("run")
}

pub fn create_runtime_dir() -> Result<()> {
    let runtime_dir = get_runtime_dir();
    if !runtime_dir.exists() {
        fs::create_dir_all(&runtime_dir)?;
    }
    Ok(())
}

/// Returns true if another process holds the daemon lock — i.e. an embedded
/// daemon (GUI or `--headless`) is already running. The advisory lock is
/// what tells us whether the daemon is alive; the lock file contents are
/// irrelevant, so do not truncate and race the running holder's open handle.
pub fn is_daemon_running() -> Result<bool> {
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

pub async fn wait_for_daemon() -> Result<()> {
    if is_daemon_running()? {
        return Ok(());
    }

    while !is_daemon_running()? {
        sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

pub(crate) async fn send_and_receive(
    mut stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    request: Request,
) -> Result<Response> {
    use tokio::time::{timeout, Duration};

    let request_data = serde_json::to_vec(&request)?;
    let request_len = request_data.len() as u32;

    timeout(
        Duration::from_secs(2),
        stream.write_all(&request_len.to_le_bytes()),
    )
    .await
    .map_err(|_| SoundboardError::Ipc("send timeout".into()))?
    .map_err(|e| SoundboardError::Ipc(format!("failed to send request length: {e}")))?;

    timeout(Duration::from_secs(2), stream.write_all(&request_data))
        .await
        .map_err(|_| SoundboardError::Ipc("send timeout".into()))?
        .map_err(|e| SoundboardError::Ipc(format!("failed to send request: {e}")))?;

    // Responses are small JSON blobs (state, paths, device lists). 1 MiB is
    // already generous; the prior 10 MiB cap let a single length prefix
    // reserve 10 MiB per call.
    const MAX_RESPONSE_SIZE: usize = 1024 * 1024;

    let mut len_bytes = [0u8; 4];
    timeout(Duration::from_secs(5), stream.read_exact(&mut len_bytes))
        .await
        .map_err(|_| SoundboardError::Ipc("read timeout".into()))?
        .map_err(|e| SoundboardError::Ipc(format!("failed to read response length: {e}")))?;

    let response_len = u32::from_le_bytes(len_bytes) as usize;

    if response_len > MAX_RESPONSE_SIZE {
        return Err(SoundboardError::ResponseTooLarge {
            actual: response_len,
            max: MAX_RESPONSE_SIZE,
        });
    }

    let mut buffer = vec![0u8; response_len];
    timeout(Duration::from_secs(5), stream.read_exact(&mut buffer))
        .await
        .map_err(|_| SoundboardError::Ipc("read timeout".into()))?
        .map_err(|e| SoundboardError::Ipc(format!("failed to read response: {e}")))?;

    Ok(serde_json::from_slice(&buffer)?)
}

pub async fn make_request(request: Request) -> Result<Response> {
    use tokio::time::timeout;

    let addr = format!("127.0.0.1:{}", DAEMON_TCP_PORT);
    let stream = timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    .map_err(|_| SoundboardError::DaemonTimeout)?
    .map_err(|e| SoundboardError::DaemonConnect(e.to_string()))?;
    send_and_receive(stream, request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    /// Drive `send_and_receive` against an in-memory duplex pipe acting as a
    /// fake server. Exercises the length-prefix framing + response decoding
    /// without binding a real port or initialising the audio backend.
    #[tokio::test]
    async fn send_and_receive_round_trip() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);

        let server_task = tokio::spawn(async move {
            // Read the request length + body the client sends.
            let mut len_bytes = [0u8; 4];
            tokio::io::AsyncReadExt::read_exact(&mut server, &mut len_bytes)
                .await
                .unwrap();
            let len = u32::from_le_bytes(len_bytes) as usize;
            let mut body = vec![0u8; len];
            tokio::io::AsyncReadExt::read_exact(&mut server, &mut body)
                .await
                .unwrap();

            // Reply with a known Response.
            let response = Response::new(true, "pong");
            let bytes = serde_json::to_vec(&response).unwrap();
            server
                .write_all(&(bytes.len() as u32).to_le_bytes())
                .await
                .unwrap();
            server.write_all(&bytes).await.unwrap();
        });

        let response = send_and_receive(client, Request::ping()).await.unwrap();
        assert!(response.status);
        assert_eq!(response.message, "pong");
        server_task.await.unwrap();
    }

    /// A response advertising more than 1 MiB must be rejected before the
    /// client allocates the buffer.
    #[tokio::test]
    async fn send_and_receive_rejects_oversized_response() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);

        let server_task = tokio::spawn(async move {
            let mut len_bytes = [0u8; 4];
            tokio::io::AsyncReadExt::read_exact(&mut server, &mut len_bytes)
                .await
                .unwrap();
            let len = u32::from_le_bytes(len_bytes) as usize;
            let mut body = vec![0u8; len];
            tokio::io::AsyncReadExt::read_exact(&mut server, &mut body)
                .await
                .unwrap();

            // Lie about the response size.
            let huge = 2_u32 * 1024 * 1024;
            server.write_all(&huge.to_le_bytes()).await.unwrap();
        });

        let err = send_and_receive(client, Request::ping())
            .await
            .expect_err("expected ResponseTooLarge");
        match err {
            SoundboardError::ResponseTooLarge { actual, max } => {
                assert!(actual > max);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
        server_task.await.unwrap();
    }
}
