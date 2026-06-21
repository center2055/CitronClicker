//! Autoclicker engine: background threads driving synthetic clicks while the user physically
//! holds the button. The UI thread owns the config and pushes a snapshot here each frame.

pub mod timing;

use crate::os;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use timing::{HumanizedDelay, Rng, SmoothJitter, fixed_delays};

/// Hot, lock-free flags shared with the threads. Relaxed ordering: these are advisory gates,
/// not synchronization of other memory.
pub struct EngineSignals {
    pub suspend_left: AtomicBool,
    pub suspend_right: AtomicBool,
    pub panic: AtomicBool,
    pub mc_focused: AtomicBool,
    pub any_focused: AtomicBool,
    pub running: AtomicBool,
}

/// Per-clicker config the engine reads. Built from the UI's `Clicker` each frame.
#[derive(Clone, PartialEq)]
pub struct ClickerSnap {
    pub enabled: bool,
    pub min_cps: f32,
    pub max_cps: f32,
    pub cps: f32,
    pub avoid_gui: bool,
    pub humanize: bool,
    pub jitter: bool,
    pub jitter_intensity: i32,
    pub only_ingame: bool,
    pub hold: bool,
    pub suspend_vk: i32,
    pub hotkey_vk: i32,
    pub is_left: bool,
}

#[derive(Clone, PartialEq)]
pub struct EngineConfig {
    pub left: ClickerSnap,
    pub right: ClickerSnap,
    pub panic_vk: i32,
}

pub enum ToggleReq {
    Left,
    Right,
    PanicAll,
}

pub struct EngineHandle {
    pub signals: Arc<EngineSignals>,
    pub config: Arc<Mutex<EngineConfig>>,
    pub toggle_rx: Receiver<ToggleReq>,
    joins: Vec<JoinHandle<()>>,
    hook_tid: u32,
}

impl EngineHandle {
    pub fn start(ctx: egui::Context, initial: EngineConfig) -> Self {
        os::begin_timer_period();
        let hook_tid = os::start_input_hook();

        let signals = Arc::new(EngineSignals {
            suspend_left: AtomicBool::new(false),
            suspend_right: AtomicBool::new(false),
            panic: AtomicBool::new(false),
            mc_focused: AtomicBool::new(false),
            any_focused: AtomicBool::new(false),
            running: AtomicBool::new(true),
        });
        let config = Arc::new(Mutex::new(initial));
        let (tx, rx) = channel::<ToggleReq>();

        let mut joins = Vec::new();
        for is_left in [true, false] {
            let s = signals.clone();
            let c = config.clone();
            joins.push(thread::spawn(move || clicker_loop(is_left, s, c)));
        }
        {
            let s = signals.clone();
            let c = config.clone();
            joins.push(thread::spawn(move || key_poll_loop(s, c, tx, ctx)));
        }

        EngineHandle {
            signals,
            config,
            toggle_rx: rx,
            joins,
            hook_tid,
        }
    }

    pub fn shutdown(&mut self) {
        self.signals.running.store(false, Ordering::Relaxed);
        os::stop_input_hook(self.hook_tid);
        for j in self.joins.drain(..) {
            let _ = j.join();
        }
    }
}

/// Map a UI key-name string to a Windows virtual-key code (0 = none). Platform-independent.
pub fn vk_from_name(name: &str) -> i32 {
    let n = name.trim();
    match n.to_ascii_lowercase().as_str() {
        "" | "none" => 0,
        "left click" => 0x01,
        "right click" => 0x02,
        "middle click" => 0x04,
        "mouse 4" => 0x05,
        "mouse 5" => 0x06,
        "space" => 0x20,
        "shift" => 0x10,
        "ctrl" | "control" => 0x11,
        "alt" => 0x12,
        "tab" => 0x09,
        _ => {
            if n.chars().count() == 1 {
                let ch = n.chars().next().unwrap().to_ascii_uppercase();
                if ch.is_ascii_alphanumeric() {
                    return ch as i32;
                }
                0
            } else if let Some(num) = n.strip_prefix(['F', 'f']) {
                match num.parse::<i32>() {
                    Ok(k) if (1..=24).contains(&k) => 0x70 + (k - 1),
                    _ => 0,
                }
            } else {
                0
            }
        }
    }
}

/// Time-accumulation scheduler: keeps the long-run click rate accurate by compensating each
/// cycle for dispatch jitter (ported from the old ClickerLoop). Returns `(comp_up, comp_down)`.
struct ClickScheduler {
    start: Instant,
    next_expected: f64,
}

impl ClickScheduler {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            next_expected: 0.0,
        }
    }
    fn reset(&mut self) {
        self.start = Instant::now();
        self.next_expected = 0.0;
    }
    fn next(&mut self, up: f64, down: f64) -> (f64, f64) {
        let total = up + down;
        self.next_expected += total;
        let elapsed = self.start.elapsed().as_secs_f64() * 1000.0;
        let mut needed = self.next_expected - elapsed;
        if needed < 10.0 {
            needed = 10.0;
            self.next_expected = elapsed + 10.0;
        }
        let ratio = if total > 0.0 { up / total } else { 0.9 };
        let comp_up = (needed * ratio).round().max(0.0);
        let comp_down = (needed - comp_up).round().max(0.0);
        (comp_up, comp_down)
    }
}

/// Accurate wait that aborts early if the engine stops or the physical button is released.
fn precise_delay(ms: f64, sig: &EngineSignals, is_left: bool) {
    if ms <= 0.0 {
        return;
    }
    let start = Instant::now();
    let target = Duration::from_secs_f64(ms / 1000.0);
    loop {
        if !sig.running.load(Ordering::Relaxed) || !os::physical_button_held(is_left) {
            break;
        }
        let elapsed = start.elapsed();
        if elapsed >= target {
            break;
        }
        if target - elapsed > Duration::from_millis(2) {
            thread::sleep(Duration::from_millis(1));
        } else {
            std::hint::spin_loop();
        }
    }
}

fn clicker_loop(is_left: bool, sig: Arc<EngineSignals>, cfg: Arc<Mutex<EngineConfig>>) {
    let mut rng = Rng::seeded(if is_left { 0xA17 } else { 0xB29 });
    let mut hd = HumanizedDelay::new();
    let mut jit = SmoothJitter::new();
    let mut sched = ClickScheduler::new();
    let mut was_clicking = false;

    while sig.running.load(Ordering::Relaxed) {
        let snap = {
            let c = cfg.lock().unwrap();
            if is_left { c.left.clone() } else { c.right.clone() }
        };

        let suspend = if is_left {
            sig.suspend_left.load(Ordering::Relaxed)
        } else {
            sig.suspend_right.load(Ordering::Relaxed)
        };
        let focus_ok = if snap.only_ingame {
            sig.mc_focused.load(Ordering::Relaxed)
        } else {
            sig.any_focused.load(Ordering::Relaxed)
        };
        let gui_block = snap.avoid_gui && os::cursor_visible();
        let phys = os::physical_button_held(is_left);
        let should = snap.enabled
            && !sig.panic.load(Ordering::Relaxed)
            && focus_ok
            && !gui_block
            && !suspend
            && phys;

        // Hold mode (right only): one sustained right-button-down while held — place / eat / block.
        if should && snap.hold && !is_left {
            if !was_clicking {
                os::click_down(false);
                was_clicking = true;
                jit.reset();
            }
            if snap.jitter {
                if let Some((dx, dy)) = jit.next(snap.jitter_intensity, &mut rng) {
                    os::jitter_move(dx, dy);
                }
            }
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        if should {
            if !was_clicking {
                sched.reset();
                jit.reset();
                was_clicking = true;
            }
            let (up_ms, down_ms) = if snap.humanize {
                hd.get_delays(snap.min_cps, snap.max_cps, &mut rng)
            } else {
                fixed_delays(snap.cps)
            };
            let (comp_up, comp_down) = sched.next(up_ms, down_ms);

            os::click_up(is_left);
            if snap.jitter {
                if let Some((dx, dy)) = jit.next(snap.jitter_intensity, &mut rng) {
                    os::jitter_move(dx, dy);
                }
            }
            precise_delay(comp_up, &sig, is_left);
            if !os::physical_button_held(is_left) {
                continue; // released mid-cycle; else-branch next loop emits the trailing UP
            }
            os::click_down(is_left);
            precise_delay(comp_down, &sig, is_left);
        } else {
            if was_clicking {
                os::click_up(is_left);
                was_clicking = false;
            }
            thread::sleep(Duration::from_millis(8));
        }
    }

    if was_clicking {
        os::click_up(is_left); // never leave a button stuck down on shutdown
    }
}

fn key_poll_loop(
    sig: Arc<EngineSignals>,
    cfg: Arc<Mutex<EngineConfig>>,
    tx: Sender<ToggleReq>,
    ctx: egui::Context,
) {
    let mut left_was = true; // require a release before the first edge counts
    let mut right_was = true;
    let mut panic_was = true;
    let mut last_focus = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);

    while sig.running.load(Ordering::Relaxed) {
        let snap = { cfg.lock().unwrap().clone() };

        sig.suspend_left.store(
            snap.left.suspend_vk != 0 && os::key_held(snap.left.suspend_vk),
            Ordering::Relaxed,
        );
        sig.suspend_right.store(
            snap.right.suspend_vk != 0 && os::key_held(snap.right.suspend_vk),
            Ordering::Relaxed,
        );

        edge(snap.left.hotkey_vk, &mut left_was, || {
            let _ = tx.send(ToggleReq::Left);
            ctx.request_repaint();
        });
        edge(snap.right.hotkey_vk, &mut right_was, || {
            let _ = tx.send(ToggleReq::Right);
            ctx.request_repaint();
        });
        if snap.panic_vk != 0 {
            let p = os::key_held(snap.panic_vk);
            if p && !panic_was {
                sig.panic.store(true, Ordering::Relaxed);
                let _ = tx.send(ToggleReq::PanicAll);
                ctx.request_repaint();
            }
            panic_was = p;
        }

        if last_focus.elapsed() >= Duration::from_millis(150) {
            sig.mc_focused
                .store(os::is_minecraft_active(), Ordering::Relaxed);
            sig.any_focused
                .store(os::any_window_focused(), Ordering::Relaxed);
            last_focus = Instant::now();
        }

        thread::sleep(Duration::from_millis(10));
    }
}

fn edge(vk: i32, was: &mut bool, on_press: impl FnOnce()) {
    if vk == 0 {
        *was = true;
        return;
    }
    let p = os::key_held(vk);
    if p && !*was {
        on_press();
    }
    *was = p;
}
