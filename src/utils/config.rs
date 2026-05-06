use std::path::PathBuf;

use crate::error::{Result, SoundboardError};

pub fn get_config_path() -> Result<PathBuf> {
    let config_path = dirs::config_dir().ok_or(SoundboardError::ConfigDirUnavailable)?;
    Ok(config_path.join("soundboard"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_config_path_returns_result() {
        let result = get_config_path();

        match result {
            Ok(path) => {
                assert!(
                    path.ends_with("soundboard"),
                    "Config path should end with 'soundboard'"
                );
                assert!(
                    path.to_string_lossy().contains("config")
                        || path.to_string_lossy().contains(".config")
                        || path.to_string_lossy().contains("AppData"),
                    "Config path should be in a config directory"
                );
            }
            Err(e) => {
                assert!(
                    matches!(e, SoundboardError::ConfigDirUnavailable),
                    "Expected ConfigDirUnavailable, got {:?}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_get_config_path_is_absolute() {
        if let Ok(path) = get_config_path() {
            assert!(path.is_absolute(), "Config path should be absolute");
        }
    }

    #[test]
    fn test_get_config_path_consistent() {
        if let (Ok(path1), Ok(path2)) = (get_config_path(), get_config_path()) {
            assert_eq!(
                path1, path2,
                "Config path should be consistent across calls"
            );
        }
    }
}
