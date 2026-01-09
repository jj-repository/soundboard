# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**PWSP (PipeWire SoundPad)** is a Rust application for playing audio files through your microphone on Linux. It uses PipeWire for audio routing and features both CLI and GUI clients with a daemon architecture.

**Version:** 1.4.0

## Files Structure

```
soundboard/
├── src/
│   ├── main.rs                    # GUI client entry point
│   ├── bin/
│   │   ├── daemon.rs              # Background daemon
│   │   └── cli.rs                 # CLI client
│   ├── gui/
│   │   ├── mod.rs                 # GUI state and logic
│   │   └── draw.rs                # egui rendering
│   ├── types/
│   │   ├── mod.rs                 # Type exports
│   │   ├── gui.rs                 # GUI state types (UpdateStatus enum)
│   │   ├── commands.rs            # Command definitions
│   │   └── socket.rs              # IPC types
│   └── utils/
│       ├── mod.rs                 # Utility exports
│       ├── commands.rs            # Command parsing with path validation
│       ├── config.rs              # Config path helper
│       └── updater.rs             # Update checking and downloading
├── Cargo.toml                     # Dependencies
├── assets/                        # Desktop files, icons, systemd service
└── CLAUDE.md                      # This file
```

## Build and Run Commands

```bash
# Build all binaries
cargo build --release

# Run GUI
cargo run --release --bin pwsp-gui

# Run daemon (background service)
cargo run --release --bin pwsp-daemon

# Run CLI
cargo run --release --bin pwsp-cli -- <command>

# Run tests
cargo test

# Check without building
cargo check
```

## Architecture Overview

### Multi-Process Design

```
pwsp-daemon (background service)
    ↕ Unix Socket IPC
pwsp-gui (desktop client)
pwsp-cli (terminal client)
```

### Daemon
- Manages PipeWire audio routing
- Handles audio playback with rodio
- Listens on Unix socket for commands
- Can run as systemd user service

### GUI Client
- egui/eframe-based desktop application
- Connects to daemon via Unix socket
- Soundboard with configurable hotkeys
- Settings panel with update management

### CLI Client
- Command-line interface for daemon control
- Used for scripting and automation

## Configuration

**Config Path:** `~/.config/pwsp/`

**Helper Function:**
```rust
// src/utils/config.rs
pub fn get_config_path() -> Result<PathBuf, Box<dyn Error>>
```

## Update System

**Status:** Fully implemented with download and progress UI

**Files:**
- `src/utils/updater.rs`: Core update logic
- `src/types/gui.rs`: UpdateStatus enum
- `src/gui/draw.rs`: Settings panel with update UI
- `src/gui/mod.rs`: Update state management

**UpdateStatus Enum:**
```rust
pub enum UpdateStatus {
    NotChecked,
    Checking,
    UpToDate,
    UpdateAvailable { latest_version, release_url, download_url },
    Downloading { progress: f32 },
    Downloaded { file_path },
    Error(String),
}
```

**Features:**
- Version check via GitHub API
- Linux binary detection (.tar.gz, .deb)
- Progress callback for downloads
- Temp directory storage
- Uses semver crate for version comparison

**GitHub Integration:**
- Repository: `jj-repository/soundboard`
- Uses reqwest for HTTP
- Parses release assets for platform-specific binaries

**Missing:**
- No `auto_check_updates` setting
- No automatic check on startup
- No UI toggle for auto-check

## Dependencies (Key)

```toml
tokio = { version = "1.48.0", features = ["full"] }
egui = "0.33.3"
eframe = "0.33.3"
rodio = "0.21.1"          # Audio playback
pipewire = "0.9.2"        # PipeWire bindings
reqwest = "0.12"          # HTTP client
semver = "1.0"            # Version comparison
global-hotkey = "0.6"     # Global keyboard shortcuts
ksni = "0.3"              # System tray
```

## Security Features

### Audio Path Validation
```rust
// src/utils/commands.rs
fn validate_audio_path(path_str: &str) -> Option<PathBuf>
```

**Checks:**
- Rejects empty strings
- Rejects null bytes
- Canonicalizes path (resolves symlinks, ../)
- Verifies is a file (not directory)
- Validates audio extension (mp3, wav, ogg, flac, m4a, aac, opus)

### IPC Security
- Buffer size limits on socket reads
- Input validation on all commands

## Async Pattern

Uses Tokio runtime with mpsc channels for GUI-async communication:

```rust
// Spawn async task
let (tx, rx) = mpsc::channel();
runtime.spawn(async move {
    let result = check_for_updates().await;
    tx.send(result).ok();
});

// Poll in GUI update loop
if let Ok(status) = rx.try_recv() {
    self.update_status = status;
}
```

## Testing

**Test Files:**
- `src/utils/commands.rs` - 9 tests for path validation
- `src/utils/config.rs` - 3 tests for config path

**Running Tests:**
```bash
cargo test
cargo test -- --nocapture  # Show println output
```

## Known Issues / Technical Debt

1. **No auto_check_updates toggle**: Always requires manual check
2. **No startup update check**: User must manually check for updates
3. **Binary updates only**: Downloads release assets, requires manual install
4. **Rust 2024 edition**: Uses latest Rust edition for let chains

## Common Development Tasks

### Adding auto_check_updates setting
1. Add field to GUI config struct
2. Add checkbox in `src/gui/draw.rs` settings section
3. Check setting on startup in `src/gui/mod.rs`
4. Persist setting to config file

### Modifying update UI
- Status display: `src/gui/draw.rs` (search for "Updates" heading)
- Status enum: `src/types/gui.rs` (UpdateStatus)
- Check logic: `src/utils/updater.rs`

### Adding new IPC command
1. Define command struct in `src/types/commands.rs`
2. Implement `Executable` trait
3. Add to `parse_command()` in `src/utils/commands.rs`
4. Add handler in daemon

## Platform Notes

- **Linux only**: Uses PipeWire (Linux audio system)
- **Systemd integration**: Service file in `assets/`
- **System tray**: Uses ksni for Linux tray icon
