# Security

## Audio Path Validation (`src/utils/commands.rs` → `validate_audio_path()`)
Rejects: empty, null bytes. Canonicalizes (resolves symlinks/`../`). Verifies is file. Validates extension: mp3, wav, ogg, flac, m4a, aac, opus.

## Sound File Path Validation (`src/gui/mod.rs` → `validate_path_within()`)
No path separators/traversal in filename. Canonicalizes parent. Constructs safe path from canonical parent + validated filename.

## IPC Security
- 10MB buffer limit on socket reads
- Response size validation on client
- Filename sanitization for download paths

## Rust Safety
- RwLock poisoning handled gracefully (no panics)
- Config save errors logged, not silent

## Review (2026-01-10 — Production Ready)
Audio path validation (extension whitelist, canonicalization), traversal prevention, IPC buffer limits, response size validation, filename sanitization, null byte rejection, symlink resolution ✓
RwLock poisoning handled, no panic-prone locks, error propagation, async/channel correct ✓
91/91 tests, clippy clean ✓
