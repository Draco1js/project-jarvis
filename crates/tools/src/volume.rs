//! Volume arithmetic shared by every platform.

use jarvis_core::Volume;

/// The new level (0..=100) for a relative or absolute change; `None` for mute changes.
pub fn target_level(current: u8, change: Volume) -> Option<u8> {
    let current = i32::from(current);
    let level = match change {
        Volume::Set(n) => i32::from(n),
        Volume::Up(n) => current + i32::from(n),
        Volume::Down(n) => current - i32::from(n),
        Volume::Mute | Volume::Unmute => return None,
    };
    Some(level.clamp(0, 100) as u8)
}
