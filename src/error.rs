//! Crate-wide error type. Replaces the prior `Box<dyn Error>` returns so
//! callers can match on transient vs. permanent failure modes (e.g. CLI/GUI
//! distinguishing `DaemonNotRunning` from `FileNotFound`).

use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, SoundboardError>;

#[derive(Debug, Error)]
pub enum SoundboardError {
    #[error("daemon is not running")]
    DaemonNotRunning,

    #[error("daemon connection timed out")]
    DaemonTimeout,

    #[error("daemon connection failed: {0}")]
    DaemonConnect(String),

    #[error("audio player is already initialized")]
    AudioPlayerAlreadyInit,

    #[error("IPC failed: {0}")]
    Ipc(String),

    #[error("response too large: {actual} bytes (max {max})")]
    ResponseTooLarge { actual: usize, max: usize },

    #[error("audio device error: {0}")]
    AudioDevice(String),

    #[error("audio decode failed: {0}")]
    AudioDecode(String),

    #[error("seek failed: {0}")]
    Seek(String),

    #[error("nothing is playing")]
    NothingPlaying,

    #[error("duration unavailable for current file")]
    DurationUnknown,

    #[error("layer index {index} out of range (max {max})")]
    LayerOutOfRange { index: usize, max: usize },

    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("path is outside allowed directory: {0}")]
    PathNotAllowed(PathBuf),

    #[error("config error: {0}")]
    Config(String),

    #[error("config directory unavailable")]
    ConfigDirUnavailable,

    #[error("update download failed: {0}")]
    UpdateDownload(String),

    #[error("update checksum mismatch: expected {expected}, got {actual}")]
    UpdateChecksum { expected: String, actual: String },

    #[error("update parse failed: {0}")]
    UpdateParse(String),

    #[error("device not found: {0}")]
    DeviceNotFound(String),

    #[error("PipeWire: {0}")]
    PipeWire(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    /// Catch-all for legacy `String` / `&str` errors. Prefer a typed variant
    /// when adding new code paths.
    #[error("{0}")]
    Other(String),
}

impl From<&str> for SoundboardError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}

impl From<String> for SoundboardError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}
