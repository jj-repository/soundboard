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
    let client = Client::builder()
        .user_agent("pwsp-updater")
        .build()?;

    let response = client.get(download_url).send().await?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()).into());
    }

    let total_size = response.content_length().unwrap_or(0);

    // Use a predictable filename based on URL's extension to avoid trusting attacker-controlled filenames
    let extension = download_url
        .split('/')
        .next_back()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext)
        .filter(|ext| matches!(*ext, "zip" | "tar.gz" | "deb" | "exe" | "msi" | "gz"))
        .unwrap_or("bin");
    let filename = format!("pwsp-update.{}", extension);

    // Create temp directory for download
    let temp_dir = std::env::temp_dir().join("pwsp-updates");
    fs::create_dir_all(&temp_dir)?;

    let file_path = temp_dir.join(filename);
    let mut file = File::create(&file_path)?;

    let bytes = response.bytes().await?;
    let downloaded = bytes.len() as u64;
    file.write_all(&bytes)?;
    progress_callback(downloaded, total_size);

    // SHA-256 verification: download the .sha256 sidecar and verify
    let sha256_url = format!("{}.sha256", download_url);
    match client.get(&sha256_url).send().await {
        Ok(sha_response) if sha_response.status().is_success() => {
            let hash_content = sha_response.text().await?;
            if let Err(e) = verify_sha256(&file_path, &hash_content) {
                let _ = fs::remove_file(&file_path);
                return Err(e);
            }
        }
        Ok(_) => {
            // .sha256 file not found — skip verification (pre-existing release without checksums)
        }
        Err(_) => {
            // Network error fetching checksum — skip rather than block update
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
        assert!(!(latest > current));
    }

    #[test]
    fn test_version_comparison_older() {
        let latest = Version::parse("1.7.0").unwrap();
        let current = Version::parse("1.8.0").unwrap();
        assert!(!(latest > current));
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
}
