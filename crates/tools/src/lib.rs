//! The OS side of the assistant's tools: computer actions ([`SystemActions`])
//! and weather. The core decides what to do; this only does it. No shell is
//! ever involved.

pub mod apps;
pub mod browser;
pub mod files;
pub mod settings;
pub mod volume;
pub mod weather;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use jarvis_core::{Action, Actions, App, Bang, Browser, Folder};

pub use settings::ActionSettings;

const APP_INDEX_MAX_AGE: Duration = Duration::from_secs(60);

pub struct SystemActions {
    settings: ActionSettings,
    apps: Option<(Instant, Vec<App>)>,
}

impl SystemActions {
    pub fn new(settings: ActionSettings) -> Self {
        Self {
            settings,
            apps: None,
        }
    }

    /// The browser and, when known, its executable.
    fn resolve_browser(&self) -> (Browser, Option<String>) {
        match self
            .settings
            .browser
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            Some(setting) => {
                let (browser, path) = browser::from_setting(setting);
                let exe = path.or_else(|| platform::find_browser_exe(browser.kind));
                (browser, exe)
            }
            None => platform::default_browser(),
        }
    }

    fn open_url(&mut self, url: &str, private: bool) -> Result<String, String> {
        let (browser, exe) = self.resolve_browser();
        if let (true, Some(flag), Some(exe)) = (private, browser.kind.private_flag(), &exe) {
            spawn(exe, &[flag, url])
                .map_err(|e| format!("could not start {}: {e}", browser.name))?;
            return Ok(String::new());
        }
        let result = match (&self.settings.browser, &exe) {
            (Some(_), Some(exe)) => spawn(exe, &[url]),
            _ => open::that_detached(url),
        };
        result.map_err(|e| format!("could not open the link: {e}"))?;
        Ok(String::new())
    }
}

fn spawn(program: &str, args: &[&str]) -> std::io::Result<()> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn folder_path(folder: Folder) -> Option<std::path::PathBuf> {
    match folder {
        Folder::Home => dirs::home_dir(),
        Folder::Desktop => dirs::desktop_dir(),
        Folder::Documents => dirs::document_dir(),
        Folder::Downloads => dirs::download_dir(),
        Folder::Pictures => dirs::picture_dir(),
        Folder::Music => dirs::audio_dir(),
        Folder::Videos => dirs::video_dir(),
    }
}

impl Actions for SystemActions {
    fn browser(&mut self) -> Browser {
        self.resolve_browser().0
    }

    fn bangs(&mut self) -> Vec<Bang> {
        self.settings.to_bangs()
    }

    fn apps(&mut self) -> Vec<App> {
        match &self.apps {
            Some((at, apps)) if at.elapsed() < APP_INDEX_MAX_AGE => apps.clone(),
            _ => {
                let apps = platform::scan_apps();
                self.apps = Some((Instant::now(), apps.clone()));
                apps
            }
        }
    }

    fn run(&mut self, action: &Action) -> Result<String, String> {
        match action {
            Action::OpenUrl { url, private } => self.open_url(url, *private),
            Action::OpenApp(app) => platform::open_app(app),
            Action::OpenFolder(folder) => {
                let path = folder_path(*folder).ok_or("that folder does not exist here")?;
                open::that_detached(&path)
                    .map_err(|e| format!("could not open the folder: {e}"))?;
                Ok(String::new())
            }
            Action::OpenRecentFile(folder) => {
                let dir = folder_path(*folder).ok_or("that folder does not exist here")?;
                let file = files::newest_file(&dir).ok_or("there are no files in that folder")?;
                open::that_detached(&file).map_err(|e| format!("could not open the file: {e}"))?;
                let name = file.file_name().unwrap_or_default().to_string_lossy();
                Ok(format!("opened {name}"))
            }
            Action::Volume(change) => platform::volume(*change),
            Action::Media(key) => platform::media(*key),
        }
    }
}
