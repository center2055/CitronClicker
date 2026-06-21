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
    pub mc_running: AtomicBool,
    pub any_focused: AtomicBool,
    pub running: AtomicBool,
    /// Set by the UI while a key-rebind is armed: fully pauses the engine so the key/button
    /// being bound does not also toggle or click.
    pub capturing: AtomicBool,
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
    pub suspend_vk: i32,
    pub hotkey_vk: i32,
    pub is_left: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub struct AudioConfig {
    pub enabled: bool,
    pub volume: f32,
    pub pitch_var: bool,
    pub separate: bool,
}

#[derive(Clone, PartialEq)]
pub struct EngineConfig {
    pub left: ClickerSnap,
    pub right: ClickerSnap,
    pub panic_vk: i32,
    pub audio: AudioConfig,
}

pub enum ToggleReq {
    Left,
    Right,
}

pub struct EngineHandle {
    pub signals: Arc<EngineSignals>,
    pub config: Arc<Mutex<EngineConfig>>,
    pub toggle_rx: Receiver<ToggleReq>,
    joins: Vec<JoinHandle<()>>,
    hook_tid: u32,
}

impl EngineHandle {
    pub fn start(
        ctx: egui::Context,
        initial: EngineConfig,
        audio: Option<crate::audio::AudioHandle>,
    ) -> Self {
        os::begin_timer_period();
        let hook_tid = os::start_input_hook();

        let signals = Arc::new(EngineSignals {
            suspend_left: AtomicBool::new(false),
            suspend_right: AtomicBool::new(false),
            panic: AtomicBool::new(false),
            mc_focused: AtomicBool::new(false),
            mc_running: AtomicBool::new(false),
            any_focused: AtomicBool::new(false),
            running: AtomicBool::new(true),
            capturing: AtomicBool::new(false),
        });
        let config = Arc::new(Mutex::new(initial));
        let (tx, rx) = channel::<ToggleReq>();

        let mut joins = Vec::new();
        for is_left in [true, false] {
            let s = signals.clone();
            let c = config.clone();
            let a = audio.clone();
            joins.push(thread::spawn(move || clicker_loop(is_left, s, c, a)));
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

fn clicker_loop(
    is_left: bool,
    sig: Arc<EngineSignals>,
    cfg: Arc<Mutex<EngineConfig>>,
    audio: Option<crate::audio::AudioHandle>,
) {
    let mut rng = Rng::seeded(if is_left { 0xA17 } else { 0xB29 });
    let mut hd = HumanizedDelay::new();
    let mut jit = SmoothJitter::new();
    let mut sched = ClickScheduler::new();
    let mut was_clicking = false;

    while sig.running.load(Ordering::Relaxed) {
        let (snap, audio_cfg) = {
            let c = cfg.lock().unwrap();
            (
                if is_left { c.left.clone() } else { c.right.clone() },
                c.audio,
            )
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
        // Avoid GUI's cursor check only makes sense in-game (cursor is hidden during play, shown
        // in menus). In "any window" mode the cursor is always visible, so don't let it block.
        let gui_block = snap.avoid_gui && snap.only_ingame && os::cursor_visible();
        let phys = os::physical_button_held(is_left);
        let should = snap.enabled
            && !sig.panic.load(Ordering::Relaxed)
            && !sig.capturing.load(Ordering::Relaxed)
            && !os::foreground_is_self() // never click into our own window, regardless of settings
            && focus_ok
            && !gui_block
            && !suspend
            && phys;

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
            if audio_cfg.separate {
                play_click(&audio, audio_cfg);
            }
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
            play_click(&audio, audio_cfg);
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

fn play_click(audio: &Option<crate::audio::AudioHandle>, cfg: AudioConfig) {
    if let (Some(a), true) = (audio, cfg.enabled) {
        let speed = if cfg.pitch_var { pitch_jitter() } else { 1.0 };
        a.play(crate::audio::PlayParams {
            volume: cfg.volume,
            speed,
        });
    }
}

fn pitch_jitter() -> f32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    1.0 + ((n % 1000) as f32 / 1000.0 - 0.5) * 0.12
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
        // While a rebind is armed, don't toggle/suspend on the key being bound. Holding the
        // *_was flags true means the just-bound (still-held) key requires a release before it fires.
        if sig.capturing.load(Ordering::Relaxed) {
            left_was = true;
            right_was = true;
            panic_was = true;
            sig.suspend_left.store(false, Ordering::Relaxed);
            sig.suspend_right.store(false, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(10));
            continue;
        }

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
                sig.panic.store(true, Ordering::Relaxed); // stop clicking instantly
                ctx.send_viewport_cmd(egui::ViewportCommand::Close); // panic = quit the app
                ctx.request_repaint();
            }
            panic_was = p;
        }

        if last_focus.elapsed() >= Duration::from_millis(150) {
            sig.mc_focused
                .store(os::is_minecraft_active(), Ordering::Relaxed);
            sig.mc_running
                .store(os::is_minecraft_running(), Ordering::Relaxed);
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
