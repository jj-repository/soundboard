# Decisions & Standards

## Design Decisions
| Decision | Rationale |
|----------|-----------|
| Daemon architecture | Audio routing needs persistent process; GUI can restart independently |
| Unix socket (Linux) / TCP (Windows) | Simple, fast, secure local communication |
| egui | Pure Rust, easy builds, sufficient UI |
| Cross-platform | PipeWire on Linux, VB-Audio on Windows |
| Manual update check | Auto-update for binaries is complex |

## Won't Fix
| Issue | Reason |
|-------|--------|
| No auto_check_updates | Complexity vs benefit; manual check fine |
| No startup update check | Same |
| Binary updates need manual install | Safe; avoids self-modification |
| No macOS | No virtual audio cable solution |

## Known Issues
1. No `auto_check_updates` config/toggle
2. No startup update check
3. Binary updates: download only, manual install

## Quality Standards
Target: reliable audio playback, responsive UI.
Do not optimize: audio latency (PipeWire/rodio), UI (egui immediate mode).

## Completed
RwLock poisoning recovery, response size validation, filename sanitization, async patterns, IPC security hardening, Windows mic passthrough (CPAL→mixer→VB-Audio), mic gain control, input device enum/selection ✓
