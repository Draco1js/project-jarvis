//! Windows: registry (via `reg.exe`, no shell), Start Menu, Core Audio, media keys.

use std::path::PathBuf;
use std::process::Command;

use jarvis_core::{App, Browser, BrowserKind, MediaKey, Volume};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK,
};

use crate::apps;
use crate::browser;
use crate::volume::target_level;

fn reg_query(key: &str, value: &str) -> Option<String> {
    let mut cmd = Command::new("reg");
    cmd.args(["query", key]);
    if value.is_empty() {
        cmd.arg("/ve");
    } else {
        cmd.args(["/v", value]);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if value.is_empty() {
        browser::parse_reg_default(&text)
    } else {
        browser::parse_reg_value(&text, value)
    }
}

pub fn default_browser() -> (Browser, Option<String>) {
    let prog_id = reg_query(
        r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
        "ProgId",
    );
    let Some(prog_id) = prog_id else {
        return (other(), None);
    };
    let kind = browser::kind_from_prog_id(&prog_id);
    let exe = reg_query(&format!(r"HKCR\{prog_id}\shell\open\command"), "")
        .and_then(|c| browser::exe_from_command(&c));
    (
        Browser {
            kind,
            name: browser::display_name(kind).to_string(),
        },
        exe,
    )
}

fn other() -> Browser {
    Browser {
        kind: BrowserKind::Other,
        name: browser::display_name(BrowserKind::Other).to_string(),
    }
}

/// Where an installed copy of a named browser usually lives.
pub fn find_browser_exe(kind: BrowserKind) -> Option<String> {
    let rel = match kind {
        BrowserKind::Chrome => r"Google\Chrome\Application\chrome.exe",
        BrowserKind::Brave => r"BraveSoftware\Brave-Browser\Application\brave.exe",
        BrowserKind::Chromium => r"Chromium\Application\chrome.exe",
        BrowserKind::Edge => r"Microsoft\Edge\Application\msedge.exe",
        BrowserKind::Firefox => r"Mozilla Firefox\firefox.exe",
        BrowserKind::Opera => r"Opera\opera.exe",
        _ => return None,
    };
    ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join(rel))
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
}

pub fn scan_apps() -> Vec<App> {
    let roots: Vec<PathBuf> = ["ProgramData", "AppData"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs"))
        .collect();
    apps::scan_start_menu(&roots)
}

pub fn open_app(app: &App) -> Result<String, String> {
    open::that_detached(&app.target).map_err(|e| format!("could not open {}: {e}", app.name))?;
    Ok(format!("opened {}", app.name))
}

fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        // Fine if COM is already initialised on this thread.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
    }
}

fn set_volume(change: Volume) -> windows::core::Result<String> {
    let vol = endpoint()?;
    unsafe {
        match change {
            Volume::Mute => {
                vol.SetMute(true, std::ptr::null())?;
                Ok("muted".into())
            }
            Volume::Unmute => {
                vol.SetMute(false, std::ptr::null())?;
                Ok("unmuted".into())
            }
            _ => {
                let current = (vol.GetMasterVolumeLevelScalar()?.clamp(0.0, 1.0) * 100.0).round();
                let level = target_level(current as u8, change).unwrap_or(0);
                vol.SetMasterVolumeLevelScalar(f32::from(level) / 100.0, std::ptr::null())?;
                Ok(format!("volume is {level}%"))
            }
        }
    }
}

pub fn volume(change: Volume) -> Result<String, String> {
    set_volume(change).map_err(|e| format!("could not change the volume: {e}"))
}

pub fn media(key: MediaKey) -> Result<String, String> {
    let vk = match key {
        MediaKey::PlayPause => VK_MEDIA_PLAY_PAUSE,
        MediaKey::Next => VK_MEDIA_NEXT_TRACK,
        MediaKey::Previous => VK_MEDIA_PREV_TRACK,
    };
    let event = |vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [event(vk, KEYBD_EVENT_FLAGS(0)), event(vk, KEYEVENTF_KEYUP)];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(String::new())
    } else {
        Err("could not send the media key".into())
    }
}
