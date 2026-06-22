//! system tray icon. real on windows (tray-icon crate), no-op stub elsewhere so it builds
//! everywhere. the icon lives on the main (event-loop) thread; events get polled each frame via
//! poll().

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::*;

pub enum TrayAction {
    Show,
    Quit,
}
