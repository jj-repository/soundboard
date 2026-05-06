# Overview

v1.10 — Windows Rust soundboard. Single binary; the audio engine ("daemon") runs in-process with the GUI, the CLI is a subcommand of the same exe.

## Modes (selected by argv)
- `soundboard.exe` → GUI + embedded engine
- `soundboard.exe cli ...` → CLI client (talks to running engine via TCP localhost:19735)
- `soundboard.exe --headless` → engine only, no UI

## Files
- `src/main.rs` — argv dispatch + GUI entry
- `src/lib.rs` — library root, `MutexExt` trait
- `src/utils/daemon_run.rs` — embedded engine (`run_headless`, `start_in_process`)
- `src/utils/cli_run.rs` — CLI subcommand
- `src/utils/daemon.rs` — shared daemon helpers (audio player init, IPC client `make_request`)
- `src/utils/commands.rs` — command parsing + path validation
- `src/utils/config.rs` — config path helper
- `src/utils/updater.rs` — update logic
- `src/gui/mod.rs` — GUI state/logic
- `src/gui/draw.rs` — egui rendering
- `src/gui/tray.rs` — Windows tray (tray-icon + muda)
- `src/types/audio_player.rs` — playback engine, mic passthrough, layers
- `src/types/commands.rs` — `DaemonCommand` enum + dispatch
- `src/types/socket.rs` — IPC `Request`/`Response` types
- `src/types/gui.rs` — `UpdateStatus`, GUI state
- `src/types/config.rs` — config schemas
- `assets/` — icon, screenshot
