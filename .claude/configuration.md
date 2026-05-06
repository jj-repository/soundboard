# Configuration

Config path: `%APPDATA%\soundboard\` (via `dirs::config_dir()`).
Runtime path (lock file, downloaded updates): `%LOCALAPPDATA%\soundboard\run\` (via `dirs::data_local_dir()`, see `utils/daemon::get_runtime_dir`).
Helper: `src/utils/config.rs` → `get_config_path() -> Result<PathBuf>`.
