//! Settings, SQLite history and memory (FTS5), keyring secrets, migrations, purge.
//!
//! So far: the data folder location, rotating redacted log files,
//! timers/alarms/reminders, and conversation history, notes and memory
//! search ([`memory`]), all in one SQLite database.

pub mod logs;
pub mod memory;
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

/// The SQLite database file (reminders, history and the memory index).
pub fn database_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("jarvis.db"))
}
