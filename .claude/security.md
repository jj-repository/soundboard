# Security

## Audio Path Validation (`src/utils/path.rs` → `validate_audio_path()`)
Rejects: empty, NUL bytes, paths whose components include a symlink, paths whose canonicalized leaf contains a `:` (NTFS Alternate Data Stream selector). Canonicalizes after symlink check, verifies the leaf is a regular file, validates extension against the whitelist (mp3, wav, ogg, flac, m4a, aac, opus).

## Sound File Path Confinement (`src/utils/path.rs` → `validate_path_within()`)
Rejects symlinks anywhere in the input. Returns the canonical path only when it is contained within the supplied base directory; for create-new flows, validates the parent dir is inside the base and that the file name is free of `\`, `/`, NUL, `.` / `..`.

## IPC Security
- Server: 64 KiB max request size, 5s read timeout, 32-arg cap (`utils/daemon_run.rs`).
- Client: 1 MiB max response size (`utils/daemon.rs::send_and_receive`).
- `Request` is `#[serde(deny_unknown_fields)]`.
- TCP listener binds `127.0.0.1` only — no external exposure.

## Rust Safety
- Mutex poisoning is recovered in-place (`MutexExt::lock_or_recover`). No panic on poison.
- Config save errors are logged, not silent.

## Review (2026-01-10)
Audio path validation, traversal prevention, IPC buffer limits, response size validation, filename sanitization, NUL rejection, symlink resolution ✓
RwLock poisoning handled, no panic-prone locks, error propagation, async/channel correct ✓
148/148 tests, clippy clean ✓
