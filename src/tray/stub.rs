use super::TrayAction;

pub struct TrayManager;

impl TrayManager {
    pub fn new(_rgba: Vec<u8>, _w: u32, _h: u32) -> Option<TrayManager> {
        None
    }
    pub fn set_visible(&self, _visible: bool) {}
    pub fn poll(&self) -> Option<TrayAction> {
        None
    }
}
