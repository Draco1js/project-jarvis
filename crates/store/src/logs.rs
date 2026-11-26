//! Local log files: `jarvis.log` plus up to four older ones, each at most
//! 5 MB, with API keys and bearer tokens redacted before anything is written.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use regex::Regex;

pub const LOG_FILE_NAME: &str = "jarvis.log";
pub const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
/// The current file plus four rotated ones.
pub const MAX_LOG_FILES: usize = 5;

const REDACTED: &str = "[REDACTED]";

/// Key-shaped strings redacted even when the key itself is not known:
/// Google API keys, Groq keys, and bearer tokens.
static KEY_PATTERNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"AIza[0-9A-Za-z_\-]{30,}|gsk_[0-9A-Za-z]{20,}|(?i:bearer)\s+[^\s\x22']+")
        .expect("valid key pattern")
});

/// `text` with every `secrets` entry and every key-shaped string replaced.
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        out = out.replace(secret.as_str(), REDACTED);
    }
    KEY_PATTERNS.replace_all(&out, REDACTED).into_owned()
}

/// An append-only log file that rotates `jarvis.log` → `jarvis.log.1` → …
/// before a line would push it past `max_bytes`, keeping `max_files` in all.
/// Lines are never split across files.
pub struct RotatingFile {
    dir: PathBuf,
    max_bytes: u64,
    max_files: usize,
    file: File,
    size: u64,
    pending: Vec<u8>,
}

impl RotatingFile {
    pub fn open(dir: &Path, max_bytes: u64, max_files: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let file = open_append(&dir.join(LOG_FILE_NAME))?;
        let size = file.metadata()?.len();
        Ok(Self {
            dir: dir.to_path_buf(),
            max_bytes,
            max_files: max_files.max(1),
            file,
            size,
            pending: Vec::new(),
        })
    }

    fn path(&self, index: usize) -> PathBuf {
        match index {
            0 => self.dir.join(LOG_FILE_NAME),
            n => self.dir.join(format!("{LOG_FILE_NAME}.{n}")),
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let oldest = self.path(self.max_files - 1);
        if oldest.exists() {
            fs::remove_file(&oldest)?;
        }
        for index in (0..self.max_files - 1).rev() {
            let from = self.path(index);
            if from.exists() {
                fs::rename(&from, self.path(index + 1))?;
            }
        }
        self.file = open_append(&self.path(0))?;
        self.size = 0;
        Ok(())
    }

    fn write_line(&mut self, line: &[u8]) -> io::Result<()> {
        // A line longer than the limit is cut so no file ever exceeds it.
        let line = &line[..line.len().min(self.max_bytes as usize)];
        if self.size > 0 && self.size + line.len() as u64 > self.max_bytes {
            self.rotate()?;
        }
        self.file.write_all(line)?;
        self.size += line.len() as u64;
        Ok(())
    }
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            self.write_line(&line)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.write_line(&line)?;
        }
        self.file.flush()
    }
}

impl Drop for RotatingFile {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// A `log` backend writing redacted lines to rotating files.
pub struct FileLogger {
    file: Mutex<RotatingFile>,
    secrets: Vec<String>,
}

impl FileLogger {
    /// Logs into `dir` with the standard limits (5 × 5 MB), redacting `secrets`.
    pub fn open(dir: &Path, secrets: Vec<String>) -> io::Result<Self> {
        Ok(Self {
            file: Mutex::new(RotatingFile::open(dir, MAX_LOG_BYTES, MAX_LOG_FILES)?),
            secrets,
        })
    }

    /// Installs this logger for the whole process.
    pub fn install(self) -> Result<(), log::SetLoggerError> {
        log::set_boxed_logger(Box::new(self))?;
        log::set_max_level(log::LevelFilter::Info);
        Ok(())
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Our crates at Info, everything else (HTTP, UI) only at Warn.
        metadata.level() <= log::Level::Warn
            || (metadata.target().starts_with("jarvis") && metadata.level() <= log::Level::Info)
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = redact(&record.args().to_string(), &self.secrets);
        let line = format!(
            "{} {:<5} {}: {}\n",
            humantime::format_rfc3339_seconds(SystemTime::now()),
            record.level(),
            record.target(),
            message.replace('\n', " ")
        );
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}
