# Decisions & Standards

## Design Decisions
| Decision | Rationale |
|----------|-----------|
| Single binary, embedded engine | Audio engine had no reason to live in a separate process on Windows; one exe is simpler to ship and run |
| TCP localhost:19735 IPC kept in-process | Lets the same code path serve both GUI and `cli` subcommand; avoids rewriting ~50 GUI sites that use `make_request_sync` |
| Argv-dispatched modes (`cli`, `--headless`) | Single binary supports CLI scripting + headless autostart without shipping extra exes |
| egui | Pure Rust, easy builds, sufficient UI |
| Windows-only | VB-Audio Virtual Cable available on Windows; macOS has no equivalent free/easy path; Linux fork lives at upstream |
| Manual update check | Auto-update for binaries is complex |

## Won't Fix
| Issue | Reason |
|-------|--------|
| No auto_check_updates | Complexity vs benefit; manual check fine |
| No startup update check | Same |
| Binary updates need manual install | Safe; avoids self-modification |
| No macOS / Linux | Out of scope after Windows-focus refactor; for Linux see upstream `arabianq/pipewire-soundpad` |

## Known Issues
1. No `auto_check_updates` config/toggle
2. No startup update check
3. Binary updates: download only, manual install
4. Engine crash kills GUI (single process). Mitigated by tokio task panic isolation.

## Quality Standards
Target: reliable audio playback, responsive UI.
Do not optimize: audio latency (rodio), UI (egui immediate mode).

## Completed
RwLock poisoning recovery, response size validation, filename sanitization, async patterns, IPC security hardening, Windows mic passthrough (CPAL→mixer→VB-Audio), mic gain control, input device enum/selection, single-binary refactor (drop daemon/cli bin entries, embed engine in GUI process).
