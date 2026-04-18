# Overview

v1.09 — Cross-platform Rust soundboard that routes audio through the microphone. Daemon architecture with GUI and CLI clients.

## Files
- `src/main.rs` — GUI entry point
- `src/bin/daemon.rs` — background daemon
- `src/bin/cli.rs` — CLI client
- `src/gui/mod.rs` — GUI state/logic
- `src/gui/draw.rs` — egui rendering
- `src/types/gui.rs` — UpdateStatus enum
- `src/types/commands.rs` — command definitions
- `src/types/socket.rs` — IPC types
- `src/utils/commands.rs` — command parsing + path validation
- `src/utils/config.rs` — config path helper
- `src/utils/updater.rs` — update logic
- `assets/` — desktop files, icons, systemd service
