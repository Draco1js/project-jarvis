//! macOS (best effort): `open`, `osascript`, /Applications.

use std::path::PathBuf;
use std::process::Command;

use jarvis_core::{App, Browser, BrowserKind, MediaKey, Volume};

use crate::apps;
use crate::browser;
use crate::volume::target_level;

pub fn find_browser_exe(_kind: BrowserKind) -> Option<String> {
    None
}

pub fn default_browser() -> (Browser, Option<String>) {
    // Safari is the usual default; we can't tell cheaply, and private mode is
    // not supported there anyway.
    let kind = BrowserKind::Safari;
    (
        Browser {
            kind,
            name: browser::display_name(kind).to_string(),
        },
        None,
    )
}

pub fn scan_apps() -> Vec<App> {
    let mut dirs = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    dirs.extend(dirs::home_dir().map(|h| h.join("Applications")));
    apps::scan_mac_apps(&dirs)
}

pub fn open_app(app: &App) -> Result<String, String> {
    let status = Command::new("open")
        .arg(&app.target)
        .status()
        .map_err(|e| format!("could not open {}: {e}", app.name))?;
    if status.success() {
        Ok(format!("opened {}", app.name))
    } else {
        Err(format!("could not open {}", app.name))
    }
}

fn osascript(script: &str) -> Result<String, String> {
    let out = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| format!("could not run osascript: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn volume(change: Volume) -> Result<String, String> {
    match change {
        Volume::Mute => {
            osascript("set volume output muted true")?;
            Ok("muted".into())
        }
        Volume::Unmute => {
            osascript("set volume output muted false")?;
            Ok("unmuted".into())
        }
        _ => {
            let current = osascript("output volume of (get volume settings)")?
                .parse::<u8>()
                .unwrap_or(0);
            let level = target_level(current, change).unwrap_or(current);
            osascript(&format!("set volume output volume {level}"))?;
            Ok(format!("volume is {level}%"))
        }
    }
}

pub fn media(_key: MediaKey) -> Result<String, String> {
    Err("media keys are not supported on macOS yet".into())
}
