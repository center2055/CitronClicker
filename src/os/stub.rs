//! non-windows no-op surface. physical_button_held is always false so the engine never injects —
//! the app degrades to ui-only.

pub fn begin_timer_period() {}
pub fn small_icon_px() -> u32 {
    16
}
pub fn acquire_single_instance() -> bool {
    true
}
pub fn focus_existing_window() {}
pub fn start_input_hook() -> u32 {
    0
}
pub fn stop_input_hook(_thread_id: u32) {}
pub fn physical_button_held(_is_left: bool) -> bool {
    false
}
pub fn click_down(_is_left: bool) {}
pub fn click_up(_is_left: bool) {}
pub fn jitter_move(_dx: i32, _dy: i32) {}
pub fn cursor_visible() -> bool {
    false
}
pub fn key_held(_vk: i32) -> bool {
    false
}
pub fn is_minecraft_active() -> bool {
    false
}
pub fn is_minecraft_running() -> bool {
    false
}
pub fn any_window_focused() -> bool {
    false
}
pub fn set_autostart(_enabled: bool) {}
pub fn foreground_is_self() -> bool {
    false
}
pub fn set_screen_capture_excluded(_excluded: bool) {}
pub fn set_taskbar_hidden(_hidden: bool) {}
