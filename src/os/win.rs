//! windows input: synthetic clicks via sendinput, physical-hold detection via a wh_mouse_ll hook
//! on its own pump thread (filters injected events so our clicks don't count), mc/foreground
//! detection, cursor + key state.

use std::ptr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Media::timeBeginPeriod;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThreadId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CURSOR_SHOWING, CURSORINFO, CallNextHookEx, DispatchMessageW, EnumWindows, GetClassNameW,
    GetCursorInfo, GetForegroundWindow, GetMessageW, GetSystemMetrics, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT,
    PostThreadMessageW, SM_CXSMICON, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

static PHYS_LMB: AtomicBool = AtomicBool::new(false);
static PHYS_RMB: AtomicBool = AtomicBool::new(false);
static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static SEND_LOCK: Mutex<()> = Mutex::new(());

pub fn begin_timer_period() {
    // 1ms timer res so sleep(1) is tight enough to only spin the last ~2ms of a cycle
    unsafe {
        timeBeginPeriod(1);
    }
}

/// tray icon size for the current dpi (16px @ 100%, 24 @ 150%). render at exactly this so the
/// shell draws it 1:1 instead of rescaling.
pub fn small_icon_px() -> u32 {
    let s = unsafe { GetSystemMetrics(SM_CXSMICON) };
    if s <= 0 { 16 } else { s as u32 }
}

/// low-level mouse hook. runs on the pump thread — must never block/alloc/panic. only physical
/// (non-injected) events touch the flags so our own clicks don't feed back.
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

/// spawn the hook thread (installs wh_mouse_ll + pumps). returns its tid for shutdown.
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
        // must unhook on the same thread that installed it
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
        // before the hook is up (or on failure) — nothing injecting yet to confuse us
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
    // serialize every sendinput (both clickers + jitter) — interleaved injection breaks
    // uwp/bedrock. held only around the call, never across a delay.
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
    // mouse-button binds use the physical flags (getasynckeystate would see our own clicks)
    if (vk == 0x01 || vk == 0x02) && HOOK_INSTALLED.load(Ordering::Relaxed) {
        return physical_button_held(vk == 0x01);
    }
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

pub fn any_window_focused() -> bool {
    unsafe { !GetForegroundWindow().is_null() }
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "Citron v2";
const RUN_NAME_LEGACY: &str = "Citron Clicker Premium"; // pre-rename entry, cleaned up below

/// add/remove the per-user run key entry so it launches at login
pub fn set_autostart(enabled: bool) {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let run = match RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY) {
        Ok((k, _)) => k,
        Err(_) => return,
    };
    // drop the old-named entry so a pre-rename autostart can't point at a dead exe
    let _ = run.delete_value(RUN_NAME_LEGACY);
    if enabled {
        if let Ok(exe) = std::env::current_exe() {
            let _ = run.set_value(RUN_NAME, &format!("\"{}\"", exe.display()));
        }
    } else {
        let _ = run.delete_value(RUN_NAME);
    }
}

/// true when our own window is focused — so we never click into our own ui
pub fn foreground_is_self() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        pid != 0 && pid == GetCurrentProcessId()
    }
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
        // queryfullprocessimagenamew, not k32getmodulebasenamew: the latter gives access_denied
        // for sandboxed app-container procs like bedrock (minecraft.windows.exe) and silently
        // breaks detection. this gives a full path; grab the file name.
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        if ok == 0 || len == 0 {
            return String::new();
        }
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        let file = full.rsplit(['\\', '/']).next().unwrap_or(&full);
        match file.get(file.len().saturating_sub(4)..) {
            Some(ext) if ext.eq_ignore_ascii_case(".exe") => file[..file.len() - 4].to_string(),
            _ => file.to_string(),
        }
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

/// true if this window is the actual mc game (not a launcher). handles custom clients (cm client
/// etc.) — the glfw/lwjgl render class is the strongest signal, checked before any process query.
fn hwnd_is_mc(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        return false;
    }
    let title = window_title(hwnd).to_lowercase();
    if title_is_launcher(&title) {
        return false;
    }
    let cls = window_class(hwnd).to_lowercase();
    if has_render_class(&cls) {
        return true; // glfw/lwjgl game window (mc java 1.13+ / most clients)
    }
    if cls == "bedrock" {
        return true; // bedrock's window class (process query is unreliable for it)
    }
    let pname = foreground_process_name(hwnd).to_lowercase();
    if pname == "minecraft.windows" || pname == "minecraft" {
        return true; // bedrock
    }
    if pname == "java" || pname == "javaw" {
        return title_suggests_minecraft(&title);
    }
    false
}

/// true when mc is the focused window — gates clicking in "only in-game"
pub fn is_minecraft_active() -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    hwnd_is_mc(hwnd)
}

unsafe extern "system" fn enum_mc(hwnd: HWND, lparam: isize) -> i32 {
    // cheap class/title pre-filter so we only do a process query on real candidates
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return 1;
    }
    let cls = window_class(hwnd).to_lowercase();
    let title = window_title(hwnd).to_lowercase();
    if (has_render_class(&cls) || cls == "bedrock" || title.contains("minecraft")) && hwnd_is_mc(hwnd)
    {
        unsafe { *(lparam as *mut bool) = true };
        return 0; // found — stop enumerating
    }
    1
}

/// true if an mc window exists anywhere (running, even if not focused). for the status badge —
/// unlike is_minecraft_active which the clicker uses.
pub fn is_minecraft_running() -> bool {
    let mut found = false;
    unsafe {
        EnumWindows(Some(enum_mc), &mut found as *mut bool as isize);
    }
    found
}
