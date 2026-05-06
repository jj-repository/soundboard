# Architecture

## Single-Binary Design (Windows-only)
```
soundboard.exe   ← single executable
├─ default mode  : GUI + embedded engine (one OS process)
├─ "cli" mode    : TCP client → talks to whichever instance is running
└─ "--headless"  : engine only (no UI)

GUI ⇄ engine    : TCP localhost:19735, JSON-over-length-prefix
CLI ⇄ engine    : same TCP loop
```

The GUI calls `daemon_run::start_in_process()` at boot; that acquires the cross-process advisory lock (`%LOCALAPPDATA%\soundboard\run\daemon.lock`), inits `AudioPlayer`, and spawns the IPC listener + player loop as `tokio::spawn` tasks. Lock file is held in a `OnceLock` for the lifetime of the process. A second `soundboard.exe` finds the lock taken and the GUI's `start_in_process` returns an error (logged as warn) — the new GUI then talks to the existing engine, same as the CLI does.

## Engine Components
- **Audio routing**: CPAL captures the selected mic → mixed with rodio playback → routed to the VB-Audio Virtual Cable output device.
- **Mic passthrough**: lock-free SPSC ring buffer (`ringbuf`) pushes captured samples from the cpal callback to a custom rodio `Source` on the output mixer thread; mic gain is the dedicated mic sink volume.
- **Output device autodetect**: at startup, scans CPAL output devices for "cable input" / "vb-audio" if no override is configured.
- **Layers**: 4 independent `Player` sinks for parallel playback.

## GUI
- egui/eframe single-window app.
- Configurable hotkeys via `global-hotkey`.
- Settings panel includes update management.
- Connects to the engine over TCP — same code path the CLI uses.

## CLI
- `clap`-derived subcommand. Re-parses argv with the leading `cli` token stripped.
- Calls `make_request` against `127.0.0.1:19735`; fails fast if no engine is running.

## Async Pattern (Tokio + mpsc)
```rust
let (tx, rx) = mpsc::channel();
runtime.spawn(async move {
    tx.send(check_for_updates().await).ok();
});
// Poll in GUI update loop
if let Ok(status) = rx.try_recv() { self.update_status = status; }
```
