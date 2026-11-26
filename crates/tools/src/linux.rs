//! Linux: xdg-settings, .desktop files, gio, wpctl/pactl, playerctl.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use jarvis_core::{App, Browser, BrowserKind, MediaKey, Volume};

use crate::apps;
use crate::browser;
use crate::volume::target_level;

fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run to completion. `Err` carries the reason, or `NotFound` if missing.
fn run(program: &str, args: &[&str]) -> Result<String, std::io::Error> {
    let out = Command::new(program).args(args).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

pub fn which(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

pub fn find_browser_exe(kind: BrowserKind) -> Option<String> {
    browser::binary_names(kind)
        .iter()
        .find_map(|n| which(n))
        .map(|p| p.to_string_lossy().into_owned())
}

pub fn default_browser() -> (Browser, Option<String>) {
    let id = output("xdg-settings", &["get", "default-web-browser"]).unwrap_or_default();
    let kind = browser::kind_from_desktop_id(&id);
    (
        Browser {
            kind,
            name: browser::display_name(kind).to_string(),
        },
        find_browser_exe(kind),
    )
}

pub fn scan_apps() -> Vec<App> {
    let dirs = apps::xdg_application_dirs(
        std::env::var("XDG_DATA_HOME").ok().as_deref(),
        dirs::home_dir().as_deref(),
        std::env::var("XDG_DATA_DIRS").ok().as_deref(),
    );
    apps::scan_desktop_dirs(&dirs)
}

pub fn open_app(app: &App) -> Result<String, String> {
    let fail = |e: String| format!("could not open {}: {e}", app.name);
    match Command::new("gio")
        .args(["launch", &app.target])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(s) if s.success() => return Ok(format!("opened {}", app.name)),
        _ => {}
    }
    let text = std::fs::read_to_string(&app.target).map_err(|e| fail(e.to_string()))?;
    let entry = apps::parse_desktop(&text).ok_or_else(|| fail("not a launchable app".into()))?;
    let words = apps::split_exec(&entry.exec);
    let (program, rest) = words
        .split_first()
        .ok_or_else(|| fail("no command".into()))?;
    Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| fail(e.to_string()))?;
    Ok(format!("opened {}", app.name))
}

fn wpctl_level() -> Option<u8> {
    // "Volume: 0.50" or "Volume: 0.50 [MUTED]"
    let text = output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])?;
    let v: f32 = text.split_whitespace().nth(1)?.parse().ok()?;
    Some((v * 100.0).round() as u8)
}

fn pactl_level() -> Option<u8> {
    let text = output("pactl", &["get-sink-volume", "@DEFAULT_SINK@"])?;
    let pct = text.split_whitespace().find(|w| w.ends_with('%'))?;
    pct.trim_end_matches('%').parse().ok()
}

pub fn volume(change: Volume) -> Result<String, String> {
    let sink = "@DEFAULT_AUDIO_SINK@";
    let (wp, pa): (Vec<String>, Vec<String>) = match change {
        Volume::Mute => (
            vec!["set-mute".into(), sink.into(), "1".into()],
            vec!["set-sink-mute".into(), "@DEFAULT_SINK@".into(), "1".into()],
        ),
        Volume::Unmute => (
            vec!["set-mute".into(), sink.into(), "0".into()],
            vec!["set-sink-mute".into(), "@DEFAULT_SINK@".into(), "0".into()],
        ),
        Volume::Set(n) => {
            let p = format!("{}%", n.min(100));
            (
                vec!["set-volume".into(), sink.into(), p.clone()],
                vec!["set-sink-volume".into(), "@DEFAULT_SINK@".into(), p],
            )
        }
        Volume::Up(n) => {
            // Never boost past 100%.
            let (mut wp, mut pa) = rel(sink, n, '+');
            wp.splice(1..1, ["-l".to_string(), "1.0".to_string()]);
            if let Some(level) = pactl_level() {
                let target = target_level(level, change).unwrap_or(100);
                pa = vec![
                    "set-sink-volume".into(),
                    "@DEFAULT_SINK@".into(),
                    format!("{target}%"),
                ];
            }
            (wp, pa)
        }
        Volume::Down(n) => rel(sink, n, '-'),
    };
    let w: Vec<&str> = wp.iter().map(String::as_str).collect();
    let p: Vec<&str> = pa.iter().map(String::as_str).collect();
    let mut result = run("wpctl", &w);
    if result.is_err() {
        result = run("pactl", &p);
    }
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err("no volume control found (install wpctl or pactl)".into())
        }
        Err(e) => Err(format!("could not change the volume: {e}")),
        Ok(_) => Ok(match change {
            Volume::Mute => "muted".into(),
            Volume::Unmute => "unmuted".into(),
            Volume::Set(n) => format!("volume is {}%", n.min(100)),
            Volume::Up(_) | Volume::Down(_) => match wpctl_level().or_else(pactl_level) {
                Some(l) => format!("volume is {l}%"),
                None => String::new(),
            },
        }),
    }
}

fn rel(sink: &str, n: u8, sign: char) -> (Vec<String>, Vec<String>) {
    // wpctl wants "10%+", pactl "+10%".
    (
        vec!["set-volume".into(), sink.into(), format!("{n}%{sign}")],
        vec![
            "set-sink-volume".into(),
            "@DEFAULT_SINK@".into(),
            format!("{sign}{n}%"),
        ],
    )
}

pub fn media(key: MediaKey) -> Result<String, String> {
    let arg = match key {
        MediaKey::PlayPause => "play-pause",
        MediaKey::Next => "next",
        MediaKey::Previous => "previous",
    };
    match run("playerctl", &[arg]) {
        Ok(_) => Ok(String::new()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err("playerctl is not installed".into())
        }
        Err(e) => Err(format!("no media player answered: {e}")),
    }
}
