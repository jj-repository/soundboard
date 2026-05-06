# Development

## Commands
```powershell
cargo build --release
cargo run --release                 # GUI + embedded engine
cargo run --release -- cli get volume   # CLI subcommand
cargo run --release -- --headless       # engine only
cargo test
```

## Dependencies
```toml
tokio = "1.51"          # full
egui/eframe = "0.34.1"
rodio = "0.22"          # audio playback
cpal (via rodio)        # mic capture, output device enumeration
ringbuf = "0.4"         # SPSC mic-passthrough ring
reqwest = "0.12"        # update fetcher
semver = "1.0"
global-hotkey = "0.7"
tray-icon + muda        # Windows tray
```

## Tests
- `src/utils/commands.rs` — path validation, command parsing
- `src/utils/path.rs` — symlink/ADS rejection, base-dir confinement
- `src/utils/config.rs` — config path
- `src/types/audio_player.rs` — gain clamping, layer info, serde roundtrip
- `src/types/socket.rs` — IPC roundtrip
- `src/types/config.rs` — config schema

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

## Platform
Windows only. VB-Audio Virtual Cable handles mic routing; CPAL captures real mic → mixes with soundboard playback → VB-Audio output device. Tray via tray-icon + muda. IPC over TCP localhost:19735.
