use super::TrayAction;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct TrayManager {
    _tray: TrayIcon,
    show_id: MenuId,
    quit_id: MenuId,
}

impl TrayManager {
    pub fn new(rgba: Vec<u8>, w: u32, h: u32) -> Option<TrayManager> {
        let menu = Menu::new();
        let show = MenuItem::new("Show Citron v2", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let show_id = show.id().clone();
        let quit_id = quit.id().clone();
        menu.append(&show).ok()?;
        menu.append(&quit).ok()?;
        let icon = Icon::from_rgba(rgba, w, h).ok()?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("Citron v2")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false) // left-click restores, right-click opens the menu
            .with_menu_on_right_click(true)
            .build()
            .ok()?;
        let _ = tray.set_visible(false);
        Some(TrayManager {
            _tray: tray,
            show_id,
            quit_id,
        })
    }

    pub fn set_visible(&self, visible: bool) {
        let _ = self._tray.set_visible(visible);
    }

    /// drain pending tray/menu events, return an action if any
    pub fn poll(&self) -> Option<TrayAction> {
        if let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.quit_id {
                return Some(TrayAction::Quit);
            }
            if ev.id == self.show_id {
                return Some(TrayAction::Show);
            }
        }
        if let Ok(TrayIconEvent::Click {
            button: MouseButton::Left,
            ..
        }) = TrayIconEvent::receiver().try_recv()
        {
            return Some(TrayAction::Show);
        }
        None
    }
}
