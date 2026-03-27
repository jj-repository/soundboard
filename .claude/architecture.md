# Architecture

## Multi-Process Design
```
pwsp-daemon (background)
    ↕ Unix socket (Linux) / TCP localhost:19735 (Windows)
pwsp-gui (desktop client)
pwsp-cli (terminal client)
```

## Daemon
- PipeWire audio routing + rodio playback
- Listens on Unix socket (Linux) or TCP:19735 (Windows)
- Runs as systemd user service (Linux)

## GUI
- egui/eframe, soundboard with configurable hotkeys, settings panel with update management
- Connects to daemon via socket

## CLI
- Daemon control for scripting/automation

## Async Pattern (Tokio + mpsc)
```rust
let (tx, rx) = mpsc::channel();
runtime.spawn(async move {
    tx.send(check_for_updates().await).ok();
});
// Poll in GUI update loop
if let Ok(status) = rx.try_recv() { self.update_status = status; }
```
