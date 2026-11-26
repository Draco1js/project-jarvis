//! Settings, SQLite history and memory (FTS5), keyring secrets, migrations, purge.
//!
//! So far: the data folder location, rotating, redacted log files, and
//! timers/alarms/reminders in SQLite.

pub mod logs;
pub mod reminders;

use std::path::PathBuf;

/// The per-OS folder holding all app data (settings, database, models, logs).
pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("Jarvis"))
}

/// Where log files go.
pub fn logs_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("logs"))
}

/// The SQLite database file.
pub fn database_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("jarvis.db"))
}
