//! Picking files in a folder.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// A download or temp file that is still being written.
pub fn is_partial(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "crdownload" | "part" | "partial" | "tmp" | "temp" | "download" | "opdownload" | "aria2"
    )
}

/// A file that would run code if opened, so it is revealed, never opened.
pub fn is_runnable(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "exe"
            | "bat"
            | "cmd"
            | "com"
            | "msi"
            | "msp"
            | "scr"
            | "ps1"
            | "psm1"
            | "vbs"
            | "vbe"
            | "js"
            | "jse"
            | "wsf"
            | "wsh"
            | "lnk"
            | "reg"
            | "hta"
            | "cpl"
            | "jar"
            | "sh"
            | "bash"
            | "zsh"
            | "run"
            | "bin"
            | "desktop"
            | "command"
            | "app"
            | "pkg"
            | "dmg"
            | "appimage"
            | "deb"
            | "rpm"
            | "py"
            | "pl"
            | "rb"
    )
}

/// The most recently modified non-hidden regular file directly in `dir`.
pub fn newest_file(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if is_partial(&entry.path()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            best = Some((modified, entry.path()));
        }
    }
    best.map(|(_, p)| p)
}
