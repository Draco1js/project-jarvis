//! Settings, SQLite history and memory (FTS5), keyring secrets, migrations, purge.
//!
//! So far: the data folder location and rotating, redacted log files.

pub mod logs;

use std::path::PathBuf;

/// The per-OS folder holding all app data (settings, database, models, logs).
pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("Jarvis"))
}

/// Where log files go.
pub fn logs_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("logs"))
}
