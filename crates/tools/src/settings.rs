//! The user-editable action settings (`actions.json`).

use std::io;
use std::path::Path;

use jarvis_core::Bang;
use serde::{Deserialize, Serialize};

/// A spoken site name and its bang (without `!`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BangSetting {
    pub name: String,
    pub bang: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionSettings {
    /// `None` is the system default browser. Otherwise a browser name
    /// ("chrome", "firefox", ...) or the path of an executable.
    pub browser: Option<String>,
    pub bangs: Vec<BangSetting>,
}

impl Default for ActionSettings {
    fn default() -> Self {
        let bang = |name: &str, bang: &str| BangSetting {
            name: name.into(),
            bang: bang.into(),
        };
        Self {
            browser: None,
            bangs: vec![
                bang("YouTube", "yt"),
                bang("Reddit", "r"),
                bang("GitHub", "gh"),
                bang("Amazon", "a"),
                bang("Wikipedia", "w"),
            ],
        }
    }
}

impl ActionSettings {
    /// Read the file. A missing file gives the defaults silently; an unreadable
    /// or invalid one gives the defaults and a warning.
    pub fn load(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Self::default(),
            Err(e) => {
                log::warn!("cannot read {}: {e}; using defaults", path.display());
                return Self::default();
            }
        };
        serde_json::from_str(&text).unwrap_or_else(|e| {
            log::warn!("bad settings in {}: {e}; using defaults", path.display());
            Self::default()
        })
    }

    /// Write pretty JSON, creating parent folders.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::write(path, text)
    }

    pub fn to_bangs(&self) -> Vec<Bang> {
        self.bangs
            .iter()
            .map(|b| Bang {
                name: b.name.clone(),
                bang: b.bang.clone(),
            })
            .collect()
    }
}
