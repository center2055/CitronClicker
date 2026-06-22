//! System tray icon. Real implementation on Windows (tray-icon crate); a no-op stub elsewhere
//! so the app builds everywhere. The tray icon lives on the main (event-loop) thread; its
//! events are polled each frame from the UI via `poll()`.

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
