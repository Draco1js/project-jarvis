//! System tray icon with "Show Jarvis" and "Quit" items.

use anyhow::Result;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

const SHOW_ID: &str = "show";
const QUIT_ID: &str = "quit";

pub enum Command {
    Show,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
}

impl Tray {
    pub fn new() -> Result<Self> {
        let menu = Menu::new();
        menu.append(&MenuItem::with_id(SHOW_ID, "Show Jarvis", true, None))?;
        menu.append(&MenuItem::with_id(QUIT_ID, "Quit", true, None))?;

        let icon = TrayIconBuilder::new()
            .with_tooltip("Jarvis")
            .with_icon(placeholder_icon()?)
            .with_menu(Box::new(menu))
            .build()?;
        Ok(Self { _icon: icon })
    }
}

/// Next pending tray command, if any.
pub fn poll() -> Option<Command> {
    let event = MenuEvent::receiver().try_recv().ok()?;
    match event.id().as_ref() {
        SHOW_ID => Some(Command::Show),
        QUIT_ID => Some(Command::Quit),
        _ => None,
    }
}

/// A plain filled circle until real artwork exists.
fn placeholder_icon() -> Result<Icon> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let center = SIZE as f32 / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let inside = dx * dx + dy * dy <= (center - 1.0) * (center - 1.0);
            rgba.extend_from_slice(if inside {
                &[0x3b, 0x82, 0xf6, 0xff]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    Ok(Icon::from_rgba(rgba, SIZE, SIZE)?)
}
