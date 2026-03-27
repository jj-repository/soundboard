# Update System

**Status:** Fully implemented with download + progress UI. Missing: auto-check on startup, `auto_check_updates` toggle.

## Files
- `src/utils/updater.rs` — core logic
- `src/types/gui.rs` — UpdateStatus enum
- `src/gui/draw.rs` — settings panel update UI
- `src/gui/mod.rs` — update state management

## UpdateStatus
```rust
pub enum UpdateStatus {
    NotChecked, Checking, UpToDate,
    UpdateAvailable { latest_version, release_url, download_url },
    Downloading { progress: f32 },
    Downloaded { file_path },
    Error(String),
}
```

## GitHub
Repo: `jj-repository/soundboard`
Platform asset detection: .tar.gz/.deb (Linux), .zip/.msi (Windows)
semver version compare, reqwest HTTP, temp dir storage.

## Missing
- No `auto_check_updates` config field
- No startup update check
- No UI toggle for auto-check
