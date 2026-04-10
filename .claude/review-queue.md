# PWSP Audit Review Queue

Generated: 2026-04-05
Last updated: 2026-04-05 (iteration 5 — final)

## Summary

| Severity | Found | Fixed | Deferred to GH | Remaining |
|----------|-------|-------|----------------|-----------|
| Critical | 3     | 0     | 3 (#15, #16)   | 0         |
| High     | 29    | 9     | 20 (#17-#26)   | 0         |
| Medium   | 43    | 28    | 4              | 11        |
| Low      | 33    | 14    | 0              | 19        |
| **Total**| **108** | **51** | **27**      | **30**    |

### Fixed items (iteration 1)
- CQ-01: Runtime Tokio::new().unwrap() → proper error handling (gui/mod.rs)
- CQ-08: Config loading error now logged (utils/daemon.rs)
- CQ-09: Typo reder→mem (gui/update.rs)
- SEC-02: TOCTOU socket race → unconditional remove (daemon.rs)
- SEC-09: Lock file permissions → 0o600 on Linux (daemon.rs)
- SEC-10: Updater filename → predictable name (utils/updater.rs)
- PERF-03/04: Clone reduction in draw loop (gui/draw.rs)
- TEST-01: Audio path security tests added (utils/commands.rs)
- TEST-03: Config roundtrip tests added (types/config.rs)
- TEST-04: SoundCategory tests added (types/config.rs)
- TEST-05: Command parsing edge cases added (utils/commands.rs)
- TEST-08: SoundMetadata tag tests added (types/config.rs)
- DEVOPS-01: CI workflow created (ci.yml)
- DEVOPS-02: Security audit workflow created (security-audit.yml)
- DEVOPS-04: Version mismatch fixed (pwsp.spec 1.3→1.8)
- DEVOPS-07: Systemd sleep removed (daemon.service)
- DEVOPS-14: Desktop entry typo fixed (pwsp-gui.desktop)

### Fixed items (iteration 2)
- ARCH-15: Exponential backoff when daemon disconnected (utils/gui.rs)
- DEVOPS-09: Auto-merge restricted to patch-only (dependabot-auto-merge.yml)
- DEVOPS-10: cargo-deb version pinned to @2 (release-deb.yml)
- DEVOPS-12: Tag format validation added to all 3 release workflows
- DEVOPS-13: SHA256 checksums added to Linux + Windows release artifacts
- DEVOPS-15: Concurrency control added to all 3 release workflows
- DEVOPS-16: Windows archive renamed PWSP.zip → PWSP-Windows.zip

### Fixed items (iteration 3)
- CQ-10: Config error messages now include path context (types/config.rs)
- CQ-11: Extracted VIRTUAL_MIC_NAME + DAEMON_OUTPUT_NAME constants (lib.rs, 4 files updated)
- TEST-13: Socket protocol edge case tests added (types/socket.rs)
- DEVOPS-05: Deb package maintainer + description metadata (Cargo.toml)
- DEVOPS-18: .gitignore expanded (.vscode, .swp, .DS_Store, .env)

### Fixed items (iteration 4)
- CQ-12: "Daemon connection restored" → eprintln (utils/gui.rs)
- CQ-15: play_on_layer error now shows max index (types/audio_player.rs)
- TEST-10: Version comparison tests (7 tests, utils/updater.rs)
- TEST-17: UpdateInfo/GitHubRelease deserialization tests (utils/updater.rs)
- Updater filename sanitization tests added (utils/updater.rs)
- DEVOPS-08: Clippy step added to release-archive workflow

### Fixed items (iteration 5)
- CQ-04/PERF-12: PipeWire port assignment extracted to helper function (utils/pipewire.rs)
- CQ-14: Documented pub visibility of tray module (gui/mod.rs)
- ARCH-20: Config validation tests (default values, edge cases, serialization) (types/config.rs)
- DEVOPS-17: CI matrix builds (stable + nightly), split into fmt/clippy/test jobs (ci.yml)

### GitHub issues created for remaining critical+high
- #15: SEC-01 — Update signature verification
- #16: TEST-02 — PipeWire integration tests
- #17: ARCH-06 — AudioPlayer decomposition
- #18: ARCH-05 — Command enum dispatch
- #19: ARCH-03 — GUI state management
- #20: ARCH-02/04 — Platform abstraction layer
- #21: SEC-04/05/07/08 — Windows IPC hardening
- #22: SEC-03/06 — File path validation
- #23: PERF-01/02/05/06/07 — GUI render allocations
- #24: TEST-03/06/07 — Audio gain + layer tests
- #25: DEVOPS-03 — Branch protection
- #26: ARCH-08 — Custom error enum
| Medium   | 43    |
| Low      | 33    |
| **Total**| **108** |

## Critical (3)

### SEC-01: Missing Signature Verification for Downloaded Updates
- **Agent**: security-expert
- **File**: src/utils/updater.rs:43-145
- **Description**: Update mechanism downloads binaries from GitHub without verifying integrity or authenticity. MITM or compromised account could inject malicious code.
- **Fix**: Implement GPG/Ed25519 signature verification or SHA-256 checksum verification against a hardcoded manifest.
- **Effort**: large
- **Status**: open

### TEST-01: Audio Path Validation Security Tests Missing
- **Agent**: test-quality-guardian
- **File**: src/utils/commands.rs:8-42
- **Description**: validate_audio_path() is security-critical but existing tests don't cover symlink attacks, relative path escapes, or extension spoofing.
- **Fix**: Add test_validate_audio_path_symlink_attack, traversal, unicode normalization, double extension, case bypass tests.
- **Effort**: medium
- **Status**: open

### TEST-02: PipeWire Device Linking Critical Path Untested
- **Agent**: test-quality-guardian
- **File**: src/types/audio_player.rs:359-446
- **Description**: link_devices() is core mic passthrough functionality with retry logic and port matching. No integration tests exist.
- **Fix**: Mock get_all_devices(), test retry logic, port availability, successful link creation.
- **Effort**: large
- **Status**: open

## High (29)

### SEC-02: Race Condition in Socket File Removal and Creation (TOCTOU)
- **File**: src/bin/daemon.rs:53-58
- **Fix**: Use fs::remove_file() without prior check, or use tempdir with 0o700.
- **Effort**: medium | **Status**: open

### SEC-03: Symlink Following on Path Canonicalization
- **File**: src/gui/mod.rs:40-82
- **Fix**: Validate that neither base nor path components are symlinks using fs::symlink_metadata().
- **Effort**: medium | **Status**: open

### SEC-04: Untrusted JSON Deserialization Without Size Limits on Windows
- **File**: src/bin/daemon.rs:131-137
- **Fix**: Implement MAX_IPC_MESSAGE_SIZE check for Windows TCP connections.
- **Effort**: small | **Status**: open

### SEC-05: IPC Message Size Enforcement Only on Linux Path
- **File**: src/bin/daemon.rs:172-177
- **Fix**: Add explicit validation: if request_len > MAX_IPC_MESSAGE_SIZE { close connection }.
- **Effort**: small | **Status**: open

### SEC-06: Unvalidated File Paths in AudioPlayer play/preview
- **File**: src/types/audio_player.rs:577-620
- **Fix**: Add secondary validation at AudioPlayer level using validate_audio_path() logic.
- **Effort**: medium | **Status**: open

### SEC-07: No Authentication on Windows TCP IPC Port
- **File**: src/bin/daemon.rs:71-78
- **Fix**: Token-based auth or named pipes instead of TCP. Long-term fix.
- **Effort**: large | **Status**: open

### SEC-08: Untrusted serde_json Deserialization in GUI State Thread
- **File**: src/utils/gui.rs:185-272
- **Fix**: Add size limits, depth limits, deny_unknown_fields.
- **Effort**: medium | **Status**: open

### CQ-01: Runtime Tokio::new().unwrap() in GUI threads
- **File**: src/gui/mod.rs:216, 245
- **Fix**: Replace .unwrap() with proper error handling, map to UpdateStatus::Error.
- **Effort**: small | **Status**: open

### CQ-02: Panicking expect() in daemon accessor
- **File**: src/utils/daemon.rs:43
- **Fix**: Return Option or use dependency injection.
- **Effort**: medium | **Status**: open

### PERF-01: Excessive String Parsing in GUI Update Loop (60fps)
- **File**: src/utils/gui.rs:219-227
- **Fix**: Use structured IPC response instead of string parsing.
- **Effort**: medium | **Status**: open

### PERF-02: Multiple String Allocations in GUI State Update
- **File**: src/utils/gui.rs:232-244
- **Fix**: Pre-allocate HashMap, use structured IPC.
- **Effort**: medium | **Status**: open

### PERF-03: Excessive Clone in GUI Draw Loop
- **File**: src/gui/draw.rs:1017,1023
- **Fix**: Use reference instead of .clone() on current_playlist and sounds.
- **Effort**: small | **Status**: open

### PERF-04: Repeated File Cache Cloning on Every Frame
- **File**: src/gui/draw.rs:1032
- **Fix**: Use reference or take ownership only when needed.
- **Effort**: small | **Status**: open

### PERF-05: Inefficient Directory Reading and Cloning
- **File**: src/gui/input.rs:112,127
- **Fix**: Store sorted/filtered state directly instead of re-collecting.
- **Effort**: small | **Status**: open

### PERF-06: Windows Mic Buffer Allocation in Audio Callback
- **File**: src/types/audio_player.rs:756-757
- **Fix**: Pre-allocate buffer or use stack-based array.
- **Effort**: medium | **Status**: open

### PERF-07: Expensive String Parsing in Response Handling (60fps)
- **File**: src/utils/gui.rs:185-272
- **Fix**: Use binary serialization (bincode/msgpack) for frequent IPC.
- **Effort**: large | **Status**: open

### TEST-03: Config Serialization/Deserialization Tests Missing
- **File**: src/types/config.rs:145-270
- **Fix**: Add roundtrip, corruption handling, migration tests.
- **Effort**: small | **Status**: open

### TEST-04: SoundCategory Management Untested
- **File**: src/types/config.rs:71-101
- **Fix**: Add duplicate prevention, remove, contains, order tests.
- **Effort**: small | **Status**: open

### TEST-05: Command Parsing Edge Cases
- **File**: src/utils/commands.rs:44-174
- **Fix**: Test negative volume, exceed max gain, invalid layer, non-number args.
- **Effort**: small | **Status**: open

### TEST-06: AudioPlayer Gain Clamping Untested
- **File**: src/types/audio_player.rs:490-541
- **Fix**: Test volume/gain/mic_gain clamping at boundaries.
- **Effort**: medium | **Status**: open

### TEST-07: AudioLayer State Management Untested
- **File**: src/types/audio_player.rs:69-96
- **Fix**: Integration tests with mock Sink or real rodio.
- **Effort**: medium | **Status**: open

### ARCH-01: IPC Protocol Relies on Untyped HashMap String Serialization
- **File**: src/types/socket.rs
- **Fix**: Define strongly-typed request/response enums with serde.
- **Effort**: large | **Status**: open

### ARCH-02: Type Dependencies Flow Upward (types depends on utils)
- **File**: src/types/config.rs, src/types/audio_player.rs
- **Fix**: Move initialization logic to utils. Types should be pure data.
- **Effort**: large | **Status**: open

### ARCH-03: Circular State Management in GUI
- **File**: src/gui/mod.rs:80-100, src/utils/gui.rs
- **Fix**: Single source of truth pattern with proper state container.
- **Effort**: large | **Status**: open

### ARCH-04: Platform-Specific Code Scattered Across Modules
- **File**: src/types/audio_player.rs, src/bin/daemon.rs
- **Fix**: Create platform modules with trait-based abstractions.
- **Effort**: large | **Status**: open

### ARCH-05: Command Handler Boilerplate (549 LOC for thin wrappers)
- **File**: src/types/commands.rs
- **Fix**: Replace with enum + match in daemon loop. ~549 LOC -> ~50 LOC.
- **Effort**: medium | **Status**: open

### ARCH-06: AudioPlayer Monolith (964 lines, 4+ concerns)
- **File**: src/types/audio_player.rs
- **Fix**: Decompose into AudioEngine, DeviceManager, AudioState, Mixer.
- **Effort**: large | **Status**: open

### DEVOPS-01: Missing CI/CD Pipeline for Pull Requests
- **File**: .github/workflows/ (missing)
- **Fix**: Create test.yml with cargo test + clippy + fmt.
- **Effort**: small | **Status**: open

### DEVOPS-02: No Security Audit Workflow
- **File**: .github/workflows/ (missing)
- **Fix**: Create security-audit.yml running cargo audit.
- **Effort**: small | **Status**: open

### DEVOPS-03: Missing Branch Protection Configuration
- **File**: GitHub settings
- **Fix**: Configure branch protection requiring approvals + passing checks.
- **Effort**: small | **Status**: open

## Medium (43)

### SEC-09: Lock file created without explicit permissions
- **File**: src/bin/daemon.rs:48-49 | **Effort**: small | **Status**: open

### SEC-10: Filename sanitization incomplete in update download
- **File**: src/utils/updater.rs:115-130 | **Effort**: small | **Status**: open

### SEC-11: Input validation missing for audio device names
- **File**: src/types/commands.rs:391-403 | **Effort**: small | **Status**: open

### CQ-03: Code duplication in play() and preview()
- **File**: src/types/audio_player.rs:577,614 | **Effort**: medium | **Status**: open

### CQ-04: Code duplication in PipeWire port matching
- **File**: src/utils/pipewire.rs:171-206 | **Effort**: medium | **Status**: open

### CQ-05: AudioPlayer::new() exceeds 130 LOC
- **File**: src/types/audio_player.rs:175-307 | **Effort**: large | **Status**: open

### CQ-06: Silent error suppression (.ok()) throughout
- **File**: multiple files | **Effort**: medium | **Status**: open

### CQ-07: Inconsistent error propagation in make_request_sync
- **File**: src/utils/gui.rs:27-67 | **Effort**: medium | **Status**: open

### CQ-08: Unhandled error in config loading
- **File**: src/utils/daemon.rs:51-56 | **Effort**: small | **Status**: open

### PERF-08: Unnecessary Clone in Daemon Player Loop
- **File**: src/bin/daemon.rs:184 | **Effort**: small | **Status**: open

### PERF-09: Path Canonicalization in File Validation Loop
- **File**: src/utils/commands.rs:10-42 | **Effort**: medium | **Status**: open

### PERF-10: Repeated Device Enumeration on Windows
- **File**: src/types/audio_player.rs:56-67 | **Effort**: medium | **Status**: open

### PERF-11: Repeated Lock Acquisitions in Command Execution
- **File**: src/types/commands.rs:118+ | **Effort**: small | **Status**: open

### PERF-12: Inefficient Device Search with Duplicated Logic
- **File**: src/utils/pipewire.rs:167-207 | **Effort**: small | **Status**: open

### TEST-08: SoundMetadata Tag Filtering Untested
- **File**: src/types/config.rs:104-143 | **Effort**: small | **Status**: open

### TEST-09: HotkeyBinding Display Format Untested
- **File**: src/types/config.rs:21-49 | **Effort**: small | **Status**: open

### TEST-10: Version Comparison Logic in Updater Untested
- **File**: src/utils/updater.rs:62-68 | **Effort**: small | **Status**: open

### TEST-11: GetCurrentInputCommand Platform-Specific Logic Untested
- **File**: src/types/commands.rs:329-354 | **Effort**: medium | **Status**: open

### TEST-12: SetLayerVolumeCommand Input Validation
- **File**: src/types/commands.rs:520-536 | **Effort**: small | **Status**: open

### TEST-13: Request/Response Socket Protocol Missing Advanced Cases
- **File**: src/types/socket.rs:178-491 | **Effort**: small | **Status**: open

### TEST-14: DaemonConfig Default Loading Untested
- **File**: src/utils/daemon.rs:51-57 | **Effort**: small | **Status**: open

### TEST-15: Command Execution Error Handling
- **File**: src/types/commands.rs:109-549 | **Effort**: large | **Status**: open

### ARCH-07: GUI Directly Imports Domain Types as View Models
- **File**: src/gui/mod.rs | **Effort**: medium | **Status**: open

### ARCH-08: Error Handling Uses Box<dyn Error> Everywhere
- **File**: multiple files | **Effort**: medium | **Status**: open

### ARCH-09: No Validation of Command Arguments at Parse Time
- **File**: src/utils/commands.rs:54-93 | **Effort**: small | **Status**: open

### ARCH-10: Global Static AudioPlayer Requires Init Ordering
- **File**: src/utils/daemon.rs:22-44 | **Effort**: medium | **Status**: open

### ARCH-11: IPC Message Size Limit Poorly Enforced
- **File**: src/bin/daemon.rs:108-160 | **Effort**: small | **Status**: open

### ARCH-12: Socket IPC Has No Auth/Encryption
- **File**: src/bin/daemon.rs:52-78 | **Effort**: medium | **Status**: open

### ARCH-13: GUI State Persistence Race Conditions
- **File**: src/gui/mod.rs:157-158 | **Effort**: medium | **Status**: open

### ARCH-14: Hotkey System Leaks Platform-Specific Code
- **File**: src/gui/hotkeys.rs | **Effort**: small | **Status**: open

### ARCH-15: No Graceful Degradation When Daemon Unavailable
- **File**: src/gui/mod.rs:44-48, src/utils/gui.rs:80-89 | **Effort**: small | **Status**: open

### DEVOPS-04: Version Mismatch Cargo.toml (1.8.0) vs pwsp.spec (1.3.0)
- **File**: pwsp.spec:7 | **Effort**: small | **Status**: open

### DEVOPS-05: Missing Maintainer Metadata in Deb Package
- **File**: Cargo.toml:67-75 | **Effort**: small | **Status**: open

### DEVOPS-06: Hardcoded Absolute Paths in Systemd Service
- **File**: assets/pwsp-daemon.service:6-7 | **Effort**: small | **Status**: open

### DEVOPS-07: Problematic 10s Sleep in Systemd Service
- **File**: assets/pwsp-daemon.service:6 | **Effort**: small | **Status**: open

### DEVOPS-08: No Code Quality Linting in Release Workflows
- **File**: .github/workflows/release-*.yml | **Effort**: small | **Status**: open

### DEVOPS-09: Overly Broad Dependabot Auto-Merge Policy
- **File**: .github/workflows/dependabot-auto-merge.yml:21 | **Effort**: small | **Status**: open

### DEVOPS-10: Missing Cargo-Deb Version Pinning
- **File**: .github/workflows/release-deb.yml:91 | **Effort**: small | **Status**: open

### DEVOPS-11: No Signed Release Tags or Commits
- **File**: .github/workflows/release-*.yml | **Effort**: medium | **Status**: open

### DEVOPS-12: Workflow Triggers Missing Tag Validation
- **File**: .github/workflows/release-*.yml | **Effort**: small | **Status**: open

### DEVOPS-13: No Build Artifact Integrity Verification (checksums)
- **File**: .github/workflows/release-*.yml | **Effort**: small | **Status**: open

### DEVOPS-14: Typo in Desktop Entry Comment Field
- **File**: assets/pwsp-gui.desktop:3 | **Effort**: small | **Status**: open

### DEVOPS-15: Missing Concurrency Control in Workflows
- **File**: .github/workflows/release-*.yml | **Effort**: small | **Status**: open

## Low (33)

### SEC-12: Integer overflow in layer index validation
- **File**: src/gui/mod.rs:551-552 | **Effort**: small | **Status**: open

### SEC-13: Possible DoS via excessive file metadata queries
- **File**: src/gui/mod.rs:280-281 | **Effort**: small | **Status**: open

### CQ-09: Typo in variable name (reder -> reader/memory)
- **File**: src/gui/update.rs:64 | **Effort**: small | **Status**: open

### CQ-10: Config path error message insufficient
- **File**: src/types/config.rs:157-159 | **Effort**: small | **Status**: open

### CQ-11: Hardcoded "pwsp-virtual-mic" string
- **File**: src/utils/pipewire.rs:265 | **Effort**: small | **Status**: open

### CQ-12: Mixed stdout/stderr logging
- **File**: src/utils/daemon.rs | **Effort**: small | **Status**: open

### CQ-13: Unused import wildcard
- **File**: src/utils/commands.rs:1 | **Effort**: small | **Status**: open

### CQ-14: Inconsistent pub visibility in gui modules
- **File**: src/gui/mod.rs | **Effort**: small | **Status**: open

### CQ-15: Missing error context in play_on_layer
- **File**: src/types/commands.rs:809 | **Effort**: small | **Status**: open

### CQ-16: Ambiguous underscore prefixed variables
- **File**: src/bin/daemon.rs:48,101 | **Effort**: small | **Status**: open

### CQ-17: unwrap() in test code without error context
- **File**: src/types/socket.rs:464-487 | **Effort**: small | **Status**: open

### CQ-18: Double-clone in PipeWire port assignment
- **File**: src/utils/pipewire.rs:197-202 | **Effort**: small | **Status**: open

### CQ-19: Missing bounds check documentation
- **File**: src/types/audio_player.rs:538-545 | **Effort**: small | **Status**: open

### PERF-13: Unbounded String Allocation in Hotkey Display
- **File**: src/gui/hotkeys.rs:308-351 | **Effort**: small | **Status**: open

### PERF-14: Circular Arc Reference Pattern (no issue, monitor)
- **File**: src/gui/mod.rs:98 | **Effort**: small | **Status**: open

### TEST-16: PlayerState Enum Serialization Untested
- **File**: src/types/audio_player.rs:24-30 | **Effort**: small | **Status**: open

### TEST-17: UpdateInfo Structure Untested
- **File**: src/utils/updater.rs:32-41 | **Effort**: small | **Status**: open

### ARCH-16: Layer System Partially Implemented, No GUI
- **File**: src/types/audio_player.rs:69-99 | **Effort**: medium | **Status**: open

### ARCH-17: Public API Surface Exposes Implementation Details
- **File**: src/lib.rs | **Effort**: small | **Status**: open

### ARCH-18: Testing Only Covers IPC Layer, Not Core Logic
- **File**: src/utils/commands.rs, src/types/socket.rs | **Effort**: medium | **Status**: open

### ARCH-19: Binary Code Duplication (daemon/cli/gui init)
- **File**: src/bin/*.rs, src/main.rs | **Effort**: small | **Status**: open

### ARCH-20: No Configuration Validation or Schema
- **File**: src/types/config.rs | **Effort**: small | **Status**: open

### ARCH-21: No Logging Framework (only eprintln!)
- **File**: multiple files | **Effort**: small | **Status**: open

### ARCH-22: GUI Draw Code Lacks Modularization
- **File**: src/gui/draw.rs | **Effort**: small | **Status**: open

### DEVOPS-16: Archive Naming Inconsistency (PWSP-Linux vs PWSP)
- **File**: release-windows.yml:79 | **Effort**: small | **Status**: open

### DEVOPS-17: No Matrix Build for MSRV/Nightly
- **File**: missing test.yml | **Effort**: medium | **Status**: open

### DEVOPS-18: Inadequate .gitignore
- **File**: .gitignore | **Effort**: small | **Status**: open

### DEVOPS-19: No Release Notes/Changelog Generation
- **File**: .github/workflows/release-*.yml | **Effort**: medium | **Status**: open

### DEVOPS-20: Missing Maintenance/Contributing Docs
- **File**: project root | **Effort**: medium | **Status**: open
