# Update System

**Status:** Implemented with download + progress UI. Missing: auto-check on startup, `auto_check_updates` toggle.

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
Platform asset detection: .zip / .msi / .exe (Windows-only after refactor); rejects assets with "linux" in the name.
semver version compare, reqwest HTTP, temp dir storage.
Allowed download hosts: `github.com`, `objects.githubusercontent.com`.

## Missing
- No `auto_check_updates` config field
- No startup update check
- No UI toggle for auto-check
