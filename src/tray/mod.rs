//! system tray icon. real on windows, no-op stub elsewhere. icon lives on the event-loop thread,
//! events polled each frame via poll().

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
    /// right-click: open our themed menu at this screen position (physical px)
    Menu { x: f64, y: f64 },
}
