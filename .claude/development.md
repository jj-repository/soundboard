# Development

## Commands
```bash
cargo build --release
cargo run --release --bin soundboard-gui
cargo run --release --bin soundboard-daemon
cargo run --release --bin soundboard-cli -- <command>
cargo test
```

## Dependencies
```toml
tokio = "1.48.0"       # full
egui/eframe = "0.33.3"
rodio = "0.21.1"       # audio playback
pipewire = "0.9.2"     # PipeWire bindings
reqwest = "0.12"
semver = "1.0"
global-hotkey = "0.6"
ksni = "0.3"           # tray Linux
tray-icon + muda       # tray Windows
```

## Tests (91 total)
- `src/utils/commands.rs` — path validation, command parsing
- `src/utils/config.rs` — config path

## Common Tasks

**Adding auto_check_updates:**
1. Add field to GUI config struct
2. Checkbox in `src/gui/draw.rs` settings section
3. Check on startup in `src/gui/mod.rs`
4. Persist to config

**Modifying update UI:**
- Display: `src/gui/draw.rs` (search "Updates" heading)
- Status enum: `src/types/gui.rs`
- Logic: `src/utils/updater.rs`

## Platform Notes
**Linux:** PipeWire routing, systemd service in `assets/`, ksni tray, Unix socket
**Windows:** VB-Audio Virtual Cable; CPAL captures real mic → mixes with soundboard → VB-Audio output; mic gain via passthrough sink volume; input device enum/selection via CPAL; tray-icon+muda; TCP IPC
