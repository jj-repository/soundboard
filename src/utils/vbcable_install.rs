//! First-run wizard for VB-Audio Virtual Cable.
//!
//! VB-Audio's licensing forbids redistributing the installer, so we fetch the
//! latest pack on demand from `download.vb-audio.com` over HTTPS, verify the
//! Authenticode signature on the extracted setup binary (signer must be
//! VB-Audio Software), and shell out to PowerShell `Start-Process -Verb RunAs`
//! to trigger the UAC prompt. Reboot is left to the OS / installer.

use crate::error::{Result, SoundboardError};
use futures_util::StreamExt;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::Duration;

const VBCABLE_URL: &str = "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack43.zip";
const ALLOWED_HOSTS: &[&str] = &["download.vb-audio.com", "vb-audio.com"];
const MAX_DOWNLOAD_BYTES: u64 = 32 * 1024 * 1024; // 32 MiB; pack is ~1 MiB
const SETUP_EXE_NAME: &str = "VBCABLE_Setup_x64.exe";
const SIGNER_SUBSTRING: &str = "VB-Audio";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const POLL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub enum InstallStatus {
    Downloading { downloaded: u64, total: u64 },
    Extracting,
    Verifying,
    Launching,
    WaitingForDevice,
    Done,
    NeedsReboot,
    Failed(String),
}

/// True if a VB-Audio Virtual Cable output endpoint is currently registered.
pub fn is_installed() -> bool {
    use rodio::cpal::{
        self,
        traits::{DeviceTrait, HostTrait},
    };
    let host = cpal::default_host();
    if let Ok(devices) = host.output_devices() {
        for device in devices {
            if let Ok(desc) = device.description() {
                let lower = desc.name().to_ascii_lowercase();
                if lower.contains("cable input") || lower.contains("vb-audio") {
                    return true;
                }
            }
        }
    }
    false
}

/// Cache directory used for the pack download + extraction.
fn cache_dir() -> Result<PathBuf> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| SoundboardError::Other("LOCALAPPDATA not available".to_string()))?;
    let dir = base.join("soundboard").join("cache").join("vbcable");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn is_allowed_host(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .map(|h| ALLOWED_HOSTS.iter().any(|a| h == *a))
        .unwrap_or(false)
}

/// Download the pack zip to `dest`, sending progress events on `tx`.
async fn download_pack(dest: &Path, tx: &Sender<InstallStatus>) -> Result<()> {
    if !is_allowed_host(VBCABLE_URL) {
        return Err(SoundboardError::Other(
            "VB-Cable URL host not in allowlist".to_string(),
        ));
    }

    let client = Client::builder()
        .user_agent("soundboard-vbcable-installer")
        .https_only(true)
        .build()?;

    let response = client.get(VBCABLE_URL).send().await?;
    if !response.status().is_success() {
        return Err(SoundboardError::Other(format!(
            "VB-Audio download returned status {}",
            response.status()
        )));
    }

    let total = response.content_length().unwrap_or(0);
    if total > MAX_DOWNLOAD_BYTES {
        return Err(SoundboardError::Other(format!(
            "VB-Cable pack reports {} bytes, exceeds {} cap",
            total, MAX_DOWNLOAD_BYTES
        )));
    }

    let mut file = File::create(dest)?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        downloaded = downloaded.saturating_add(bytes.len() as u64);
        if downloaded > MAX_DOWNLOAD_BYTES {
            return Err(SoundboardError::Other(
                "VB-Cable pack exceeded size cap mid-download".to_string(),
            ));
        }
        file.write_all(&bytes)?;
        let _ = tx.send(InstallStatus::Downloading { downloaded, total });
    }

    file.sync_all()?;
    Ok(())
}

/// Extract the zip into `dest_dir`, refusing entries that escape it (zip-slip).
fn extract_pack(zip_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SoundboardError::Other(format!("zip open failed: {e}")))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| SoundboardError::Other(format!("zip entry {i} failed: {e}")))?;
        let entry_path = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue, // refuse path-traversal entries
        };

        let out_path = dest_dir.join(&entry_path);
        // Containment check BEFORE creating directories. Walking up to the
        // nearest existing ancestor and canonicalising avoids the TOCTOU
        // window where a symlink planted between mkdir and check could
        // redirect extraction. Reject the whole entry if any ancestor is a
        // symlink.
        let canonical_dest = std::fs::canonicalize(dest_dir)?;
        if let Some(parent) = out_path.parent() {
            let mut ancestor = parent;
            let canonical_existing = loop {
                match std::fs::symlink_metadata(ancestor) {
                    Ok(meta) => {
                        if meta.file_type().is_symlink() {
                            return Err(SoundboardError::Other(format!(
                                "zip entry parent contains symlink: {}",
                                ancestor.display()
                            )));
                        }
                        break std::fs::canonicalize(ancestor)?;
                    }
                    Err(_) => match ancestor.parent() {
                        Some(p) => ancestor = p,
                        None => {
                            return Err(SoundboardError::Other(format!(
                                "zip entry has no existing ancestor under destination: {}",
                                entry_path.display()
                            )));
                        }
                    },
                }
            };
            if !canonical_existing.starts_with(&canonical_dest) {
                return Err(SoundboardError::Other(format!(
                    "zip entry escapes destination: {}",
                    entry_path.display()
                )));
            }
            std::fs::create_dir_all(parent)?;
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            let mut out = File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }

    Ok(())
}

/// Verify the extracted setup .exe is Authenticode-signed by VB-Audio.
///
/// We shell out to PowerShell `Get-AuthenticodeSignature` rather than calling
/// `WinVerifyTrust` directly to avoid a second `windows` dependency for one
/// call; the setup binary is small and the perf cost is negligible.
fn verify_signature(exe_path: &Path) -> Result<()> {
    let exe_str = exe_path
        .to_str()
        .ok_or_else(|| SoundboardError::Other("non-UTF8 setup path".to_string()))?;

    let script = format!(
        "$sig = Get-AuthenticodeSignature -FilePath '{}'; \
         if ($sig.Status -ne 'Valid') {{ Write-Output ('STATUS:' + $sig.Status); exit 1 }} \
         Write-Output ('SUBJECT:' + $sig.SignerCertificate.Subject)",
        exe_str.replace('\'', "''")
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| SoundboardError::Other(format!("powershell launch failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(SoundboardError::Other(format!(
            "signature invalid: stdout={} stderr={}",
            stdout.trim(),
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let subject = stdout
        .lines()
        .find_map(|l| l.strip_prefix("SUBJECT:"))
        .unwrap_or("");

    if !subject.contains(SIGNER_SUBSTRING) {
        return Err(SoundboardError::Other(format!(
            "signer subject does not match VB-Audio: {subject}"
        )));
    }

    Ok(())
}

/// Launch the setup binary elevated. Returns once the installer process exits
/// (whether successfully or cancelled).
fn run_setup_elevated(exe_path: &Path) -> Result<()> {
    let exe_str = exe_path
        .to_str()
        .ok_or_else(|| SoundboardError::Other("non-UTF8 setup path".to_string()))?;

    // -i = install, -h = headless. Documented on VB-Audio's forum; if the
    // installer ignores them it falls back to the interactive UI, which is
    // also acceptable behaviour.
    let script = format!(
        "Start-Process -FilePath '{}' -ArgumentList '-i','-h' -Verb RunAs -Wait",
        exe_str.replace('\'', "''")
    );

    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|e| SoundboardError::Other(format!("powershell launch failed: {e}")))?;

    if !status.success() {
        return Err(SoundboardError::Other(format!(
            "installer exited with status {status}"
        )));
    }
    Ok(())
}

/// Drive the full install flow on a worker thread; status updates flow back
/// via `tx`. Errors are reported as `InstallStatus::Failed` and the function
/// returns `Ok` so the caller's join doesn't observe a panic.
pub async fn run_install(tx: Sender<InstallStatus>) -> Result<()> {
    let dir = cache_dir()?;
    let zip_path = dir.join("VBCABLE_Driver_Pack.zip");
    let setup_path = dir.join(SETUP_EXE_NAME);

    let _ = tx.send(InstallStatus::Downloading {
        downloaded: 0,
        total: 0,
    });
    if let Err(e) = download_pack(&zip_path, &tx).await {
        let _ = tx.send(InstallStatus::Failed(format!("download: {e}")));
        return Ok(());
    }

    let _ = tx.send(InstallStatus::Extracting);
    if let Err(e) = extract_pack(&zip_path, &dir) {
        let _ = tx.send(InstallStatus::Failed(format!("extract: {e}")));
        return Ok(());
    }

    if !setup_path.exists() {
        let _ = tx.send(InstallStatus::Failed(format!(
            "{SETUP_EXE_NAME} missing after extract"
        )));
        return Ok(());
    }

    let _ = tx.send(InstallStatus::Verifying);
    if let Err(e) = verify_signature(&setup_path) {
        let _ = tx.send(InstallStatus::Failed(format!("verify: {e}")));
        return Ok(());
    }

    let _ = tx.send(InstallStatus::Launching);
    if let Err(e) = run_setup_elevated(&setup_path) {
        let _ = tx.send(InstallStatus::Failed(format!("launch: {e}")));
        return Ok(());
    }

    let _ = tx.send(InstallStatus::WaitingForDevice);
    let deadline = std::time::Instant::now() + POLL_TIMEOUT;
    loop {
        if is_installed() {
            let _ = tx.send(InstallStatus::Done);
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let _ = tx.send(InstallStatus::NeedsReboot);
            return Ok(());
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Compute SHA-256 of a file. Used by tests and exposed for diagnostics.
#[allow(dead_code)]
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_allowlist_accepts_vbaudio() {
        assert!(is_allowed_host(VBCABLE_URL));
        assert!(is_allowed_host("https://download.vb-audio.com/foo"));
    }

    #[test]
    fn host_allowlist_rejects_random_hosts() {
        assert!(!is_allowed_host("https://evil.example.com/VBCABLE.zip"));
        assert!(!is_allowed_host("https://github.com/x"));
    }

    #[test]
    fn host_allowlist_rejects_garbage() {
        assert!(!is_allowed_host("not-a-url"));
        assert!(!is_allowed_host(""));
    }
}
