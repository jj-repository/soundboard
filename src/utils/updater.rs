use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

const GITHUB_REPO: &str = "jj-repository/soundboard";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

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

    // Find the appropriate asset for Linux
    let download_url = release
        .assets
        .iter()
        .find(|a| {
            let name = a.name.to_lowercase();
            name.contains("linux") || name.ends_with(".tar.gz") || name.ends_with(".deb")
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

    // Get filename from URL and sanitize to prevent directory traversal
    let raw_filename = download_url
        .split('/')
        .next_back()
        .unwrap_or("pwsp-update");

    // Sanitize filename: remove path separators and traversal sequences
    let filename: String = raw_filename
        .chars()
        .filter(|c| *c != '/' && *c != '\\' && *c != '\0')
        .collect();
    let filename = filename.trim_start_matches('.'); // Remove leading dots
    let filename = if filename.is_empty() {
        "pwsp-update"
    } else {
        &filename
    };

    // Create temp directory for download
    let temp_dir = std::env::temp_dir().join("pwsp-updates");
    fs::create_dir_all(&temp_dir)?;

    let file_path = temp_dir.join(filename);
    let mut file = File::create(&file_path)?;

    let bytes = response.bytes().await?;
    let downloaded = bytes.len() as u64;
    file.write_all(&bytes)?;
    progress_callback(downloaded, total_size);

    Ok(file_path)
}

pub fn get_current_version() -> &'static str {
    CURRENT_VERSION
}
