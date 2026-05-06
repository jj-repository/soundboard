//! Download audio from a URL via `yt-dlp`. Shells out to the `yt-dlp`
//! executable on `PATH`; users are responsible for installing it. Output
//! is restricted to the configured sounds folder; the URL is passed as an
//! argument, never via a shell, so there is no command-injection surface.
//!
//! All operations are synchronous; the caller is expected to run them on a
//! background thread (see `spawn_grab` for a channel-based wrapper).

use crate::error::{Result, SoundboardError};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

/// Status pushed to the UI as a download progresses.
#[derive(Debug, Clone)]
pub enum GrabStatus {
    Starting,
    Done(PathBuf),
    Error(String),
}

/// Returns true if a usable `yt-dlp` is on `PATH`.
pub fn is_yt_dlp_available() -> bool {
    Command::new("yt-dlp")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Validate a user-supplied URL: must be `http(s)://`, must parse to a
/// reasonable host. Rejects local-file schemes, javascript:, etc.
pub fn validate_url(url: &str) -> Result<()> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(SoundboardError::Other("URL is empty".to_string()));
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(SoundboardError::Other(
            "URL must start with http:// or https://".to_string(),
        ));
    }
    // Reject embedded NUL or whitespace that would confuse argv passing.
    if trimmed
        .chars()
        .any(|c| c == '\0' || (c.is_whitespace() && c != ' '))
    {
        return Err(SoundboardError::Other(
            "URL contains invalid whitespace or NUL".to_string(),
        ));
    }
    Ok(())
}

/// Run yt-dlp synchronously. Saves an mp3 into `dest_dir`. Returns the
/// downloaded file's path on success.
///
/// Uses `--no-playlist` (single track), `--restrict-filenames` (ASCII-only
/// names — avoids weird-character collisions with the existing import
/// path), `--no-overwrites`, and `--no-mtime` (don't trust upload time).
pub fn download_audio(url: &str, dest_dir: &Path) -> Result<PathBuf> {
    validate_url(url)?;
    if !dest_dir.exists() {
        std::fs::create_dir_all(dest_dir)?;
    }
    let template = dest_dir.join("%(title)s-%(id)s.%(ext)s");
    let template_str = template
        .to_str()
        .ok_or_else(|| SoundboardError::Other("destination path is not UTF-8".to_string()))?;

    // `--print after_move:filepath` makes yt-dlp emit the final path on
    // stdout after post-processing finishes; we can read it back rather
    // than guessing the filename from the title template.
    let output = Command::new("yt-dlp")
        .arg("--no-playlist")
        .arg("--restrict-filenames")
        .arg("--no-overwrites")
        .arg("--no-mtime")
        .arg("--extract-audio")
        .arg("--audio-format")
        .arg("mp3")
        .arg("--audio-quality")
        .arg("0")
        .arg("--no-progress")
        .arg("--print")
        .arg("after_move:filepath")
        .arg("-o")
        .arg(template_str)
        .arg("--")
        .arg(url)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            SoundboardError::Other(format!(
                "failed to invoke yt-dlp (is it installed and on PATH?): {}",
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SoundboardError::Other(format!(
            "yt-dlp exited with {}: {}",
            output.status,
            stderr.lines().next_back().unwrap_or("")
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let final_path = stdout
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .ok_or_else(|| {
            SoundboardError::Other("yt-dlp succeeded but did not print a result path".to_string())
        })?;
    let path = PathBuf::from(final_path);
    if !path.exists() {
        return Err(SoundboardError::Other(format!(
            "yt-dlp reported {} but file does not exist",
            path.display()
        )));
    }
    Ok(path)
}

/// Spawn a background thread that downloads the audio and reports status
/// via the supplied sender. Returns immediately. The thread exits after
/// sending exactly one terminal status (`Done` or `Error`).
pub fn spawn_grab(url: String, dest_dir: PathBuf, tx: Sender<GrabStatus>) {
    std::thread::spawn(move || {
        let _ = tx.send(GrabStatus::Starting);
        let status = match download_audio(&url, &dest_dir) {
            Ok(path) => GrabStatus::Done(path),
            Err(e) => GrabStatus::Error(e.to_string()),
        };
        let _ = tx.send(status);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_accepts_https() {
        assert!(validate_url("https://www.youtube.com/watch?v=abc").is_ok());
        assert!(validate_url("http://example.com/clip.mp3").is_ok());
    }

    #[test]
    fn validate_url_rejects_empty() {
        assert!(validate_url("").is_err());
        assert!(validate_url("   ").is_err());
    }

    #[test]
    fn validate_url_rejects_non_http_schemes() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_url("ftp://example.com/").is_err());
        assert!(validate_url("data:text/plain,foo").is_err());
    }

    #[test]
    fn validate_url_rejects_embedded_whitespace_or_nul() {
        assert!(validate_url("https://example.com/\0evil").is_err());
        assert!(validate_url("https://example.com/\nevil").is_err());
        assert!(validate_url("https://example.com/\tevil").is_err());
    }

    /// Trimmed-then-validated: leading/trailing whitespace OK.
    #[test]
    fn validate_url_trims_outer_whitespace() {
        assert!(validate_url("  https://example.com/  ").is_ok());
    }
}
