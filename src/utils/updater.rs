use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const GITHUB_REPO: &str = "jj-repository/soundboard";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Display version in X.YY format (e.g. "1.08")
const DISPLAY_VERSION: &str = "1.08";
/// Hard cap on update payload size to avoid OOM from an oversized or hostile asset.
const MAX_UPDATE_SIZE: u64 = 256 * 1024 * 1024;
/// Only these hosts are acceptable download origins. Redirects outside the allowlist are rejected.
const ALLOWED_DOWNLOAD_HOSTS: &[&str] = &["github.com", "objects.githubusercontent.com"];

/// Returns true if the URL's host is in the download allowlist.
fn is_allowed_host(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .map(|h| ALLOWED_DOWNLOAD_HOSTS.iter().any(|allowed| h == *allowed))
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: String,
    pub body: Option<String>,
    pub html_url: String,
    pub assets: Vec<GitHubAsset>,
    pub prerelease: bool,
    pub draft: bool,
}

#[derive(Debug, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_name: String,
    pub release_notes: Option<String>,
    pub release_url: String,
    pub download_url: Option<String>,
    pub update_available: bool,
}

pub async fn check_for_updates() -> Result<UpdateInfo, Box<dyn Error + Send + Sync>> {
    let client = Client::builder()
        .user_agent("pwsp-updater")
        .build()?;

    let url = format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(format!("GitHub API returned status: {}", response.status()).into());
    }

    let release: GitHubRelease = response.json().await?;

    // Parse versions (strip 'v' prefix if present)
    let latest_version_str = release.tag_name.trim_start_matches('v');
    let current_version_str = CURRENT_VERSION.trim_start_matches('v');

    let update_available = match (
        Version::parse(latest_version_str),
        Version::parse(current_version_str),
    ) {
        (Ok(latest), Ok(current)) => latest > current,
        _ => false,
    };

    // Find the appropriate asset for the current platform
    let download_url = release
        .assets
        .iter()
        .find(|a| {
            let name = a.name.to_lowercase();
            #[cfg(target_os = "linux")]
            {
                name.contains("linux") || name.ends_with(".tar.gz") || name.ends_with(".deb")
            }
            #[cfg(target_os = "windows")]
            {
                name.contains("windows") || name.ends_with(".exe") || name.ends_with(".zip") || name.ends_with(".msi")
            }
        })
        .map(|a| a.browser_download_url.clone());

    Ok(UpdateInfo {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest_version_str.to_string(),
        release_name: release.name,
        release_notes: release.body,
        release_url: release.html_url,
        download_url,
        update_available,
    })
}

fn verify_sha256(file_path: &Path, expected_hash: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut file = File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    // sha256sum format: "<hash>  <filename>" or just "<hash>"
    let expected = expected_hash
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    if actual != expected {
        return Err(format!(
            "SHA-256 mismatch! Expected: {}... Got: {}...",
            &expected[..expected.len().min(16)],
            &actual[..actual.len().min(16)]
        )
        .into());
    }
    Ok(())
}

pub async fn download_update(
    download_url: &str,
    progress_callback: impl Fn(u64, u64),
) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    if !is_allowed_host(download_url) {
        return Err(format!("Refusing download from untrusted host: {}", download_url).into());
    }

    let client = Client::builder()
        .user_agent("pwsp-updater")
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() > 5 {
                return attempt.error("too many redirects");
            }
            match attempt.url().host_str() {
                Some(host) if ALLOWED_DOWNLOAD_HOSTS
                    .iter()
                    .any(|h| host.eq_ignore_ascii_case(h)) =>
                {
                    attempt.follow()
                }
                _ => attempt.error("redirect host not in allowlist"),
            }
        }))
        .build()?;

    let response = client.get(download_url).send().await?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()).into());
    }

    let total_size = response.content_length().unwrap_or(0);
    if total_size > MAX_UPDATE_SIZE {
        return Err(format!(
            "Update too large: {} bytes (max {})",
            total_size, MAX_UPDATE_SIZE
        )
        .into());
    }

    // Use a predictable filename based on URL's extension to avoid trusting attacker-controlled filenames
    let extension = download_url
        .split('/')
        .next_back()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext)
        .filter(|ext| matches!(*ext, "zip" | "tar.gz" | "deb" | "exe" | "msi" | "gz"))
        .unwrap_or("bin");
    let filename = format!("pwsp-update.{}", extension);

    // Runtime-scoped download dir with restrictive perms, not shared /tmp
    let temp_dir = crate::utils::daemon::get_runtime_dir().join("updates");
    fs::create_dir_all(&temp_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp_dir, fs::Permissions::from_mode(0o700))?;
    }

    let file_path = temp_dir.join(filename);
    // Remove any pre-existing file so create_new cannot be subverted by a symlink someone else planted
    let _ = fs::remove_file(&file_path);
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(&file_path)?;

    // Stream to disk with an enforced size cap; abort and delete on breach.
    let mut response = response;
    let mut downloaded: u64 = 0;
    while let Some(chunk) = response.chunk().await? {
        downloaded += chunk.len() as u64;
        if downloaded > MAX_UPDATE_SIZE {
            drop(file);
            let _ = fs::remove_file(&file_path);
            return Err(format!("Update exceeded max size of {} bytes", MAX_UPDATE_SIZE).into());
        }
        file.write_all(&chunk)?;
        progress_callback(downloaded, total_size);
    }

    // SHA-256 verification: the sidecar MUST exist unless checksum verification is
    // explicitly opted out via PWSP_UPDATE_SKIP_CHECKSUM=1 (operator override for
    // legacy releases published before checksums were attached).
    let sha256_url = format!("{}.sha256", download_url);
    let allow_skip = std::env::var("PWSP_UPDATE_SKIP_CHECKSUM")
        .map(|v| v == "1")
        .unwrap_or(false);

    match client.get(&sha256_url).send().await {
        Ok(sha_response) if sha_response.status().is_success() => {
            let hash_content = sha_response.text().await?;
            if let Err(e) = verify_sha256(&file_path, &hash_content) {
                let _ = fs::remove_file(&file_path);
                return Err(e);
            }
        }
        Ok(sha_response) if sha_response.status() == reqwest::StatusCode::NOT_FOUND && allow_skip => {
            tracing::error!(
                "WARNING: SHA-256 sidecar missing for update. Skipping verification because \
                 PWSP_UPDATE_SKIP_CHECKSUM=1 was set."
            );
        }
        Ok(sha_response) => {
            let _ = fs::remove_file(&file_path);
            return Err(format!(
                "Refusing to install update: checksum fetch returned status {}",
                sha_response.status()
            )
            .into());
        }
        Err(e) => {
            let _ = fs::remove_file(&file_path);
            return Err(format!("Refusing to install update: checksum fetch failed: {}", e).into());
        }
    }

    Ok(file_path)
}

pub fn get_current_version() -> &'static str {
    DISPLAY_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Version comparison tests (TEST-10) ---

    #[test]
    fn test_version_parse_current() {
        assert!(Version::parse(CURRENT_VERSION).is_ok());
    }

    #[test]
    fn test_version_comparison_newer() {
        let latest = Version::parse("1.9.0").unwrap();
        let current = Version::parse("1.8.0").unwrap();
        assert!(latest > current);
    }

    #[test]
    fn test_version_comparison_same() {
        let latest = Version::parse("1.8.0").unwrap();
        let current = Version::parse("1.8.0").unwrap();
        assert!(latest <= current);
    }

    #[test]
    fn test_version_comparison_older() {
        let latest = Version::parse("1.7.0").unwrap();
        let current = Version::parse("1.8.0").unwrap();
        assert!(latest <= current);
    }

    #[test]
    fn test_version_strip_v_prefix() {
        let ver_str = "v1.8.0".trim_start_matches('v');
        assert_eq!(ver_str, "1.8.0");
        assert!(Version::parse(ver_str).is_ok());
    }

    #[test]
    fn test_version_invalid_semver_returns_false() {
        let result = match (Version::parse("not.valid"), Version::parse("1.8.0")) {
            (Ok(latest), Ok(current)) => latest > current,
            _ => false,
        };
        assert!(!result);
    }

    #[test]
    fn test_version_prerelease_comparison() {
        let latest = Version::parse("1.9.0-beta.1").unwrap();
        let current = Version::parse("1.8.0").unwrap();
        assert!(latest > current);
    }

    // --- UpdateInfo tests (TEST-17) ---

    #[test]
    fn test_github_release_deserialization() {
        let json = r#"{
            "tag_name": "v1.9.0",
            "name": "Release 1.9.0",
            "body": "Changelog here",
            "html_url": "https://github.com/test/repo/releases/v1.9.0",
            "assets": [],
            "prerelease": false,
            "draft": false
        }"#;
        let release: GitHubRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.tag_name, "v1.9.0");
        assert_eq!(release.body, Some("Changelog here".to_string()));
        assert!(!release.prerelease);
    }

    #[test]
    fn test_github_release_with_assets() {
        let json = r#"{
            "tag_name": "v1.9.0",
            "name": "Release",
            "body": null,
            "html_url": "https://example.com",
            "assets": [
                {"name": "PWSP-Linux.zip", "browser_download_url": "https://dl.example.com/linux.zip", "size": 1024},
                {"name": "PWSP-Windows.zip", "browser_download_url": "https://dl.example.com/win.zip", "size": 2048}
            ],
            "prerelease": false,
            "draft": false
        }"#;
        let release: GitHubRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.assets.len(), 2);
        assert_eq!(release.assets[0].name, "PWSP-Linux.zip");
        assert_eq!(release.assets[1].size, 2048);
    }

    #[test]
    fn test_display_version_format() {
        assert!(DISPLAY_VERSION.contains('.'));
        assert!(!DISPLAY_VERSION.starts_with('v'));
    }

    // --- Filename sanitization tests ---

    #[test]
    fn test_download_filename_known_extension() {
        let url = "https://github.com/repo/releases/download/v1.0.0/PWSP-Linux.zip";
        let ext = url
            .split('/')
            .next_back()
            .and_then(|name| name.rsplit_once('.'))
            .map(|(_, ext)| ext)
            .filter(|ext| matches!(*ext, "zip" | "tar.gz" | "deb" | "exe" | "msi" | "gz"))
            .unwrap_or("bin");
        assert_eq!(ext, "zip");
    }

    #[test]
    fn test_download_filename_unknown_extension() {
        let url = "https://github.com/repo/releases/download/v1.0.0/binary";
        let ext = url
            .split('/')
            .next_back()
            .and_then(|name| name.rsplit_once('.'))
            .map(|(_, ext)| ext)
            .filter(|ext| matches!(*ext, "zip" | "tar.gz" | "deb" | "exe" | "msi" | "gz"))
            .unwrap_or("bin");
        assert_eq!(ext, "bin");
    }

    // --- Host allowlist tests ---

    #[test]
    fn test_allowed_host_github_release_url() {
        assert!(is_allowed_host(
            "https://github.com/jj-repository/soundboard/releases/download/v1.0.0/PWSP-Linux.zip"
        ));
    }

    #[test]
    fn test_allowed_host_github_objects() {
        assert!(is_allowed_host(
            "https://objects.githubusercontent.com/release-assets/123/artifact.zip"
        ));
    }

    #[test]
    fn test_disallowed_host_rejected() {
        assert!(!is_allowed_host("https://evil.example.com/foo.zip"));
        assert!(!is_allowed_host("http://127.0.0.1/foo.zip"));
        assert!(!is_allowed_host("https://github.com.evil.com/foo.zip"));
    }

    #[test]
    fn test_malformed_url_rejected() {
        assert!(!is_allowed_host("not a url"));
        assert!(!is_allowed_host(""));
    }

    // --- verify_sha256 tests ---

    fn write_temp_file(contents: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, contents).unwrap();
        file
    }

    #[test]
    fn test_verify_sha256_matches() {
        // sha256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let file = write_temp_file(b"hello");
        let result = verify_sha256(
            file.path(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        );
        assert!(result.is_ok(), "should match: {:?}", result.err());
    }

    #[test]
    fn test_verify_sha256_mismatch() {
        let file = write_temp_file(b"hello");
        let result = verify_sha256(
            file.path(),
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_sha256_accepts_sidecar_format() {
        // GNU sha256sum format: "<hash>  <filename>"
        let file = write_temp_file(b"hello");
        let result = verify_sha256(
            file.path(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  PWSP-Linux.zip\n",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_sha256_uppercase_hash_accepted() {
        let file = write_temp_file(b"hello");
        let result = verify_sha256(
            file.path(),
            "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_sha256_empty_expected_rejected() {
        let file = write_temp_file(b"hello");
        assert!(verify_sha256(file.path(), "").is_err());
        assert!(verify_sha256(file.path(), "   \n").is_err());
    }

    #[test]
    fn test_verify_sha256_nonexistent_file() {
        let result = verify_sha256(
            Path::new("/nonexistent/path/to/missing/file"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_sha256_empty_file() {
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let file = write_temp_file(b"");
        let result = verify_sha256(
            file.path(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        );
        assert!(result.is_ok());
    }
}
