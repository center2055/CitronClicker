//! Windows input layer: synthetic clicks via SendInput, physical-hold detection via a
//! WH_MOUSE_LL hook on a dedicated message-pump thread (filtering LLMHF_INJECTED so our own
//! synthetic clicks never count), foreground / Minecraft detection, cursor + key state.

use std::ptr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Media::timeBeginPeriod;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::ProcessStatus::K32GetModuleBaseNameW;
use windows_sys::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CURSOR_SHOWING, CURSORINFO, CallNextHookEx, DispatchMessageW, GetClassNameW, GetCursorInfo,
    GetForegroundWindow, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, LLMHF_INJECTED,
    MSG, MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WM_RBUTTONDOWN,
    WM_RBUTTONUP,
};

static PHYS_LMB: AtomicBool = AtomicBool::new(false);
static PHYS_RMB: AtomicBool = AtomicBool::new(false);
static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static SEND_LOCK: Mutex<()> = Mutex::new(());

pub fn begin_timer_period() {
    // 1ms timer resolution so thread::sleep(1) is accurate enough that precise_delay only
    // has to spin the last ~2ms of each click cycle.
    unsafe {
        timeBeginPeriod(1);
    }
}

/// The low-level mouse hook proc. Runs on the hook thread while it pumps messages. Must never
/// block, allocate, or panic. Only PHYSICAL (non-injected) events update the shared flags, so
/// our own synthetic clicks cannot create a feedback loop.
unsafe extern "system" fn ll_mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 {
        let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        if info.flags & LLMHF_INJECTED == 0 {
            match wparam as u32 {
                WM_LBUTTONDOWN => PHYS_LMB.store(true, Ordering::Relaxed),
                WM_LBUTTONUP => PHYS_LMB.store(false, Ordering::Relaxed),
                WM_RBUTTONDOWN => PHYS_RMB.store(true, Ordering::Relaxed),
                WM_RBUTTONUP => PHYS_RMB.store(false, Ordering::Relaxed),
                _ => {}
            }
        }
    }
    unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
}

/// Spawn the hook thread (installs WH_MOUSE_LL and pumps). Returns its thread id for shutdown.
pub fn start_input_hook() -> u32 {
    let (tx, rx) = mpsc::channel::<u32>();
    thread::spawn(move || unsafe {
        let hmod = GetModuleHandleW(ptr::null());
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(ll_mouse_proc), hmod, 0);
        let tid = GetCurrentThreadId();
        if hook.is_null() {
            let _ = tx.send(0);
            return;
        }
        HOOK_INSTALLED.store(true, Ordering::Relaxed);
        let _ = tx.send(tid);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // Unhook on the SAME thread that installed it (required by Win32).
        UnhookWindowsHookEx(hook);
        HOOK_INSTALLED.store(false, Ordering::Relaxed);
    });
    rx.recv().unwrap_or(0)
}

pub fn stop_input_hook(thread_id: u32) {
    if thread_id != 0 {
        unsafe {
            PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
        }
    }
}

pub fn physical_button_held(is_left: bool) -> bool {
    if HOOK_INSTALLED.load(Ordering::Relaxed) {
        if is_left {
            PHYS_LMB.load(Ordering::Relaxed)
        } else {
            PHYS_RMB.load(Ordering::Relaxed)
        }
    } else {
        // Fallback before the hook installs (or on failure): no synthetic stream yet to confuse us.
        let vk = if is_left { 0x01 } else { 0x02 };
        unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
    }
}

fn send_mouse(flags: u32, dx: i32, dy: i32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    // Serialize every SendInput (both clickers + jitter) — interleaved injection destabilizes
    // UWP/Bedrock. Held only around the single call, never across a delay.
    let _g = SEND_LOCK.lock().unwrap();
    unsafe {
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn click_down(is_left: bool) {
    send_mouse(
        if is_left {
            MOUSEEVENTF_LEFTDOWN
        } else {
            MOUSEEVENTF_RIGHTDOWN
        },
        0,
        0,
    );
}

pub fn click_up(is_left: bool) {
    send_mouse(
        if is_left {
            MOUSEEVENTF_LEFTUP
        } else {
            MOUSEEVENTF_RIGHTUP
        },
        0,
        0,
    );
}

pub fn jitter_move(dx: i32, dy: i32) {
    send_mouse(MOUSEEVENTF_MOVE, dx, dy);
}

pub fn cursor_visible() -> bool {
    unsafe {
        let mut ci: CURSORINFO = std::mem::zeroed();
        ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
        if GetCursorInfo(&mut ci) != 0 {
            ci.flags & CURSOR_SHOWING != 0
        } else {
            false
        }
    }
}

pub fn key_held(vk: i32) -> bool {
    if vk == 0 {
        return false;
    }
    // Mouse-button bindings must use the physical flags (GetAsyncKeyState would see our clicks).
    if (vk == 0x01 || vk == 0x02) && HOOK_INSTALLED.load(Ordering::Relaxed) {
        return physical_button_held(vk == 0x01);
    }
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

pub fn any_window_focused() -> bool {
    unsafe { !GetForegroundWindow().is_null() }
}

fn read_w(f: impl Fn(*mut u16, i32) -> i32) -> String {
    let mut buf = [0u16; 512];
    let n = f(buf.as_mut_ptr(), buf.len() as i32);
    if n <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn window_title(hwnd: HWND) -> String {
    read_w(|p, c| unsafe { GetWindowTextW(hwnd, p, c) })
}

fn window_class(hwnd: HWND) -> String {
    read_w(|p, c| unsafe { GetClassNameW(hwnd, p, c) })
}

fn foreground_process_name(hwnd: HWND) -> String {
    unsafe {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return String::new();
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 260];
        let n = K32GetModuleBaseNameW(handle, ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32);
        CloseHandle(handle);
        if n == 0 {
            return String::new();
        }
        let mut name = String::from_utf16_lossy(&buf[..n as usize]);
        if let Some(stripped) = name.strip_suffix(".exe") {
            name = stripped.to_string();
        }
        name
    }
}

fn title_is_launcher(t: &str) -> bool {
    const L: [&str; 13] = [
        "hello minecraft",
        "minecraft launcher",
        "prism launcher",
        "polymc",
        "multimc",
        "curseforge",
        "curse forge",
        "gdlauncher",
        "modrinth",
        "tlauncher",
        "ftb app",
        "atlauncher",
        "pcl2",
    ];
    L.iter().any(|s| t.contains(s))
}

fn title_suggests_minecraft(t: &str) -> bool {
    if title_is_launcher(t) {
        return false;
    }
    const M: [&str; 13] = [
        "minecraft",
        "lunar client",
        "badlion",
        "feather",
        "labymod",
        "salwyrr",
        "cm client",
        "cmclient",
        "cm-pack",
        "forge",
        "fabric",
        "neoforge",
        "fml earlyloading",
    ];
    M.iter().any(|s| t.contains(s))
}

fn has_render_class(cls: &str) -> bool {
    cls.contains("glfw") || cls.contains("lwjgl")
}

/// True when the focused window is the actual Minecraft game (launcher-safe; ports the
/// detection from the old source incl. the CM Client fix and custom-runtime clients).
pub fn is_minecraft_active() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }
        let title = window_title(hwnd).to_lowercase();
        if title_is_launcher(&title) {
            return false;
        }
        let cls = window_class(hwnd).to_lowercase();
        let pname = foreground_process_name(hwnd).to_lowercase();

        if pname == "minecraft.windows" || pname == "minecraft" {
            return true; // Bedrock (launcher titles already excluded above)
        }
        if pname == "java" || pname == "javaw" {
            return has_render_class(&cls) || title_suggests_minecraft(&title);
        }
        // Custom clients with bundled/renamed runtimes: require the real GLFW/LWJGL render
        // window so we never fire on an unrelated app that merely has "minecraft" in its title.
        has_render_class(&cls)
    }
}
