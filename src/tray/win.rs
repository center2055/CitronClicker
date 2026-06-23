use super::TrayAction;
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct TrayManager {
    _tray: TrayIcon,
}

impl TrayManager {
    pub fn new(rgba: Vec<u8>, w: u32, h: u32) -> Option<TrayManager> {
        // no native menu — we draw our own themed one on right-click (see main.rs)
        let icon = Icon::from_rgba(rgba, w, h).ok()?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("Citron v2")
            .with_icon(icon)
            .build()
            .ok()?;
        let _ = tray.set_visible(false);
        Some(TrayManager { _tray: tray })
    }

    pub fn set_visible(&self, visible: bool) {
        let _ = self._tray.set_visible(visible);
    }

    /// left-click restores, right-click opens our menu at the cursor. act on button-up so it
    /// fires once per click.
    pub fn poll(&self) -> Option<TrayAction> {
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button, button_state, position, .. } = ev {
                if button_state == MouseButtonState::Up {
                    match button {
                        MouseButton::Left => return Some(TrayAction::Show),
                        MouseButton::Right => {
                            return Some(TrayAction::Menu { x: position.x, y: position.y });
                        }
                        _ => {}
                    }
                }
            }
        }
        None
    }
}
