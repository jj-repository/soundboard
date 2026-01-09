use std::{error::Error, path::PathBuf};

pub fn get_config_path() -> Result<PathBuf, Box<dyn Error>> {
    let config_path = dirs::config_dir()
        .ok_or_else(|| "Failed to obtain config directory: platform may not support config dirs")?;
    Ok(config_path.join("pwsp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_config_path_returns_result() {
        // This test verifies get_config_path returns a valid Result
        // On most systems, this should succeed
        let result = get_config_path();

        // The function should either succeed or return a meaningful error
        match result {
            Ok(path) => {
                // Path should end with "pwsp"
                assert!(path.ends_with("pwsp"), "Config path should end with 'pwsp'");
                // Path should be within a config directory
                assert!(path.to_string_lossy().contains("config") ||
                        path.to_string_lossy().contains(".config") ||
                        path.to_string_lossy().contains("AppData"),
                        "Config path should be in a config directory");
            }
            Err(e) => {
                // If it fails, the error message should be descriptive
                let error_msg = e.to_string();
                assert!(error_msg.contains("config directory") ||
                        error_msg.contains("platform"),
                        "Error should mention config directory issue");
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
        // Calling get_config_path multiple times should return the same path
        if let (Ok(path1), Ok(path2)) = (get_config_path(), get_config_path()) {
            assert_eq!(path1, path2, "Config path should be consistent across calls");
        }
    }
}
