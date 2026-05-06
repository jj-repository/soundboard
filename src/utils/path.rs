//! Filesystem path validation helpers. Each entry point that accepts caller-
//! supplied paths must validate before any privileged filesystem operation;
//! these helpers provide defense-in-depth against symlink escape, TOCTOU
//! between validation and open, and NTFS Alternate Data Stream selectors.

use std::path::{Path, PathBuf};

/// Audio extensions accepted by the player. Lowercase, no leading dot.
pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] =
    &["mp3", "wav", "ogg", "flac", "m4a", "aac", "opus"];

/// Case-insensitive check that `path` ends in a supported audio extension.
pub fn has_supported_audio_extension(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let lower = ext.to_ascii_lowercase();
            SUPPORTED_AUDIO_EXTENSIONS.contains(&lower.as_str())
        }
        None => false,
    }
}

/// Validate a user-supplied audio path. Returns the canonicalized path if:
/// - non-empty, no NUL bytes
/// - resolves to a real regular file (final component is not a symlink)
/// - no NTFS Alternate Data Stream selector on Windows
/// - extension is in `SUPPORTED_AUDIO_EXTENSIONS`
pub fn validate_audio_path(path_str: &str) -> Option<PathBuf> {
    if path_str.is_empty() || path_str.contains('\0') {
        return None;
    }
    let input = PathBuf::from(path_str);

    // canonicalize() resolves symlinks; running symlink_metadata on its
    // result therefore can't see them. We must walk the supplied path with
    // symlink_metadata to reject symlinks anywhere in the input.
    let mut walk = PathBuf::new();
    for comp in input.components() {
        walk.push(comp);
        let Ok(meta) = std::fs::symlink_metadata(&walk) else {
            return None;
        };
        if meta.file_type().is_symlink() {
            return None;
        }
    }

    let canonical = input.canonicalize().ok()?;

    let meta = std::fs::symlink_metadata(&canonical).ok()?;
    if !meta.is_file() {
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        // NTFS ADS syntax (`filename:streamname`) lives in the leaf
        // component. Inspect only the file name; the rest of the canonical
        // path on Windows can include drive-letter colons (`C:`) and the
        // verbatim `\\?\C:\` prefix returned by `canonicalize`, which would
        // otherwise produce false positives.
        let leaf = canonical.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if leaf.contains(':') {
            return None;
        }
    }

    has_supported_audio_extension(&canonical).then_some(canonical)
}

/// Validate that `path` is contained within `base_dir`, with no symlinks
/// anywhere along the supplied path. Returns the canonicalized path on
/// success, None on rejection.
///
/// Walking the input's components with `symlink_metadata` (instead of trusting
/// the canonicalized result alone) defends against a TOCTOU between
/// validation and a later open/unlink: even if the path canonicalizes inside
/// `base_dir`, a symlink anywhere in the supplied components would be unsafe
/// to operate on.
pub fn validate_path_within(path: &Path, base_dir: &Path) -> Option<PathBuf> {
    let canonical_base = base_dir.canonicalize().ok()?;

    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // Path doesn't exist yet: validate the (existing) parent and the
            // requested filename separately, mirroring the original behavior
            // for "create new file in safe dir" callers.
            let parent = path.parent()?;
            let canonical_parent = parent.canonicalize().ok()?;
            if !canonical_parent.starts_with(&canonical_base) {
                return None;
            }
            let file_name = path.file_name()?;
            let file_name_str = file_name.to_str()?;
            if file_name_str.contains('/')
                || file_name_str.contains('\\')
                || file_name_str.contains('\0')
                || file_name_str == ".."
                || file_name_str == "."
            {
                return None;
            }
            let safe_path = canonical_parent.join(file_name);
            return safe_path.starts_with(&canonical_base).then_some(safe_path);
        }
    };

    if !canonical.starts_with(&canonical_base) {
        return None;
    }

    // Reject symlinks at any level of the supplied path. Components that
    // don't yet exist are skipped (the parent-only branch above handles
    // create-new cases).
    let mut current = PathBuf::new();
    for comp in path.components() {
        current.push(comp);
        let Ok(meta) = std::fs::symlink_metadata(&current) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            return None;
        }
    }

    Some(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn audio_ext_accepts_supported() {
        assert!(has_supported_audio_extension(Path::new("foo.mp3")));
        assert!(has_supported_audio_extension(Path::new("foo.MP3")));
        assert!(has_supported_audio_extension(Path::new("a/b/c.flac")));
    }

    #[test]
    fn audio_ext_rejects_unsupported() {
        assert!(!has_supported_audio_extension(Path::new("foo.exe")));
        assert!(!has_supported_audio_extension(Path::new("foo")));
        assert!(!has_supported_audio_extension(Path::new("foo.")));
    }

    #[test]
    fn validate_audio_path_rejects_empty_and_nul() {
        assert!(validate_audio_path("").is_none());
        assert!(validate_audio_path("a\0b.mp3").is_none());
    }

    #[test]
    fn validate_audio_path_accepts_real_audio_file() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("good.mp3");
        fs::write(&p, b"fake").unwrap();
        let got = validate_audio_path(p.to_str().unwrap());
        assert!(got.is_some(), "{p:?} should validate");
    }

    #[test]
    fn validate_audio_path_rejects_directory() {
        let dir = tempdir().unwrap();
        assert!(validate_audio_path(dir.path().to_str().unwrap()).is_none());
    }

    #[test]
    fn validate_audio_path_rejects_unsupported_ext() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("bad.exe");
        fs::write(&p, b"x").unwrap();
        assert!(validate_audio_path(p.to_str().unwrap()).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn validate_audio_path_rejects_symlink_to_audio_file() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real.mp3");
        fs::write(&real, b"x").unwrap();
        let link = dir.path().join("link.mp3");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(
            validate_audio_path(link.to_str().unwrap()).is_none(),
            "symlink to audio file should be rejected"
        );
    }

    #[cfg(unix)]
    #[test]
    fn validate_path_within_rejects_symlink_into_base() {
        let dir = tempdir().unwrap();
        let outside = dir.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let outside_file = outside.join("secret.mp3");
        fs::write(&outside_file, b"secret").unwrap();

        let base = dir.path().join("safe");
        fs::create_dir(&base).unwrap();
        let link = base.join("link.mp3");
        std::os::unix::fs::symlink(&outside_file, &link).unwrap();

        // The symlink resolves outside the base, so canonicalize-based logic
        // already rejects it; this test pins that contract.
        assert!(
            validate_path_within(&link, &base).is_none(),
            "symlink escaping base must be rejected"
        );
    }

    #[cfg(unix)]
    #[test]
    fn validate_path_within_rejects_symlink_within_base() {
        let dir = tempdir().unwrap();
        let base = dir.path().join("safe");
        fs::create_dir(&base).unwrap();
        let real = base.join("real.mp3");
        fs::write(&real, b"x").unwrap();
        let link = base.join("link.mp3");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        // Even though the symlink target is inside base, reject because we
        // refuse to follow symlinks at all (TOCTOU defense).
        assert!(
            validate_path_within(&link, &base).is_none(),
            "symlink within base must be rejected"
        );
    }

    #[test]
    fn validate_path_within_accepts_real_file_in_base() {
        let dir = tempdir().unwrap();
        let base = dir.path().join("safe");
        fs::create_dir(&base).unwrap();
        let real = base.join("real.mp3");
        fs::write(&real, b"x").unwrap();
        assert!(validate_path_within(&real, &base).is_some());
    }
}
