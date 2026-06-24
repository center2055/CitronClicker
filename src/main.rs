#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, RichText, Sense,
    Stroke, StrokeKind, Vec2,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::Ordering;

mod audio;
mod engine;
mod os;
mod tray;
mod update;

use engine::{ClickerSnap, EngineConfig, EngineHandle, ToggleReq};

const BG: Color32 = Color32::from_rgb(10, 13, 8);
const PANEL: Color32 = Color32::from_rgb(20, 25, 15);
const PANEL2: Color32 = Color32::from_rgb(30, 36, 22);
const LINE: Color32 = Color32::from_rgb(38, 45, 29);
const WIN_BORDER: Color32 = Color32::from_rgb(52, 61, 37);
const TRACK: Color32 = Color32::from_rgb(42, 47, 36);
const TXT: Color32 = Color32::from_rgb(238, 243, 230);
const MUT: Color32 = Color32::from_rgb(140, 148, 136);
const KNOB_OFF: Color32 = Color32::from_rgb(207, 212, 198);
const LOGO_H: f32 = 30.0; // title-bar logo height in points
const BTC_ADDR: &str = "bc1q0gvnvrr0a64kpxylwgqkvlp5gt4c48jqxy9jy2";

mod ic {
    pub const MOUSE: char = '\u{e28e}';
    pub const VOLUME: char = '\u{e1ab}';
    pub const SETTINGS: char = '\u{e154}';
    pub const MINUS: char = '\u{e11c}';
    pub const CLOSE: char = '\u{e1b2}';
    pub const PAUSE: char = '\u{e12e}';
    pub const KEYBOARD: char = '\u{e284}';
    pub const EYE_OFF: char = '\u{e0bb}';
    pub const SPARKLES: char = '\u{e412}';
    pub const ACTIVITY: char = '\u{e038}';
    pub const GAMEPAD: char = '\u{e0df}';
    pub const UPLOAD: char = '\u{e19e}';
    pub const SLIDERS: char = '\u{e29a}';
    pub const SPLIT: char = '\u{e440}';
    pub const WAVEFORM: char = '\u{e55b}';
    pub const PLAY: char = '\u{e13c}';
    pub const PALETTE: char = '\u{e1dd}';
    pub const POWER: char = '\u{e140}';
    pub const TRAY: char = '\u{e42c}';
    pub const ZAP: char = '\u{e1b4}';
    pub const REFRESH: char = '\u{e145}';
    pub const CHART: char = '\u{e2a2}';
    pub const DISC: char = '\u{e494}';
}

fn main() -> eframe::Result {
    // single instance only — if one's already running, surface it and bail
    if !os::acquire_single_instance() {
        os::focus_existing_window();
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 730.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_icon(Arc::new(load_icon())),
        // fixed-size app. eframe always *loads* persisted geometry (persist_window only gates
        // saving), so force the size in the window_builder hook — it runs last and overrides the
        // restored geometry. config still auto-saves separately.
        persist_window: false,
        window_builder: Some(Box::new(|vb| {
            vb.with_inner_size([720.0, 730.0])
                .with_min_inner_size([720.0, 730.0])
                .with_max_inner_size([720.0, 730.0])
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Citron v2",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            setup_style(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx); // svg loader for the about icons
            Ok(Box::new(CitronApp::new(cc)))
        }),
    )
}

fn load_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/citron_fruit.png"))
        .expect("logo")
        .to_rgba8();
    let (w, h) = (img.width(), img.height());
    let side = w.max(h);
    let mut canvas = image::RgbaImage::new(side, side);
    let (ox, oy) = ((side - w) / 2, (side - h) / 2);
    for (x, y, p) in img.enumerate_pixels() {
        canvas.put_pixel(ox + x, oy + y, image::Rgba([216, 242, 74, p[3]]));
    }
    egui::IconData {
        rgba: canvas.into_raw(),
        width: side,
        height: side,
    }
}

/// build the tray icon at exactly `px` square (the shell's small-icon size) so it draws 1:1
/// instead of getting rescaled from our full-size source. same lime silhouette as the window icon,
/// lanczos3-fit and centered.
fn load_tray_icon(px: u32) -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory(include_bytes!("../assets/citron_fruit.png"))
        .expect("icon")
        .to_rgba8();
    // recolor to the brand lime via the source alpha (matches the window icon)
    let mut lime = image::RgbaImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        lime.put_pixel(x, y, image::Rgba([216, 242, 74, p[3]]));
    }
    // fit into px×px keeping aspect, then center on a transparent square
    let scale = px as f32 / img.width().max(img.height()) as f32;
    let nw = ((img.width() as f32 * scale).round() as u32).max(1);
    let nh = ((img.height() as f32 * scale).round() as u32).max(1);
    let resized = image::imageops::resize(&lime, nw, nh, image::imageops::FilterType::Lanczos3);
    let mut canvas = image::RgbaImage::new(px, px);
    image::imageops::overlay(&mut canvas, &resized, ((px - nw) / 2) as i64, ((px - nh) / 2) as i64);
    (canvas.into_raw(), px, px)
}

/// bake the wordmark to a texture at the exact device-pixel size it'll draw at, so it's 1:1
/// (crisp like a glyph) instead of mipmap-downscaled (soft). rebaked when the dpi changes.
fn bake_logo(ctx: &egui::Context, ppp: f32) -> (egui::TextureHandle, f32) {
    let img = image::load_from_memory(include_bytes!("../assets/citron_logo.png"))
        .expect("logo")
        .to_rgba8();
    let aspect = img.width() as f32 / img.height() as f32;
    let h_px = (LOGO_H * ppp).round().max(1.0) as u32;
    let w_px = ((h_px as f32) * aspect).round().max(1.0) as u32;
    let resized = image::imageops::resize(&img, w_px, h_px, image::imageops::FilterType::Lanczos3);
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [resized.width() as usize, resized.height() as usize],
        resized.as_raw(),
    );
    // no mipmaps — already at display res, samples 1:1
    let tex = ctx.load_texture("citron_logo", color, egui::TextureOptions::LINEAR);
    (tex, aspect)
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "poppins".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/Poppins-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "poppins_semibold".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/Poppins-SemiBold.ttf"
        ))),
    );
    fonts.font_data.insert(
        "lucide".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/lucide.ttf"
        ))),
    );
    let prop = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    prop.insert(0, "poppins".into());
    prop.push("lucide".into());
    fonts.families.insert(
        egui::FontFamily::Name("semibold".into()),
        vec!["poppins_semibold".into(), "lucide".into()],
    );
    fonts
        .families
        .insert(egui::FontFamily::Name("icons".into()), vec!["lucide".into()]);
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(TXT);
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = PANEL2;
    v.selection.bg_fill = Color32::from_rgb(70, 84, 30);
    v.widgets.noninteractive.bg_stroke = Stroke::NONE;
    style.visuals = v;
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    ctx.set_global_style(style);
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Left,
    Right,
    Sounds,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum BindSlot {
    Suspend,
    Hotkey,
}

#[derive(Clone, Copy, PartialEq)]
enum RebindTarget {
    Clicker { is_left: bool, slot: BindSlot },
    Panic,
}

fn default_panic_key() -> String {
    "F8".into()
}

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize)]
enum Pack {
    Default,
    Custom,
}

fn default_cps() -> f32 {
    13.0
}

fn default_jitter_strength() -> i32 {
    2
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Clicker {
    enabled: bool,
    min_cps: f32,
    max_cps: f32,
    #[serde(default = "default_cps")]
    cps: f32,
    suspend: String,
    hotkey: String,
    avoid_gui: bool,
    humanize: bool,
    jitter: bool,
    #[serde(default = "default_jitter_strength")]
    jitter_strength: i32,
    only_ingame: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Config {
    left: Clicker,
    right: Clicker,
    right_hold: bool,
    sounds_on: bool,
    pack: Pack,
    volume: f32,
    separate: bool,
    pitch_var: bool,
    accent: [u8; 3],
    start_system: bool,
    tray: bool,
    autoupdate: bool,
    #[serde(default = "default_panic_key")]
    panic_key: String,
    #[serde(default)]
    custom_wav: Option<std::path::PathBuf>,
}

struct CitronApp {
    tab: Tab,
    left: Clicker,
    right: Clicker,
    right_hold: bool,
    sounds_on: bool,
    pack: Pack,
    volume: f32,
    separate: bool,
    pitch_var: bool,
    accent: Color32,
    start_system: bool,
    tray: bool,
    autoupdate: bool,
    histo: Vec<f32>,
    logo: egui::TextureHandle,
    logo_aspect: f32,
    logo_ppp: f32,
    saved: Option<Config>,
    engine: EngineHandle,
    last_pushed: Option<EngineConfig>,
    humanize_warn: Option<bool>,
    rebind: Option<RebindTarget>,
    rebind_armed_at: u64,
    panic_key: String,
    audio: Option<audio::AudioHandle>,
    last_pack: Pack,
    custom_wav: Option<std::path::PathBuf>,
    tray_mgr: Option<tray::TrayManager>,
    hidden: bool,
    quitting: bool,
    tray_menu: Option<egui::Pos2>,
    tray_menu_focused: bool,
    menu_focus_pending: bool,
    btc_copied: Option<std::time::Instant>,
    tray_applied: Option<bool>,
    autostart_applied: Option<bool>,
}

fn snap_of(ck: &Clicker, is_left: bool) -> ClickerSnap {
    ClickerSnap {
        enabled: ck.enabled,
        min_cps: ck.min_cps,
        max_cps: ck.max_cps,
        cps: ck.cps,
        avoid_gui: ck.avoid_gui,
        humanize: ck.humanize,
        jitter: ck.jitter,
        jitter_intensity: ck.jitter_strength,
        only_ingame: ck.only_ingame,
        suspend_vk: engine::vk_from_name(&ck.suspend),
        hotkey_vk: engine::vk_from_name(&ck.hotkey),
        is_left,
    }
}

impl CitronApp {
    fn new(cc: &eframe::CreationContext) -> Self {
        let logo_ppp = cc.egui_ctx.pixels_per_point();
        let (logo, logo_aspect) = bake_logo(&cc.egui_ctx, logo_ppp);

        let histo = (0..46)
            .map(|i| {
                let x = i as f32;
                let base = (x * 0.5).sin() * 0.5 + 0.5;
                let n = ((x * 12.9898).sin() * 43758.5453).fract().abs();
                0.28 + (base * 0.5 + n * 0.4).min(1.0) * 0.7
            })
            .collect();

        let left = Clicker {
            enabled: true,
            min_cps: 13.0,
            max_cps: 19.0,
            cps: 16.0,
            suspend: "Mouse 5".into(),
            hotkey: "V".into(),
            avoid_gui: true,
            humanize: true,
            jitter: false,
            jitter_strength: 2,
            only_ingame: true,
        };
        let right = Clicker {
            enabled: false,
            min_cps: 8.0,
            max_cps: 12.0,
            cps: 10.0,
            suspend: "None".into(),
            hotkey: "None".into(),
            avoid_gui: true,
            humanize: true,
            jitter: false,
            jitter_strength: 2,
            only_ingame: true,
        };
        let audio = audio::AudioHandle::spawn();
        let engine = EngineHandle::start(
            cc.egui_ctx.clone(),
            EngineConfig {
                left: snap_of(&left, true),
                right: snap_of(&right, false),
                panic_vk: engine::vk_from_name("F8"),
                audio: engine::AudioConfig {
                    enabled: true,
                    volume: 0.70,
                    pitch_var: true,
                    separate: false,
                },
            },
            audio.clone(),
        );
        let (tray_rgba, tray_w, tray_h) = load_tray_icon(os::small_icon_px());
        let tray_mgr = tray::TrayManager::new(tray_rgba, tray_w, tray_h);
        let mut app = Self {
            tab: Tab::Left,
            left,
            right,
            right_hold: true,
            sounds_on: true,
            pack: Pack::Default,
            volume: 70.0,
            separate: false,
            pitch_var: true,
            accent: Color32::from_rgb(216, 242, 74),
            start_system: false,
            tray: true,
            autoupdate: true,
            histo,
            logo,
            logo_aspect,
            logo_ppp,
            saved: None,
            engine,
            last_pushed: None,
            humanize_warn: None,
            rebind: None,
            rebind_armed_at: 0,
            panic_key: "F8".into(),
            audio,
            last_pack: Pack::Default,
            custom_wav: None,
            tray_mgr,
            hidden: false,
            quitting: false,
            tray_menu: None,
            tray_menu_focused: false,
            menu_focus_pending: false,
            btc_copied: None,
            tray_applied: None,
            autostart_applied: None,
        };
        if let Some(storage) = cc.storage {
            if let Some(cfg) = eframe::get_value::<Config>(storage, "config") {
                app.apply(cfg);
            }
        }
        // clear any leftover from a prior update, then (if enabled) silently check for a newer
        // release in the background — it stages over this exe for the next launch.
        update::startup_cleanup();
        if app.autoupdate {
            update::spawn_check();
        }
        app
    }

    fn snapshot(&self) -> Config {
        Config {
            left: self.left.clone(),
            right: self.right.clone(),
            right_hold: self.right_hold,
            sounds_on: self.sounds_on,
            pack: self.pack,
            volume: self.volume,
            separate: self.separate,
            pitch_var: self.pitch_var,
            accent: [self.accent.r(), self.accent.g(), self.accent.b()],
            start_system: self.start_system,
            tray: self.tray,
            autoupdate: self.autoupdate,
            panic_key: self.panic_key.clone(),
            custom_wav: self.custom_wav.clone(),
        }
    }

    fn apply(&mut self, c: Config) {
        self.left = c.left;
        self.right = c.right;
        self.right_hold = c.right_hold;
        self.sounds_on = c.sounds_on;
        self.pack = c.pack;
        self.volume = c.volume;
        self.separate = c.separate;
        self.pitch_var = c.pitch_var;
        self.accent = Color32::from_rgb(c.accent[0], c.accent[1], c.accent[2]);
        self.start_system = c.start_system;
        self.tray = c.tray;
        self.autoupdate = c.autoupdate;
        self.panic_key = c.panic_key;
        self.custom_wav = c.custom_wav;
        self.last_pack = self.pack;
        // reload a saved custom sound, fall back to default if it's gone/bad
        if self.pack == Pack::Custom {
            let loaded = self.custom_wav.as_ref().and_then(|p| std::fs::read(p).ok());
            match loaded {
                Some(bytes) if audio::validate_wav(&bytes).is_ok() => {
                    if let Some(a) = &self.audio {
                        a.set_custom(bytes);
                    }
                }
                _ => {
                    self.pack = Pack::Default;
                    self.custom_wav = None;
                    self.last_pack = Pack::Default;
                }
            }
        }
    }

    fn to_engine_config(&self) -> EngineConfig {
        EngineConfig {
            left: snap_of(&self.left, true),
            right: snap_of(&self.right, false),
            panic_vk: engine::vk_from_name(&self.panic_key),
            audio: engine::AudioConfig {
                enabled: self.sounds_on,
                volume: self.volume / 100.0,
                pitch_var: self.pitch_var,
                separate: self.separate,
            },
        }
    }

    fn sync_engine(&mut self) {
        while let Ok(req) = self.engine.toggle_rx.try_recv() {
            match req {
                ToggleReq::Left => self.left.enabled = !self.left.enabled,
                ToggleReq::Right => self.right.enabled = !self.right.enabled,
            }
        }
        let ec = self.to_engine_config();
        if self.last_pushed.as_ref() != Some(&ec) {
            *self.engine.config.lock().unwrap() = ec.clone();
            self.last_pushed = Some(ec);
        }
    }

    fn ensure_logo(&mut self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        if (ppp - self.logo_ppp).abs() > 0.01 {
            let (tex, aspect) = bake_logo(ctx, ppp);
            self.logo = tex;
            self.logo_aspect = aspect;
            self.logo_ppp = ppp;
        }
    }

    fn sync_system(&mut self, ctx: &egui::Context) {
        // intercept os-level closes (alt+f4, taskbar "close window") while close-to-tray is on:
        // cancel and tuck into the tray instead. `quitting` is the escape hatch the tray's quit
        // menu sets so a real exit isn't swallowed here.
        if !self.quitting
            && self.tray
            && self.tray_mgr.is_some()
            && ctx.input(|i| i.viewport().close_requested())
        {
            if self.engine.signals.panic.load(Ordering::Relaxed) {
                // panic = force-quit, let the close through instead of hiding
                self.quitting = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.hidden = true;
            }
        }
        // autostart: apply on first frame and whenever the toggle changes
        if self.autostart_applied != Some(self.start_system) {
            os::set_autostart(self.start_system);
            self.autostart_applied = Some(self.start_system);
        }
        // tray icon visibility follows the toggle
        if self.tray_applied != Some(self.tray) {
            if let Some(t) = &self.tray_mgr {
                t.set_visible(self.tray);
            }
            if !self.tray && self.hidden {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                self.hidden = false;
            }
            self.tray_applied = Some(self.tray);
        }
        // handle tray clicks (poll returns an owned action so no borrow is held)
        let action = self.tray_mgr.as_ref().and_then(|t| t.poll());
        match action {
            Some(tray::TrayAction::Show) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.hidden = false;
                self.tray_menu = None;
            }
            Some(tray::TrayAction::Menu { x, y }) => {
                // physical cursor px -> egui logical points
                let ppp = ctx.pixels_per_point();
                self.tray_menu = Some(egui::pos2(x as f32 / ppp, y as f32 / ppp));
                self.tray_menu_focused = false;
                self.menu_focus_pending = true;
                ctx.request_repaint();
            }
            None => {}
        }
        // keep polling the tray while its icon is up — the window may be hidden, minimized, or
        // behind the game, and tray menu clicks must still be handled
        if self.tray && self.tray_mgr.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    // our own themed tray menu. it's a viewport, kept alive only while it's open OR while the
    // window is hidden in the tray — so dismissing it never destroys a window under a hidden root
    // (that exited the app before). when the main window is in normal use the popup doesn't exist
    // at all, so its per-frame visibility churn can't interfere with the main window's input.
    fn tray_menu_popup(&mut self, ctx: &egui::Context) {
        let needed = self.tray
            && self.tray_mgr.is_some()
            && (self.tray_menu.is_some() || self.hidden);
        if !needed {
            return;
        }
        let open = self.tray_menu.is_some();
        // tray sits bottom-right: pop up-left of the cursor with a small gap so the cursor isn't
        // sitting on an item. parked off-screen while closed.
        let pos = match self.tray_menu {
            Some(c) => egui::pos2((c.x - 188.0).max(4.0), (c.y - 84.0).max(4.0)),
            None => egui::pos2(-2000.0, -2000.0),
        };
        let accent = self.accent;
        let grab = self.menu_focus_pending;
        let (mut show, mut quit, mut esc, mut focused_now) = (false, false, false, false);
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("tray_menu"),
            egui::ViewportBuilder::default()
                .with_position(pos)
                .with_inner_size([188.0, 76.0])
                .with_decorations(false)
                .with_resizable(false)
                .with_always_on_top()
                .with_taskbar(false)
                .with_visible(false),
            |ctx, _| {
                // toggle visibility/position each frame instead of recreating the window
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(open));
                if !open {
                    return;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                if grab {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                focused_now = ctx.input(|i| i.focused);
                esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                let area = ctx.content_rect();
                egui::Area::new(egui::Id::new("tray_menu_area"))
                    .fixed_pos(area.min)
                    .show(ctx, |ui| {
                        egui::Frame::default()
                            .fill(PANEL)
                            .stroke(Stroke::new(1.0, WIN_BORDER))
                            .inner_margin(Margin::same(6))
                            .show(ui, |ui| {
                                ui.set_width(area.width() - 12.0);
                                ui.spacing_mut().item_spacing.y = 4.0;
                                if tray_menu_item(ui, ic::TRAY, "Show Citron v2", accent) {
                                    show = true;
                                }
                                if tray_menu_item(ui, ic::POWER, "Quit", accent) {
                                    quit = true;
                                }
                            });
                    });
            },
        );
        if !open {
            return;
        }
        self.menu_focus_pending = false;
        if focused_now {
            self.tray_menu_focused = true;
        }
        let lost_focus = self.tray_menu_focused && !focused_now;
        if show {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.hidden = false;
            self.tray_menu = None;
            self.tray_menu_focused = false;
        } else if quit {
            self.engine.shutdown();
            self.tray_mgr = None;
            std::process::exit(0);
        } else if esc || lost_focus {
            self.tray_menu = None;
            self.tray_menu_focused = false;
        } else {
            ctx.request_repaint();
        }
    }
}

impl Drop for CitronApp {
    fn drop(&mut self) {
        self.engine.shutdown();
    }
}

fn semibold(text: &str, size: f32, color: Color32) -> RichText {
    RichText::new(text)
        .size(size)
        .color(color)
        .family(egui::FontFamily::Name("semibold".into()))
}

fn iconrt(ch: char, size: f32, color: Color32) -> RichText {
    RichText::new(ch)
        .size(size)
        .color(color)
        .family(egui::FontFamily::Name("icons".into()))
}

fn paint_icon(p: &egui::Painter, center: Pos2, ch: char, size: f32, color: Color32) {
    p.text(
        center,
        Align2::CENTER_CENTER,
        ch,
        FontId::new(size, egui::FontFamily::Name("icons".into())),
        color,
    );
}

fn cap(text: &str, color: Color32) -> RichText {
    RichText::new(text).size(11.0).color(color).extra_letter_spacing(1.3)
}

fn card() -> egui::Frame {
    egui::Frame::default()
        .fill(PANEL)
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(16))
}

fn row_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(PANEL)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(13, 11))
}

fn icon_box(ui: &mut egui::Ui, ch: char, accent: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(9), PANEL2);
    paint_icon(ui.painter(), rect.center(), ch, 17.0, accent);
}

// straight rgb lerp (colours here are opaque)
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

// one row of the themed tray menu: icon + label, accent-highlighted on hover
fn tray_menu_item(ui: &mut egui::Ui, glyph: char, label: &str, accent: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    let hov = resp.hovered();
    if hov {
        ui.painter().rect_filled(rect, CornerRadius::same(6), PANEL2);
    }
    let col = if hov { accent } else { TXT };
    paint_icon(ui.painter(), Pos2::new(rect.left() + 16.0, rect.center().y), glyph, 15.0, col);
    let g = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::new(13.0, egui::FontFamily::Name("semibold".into())),
        col,
    );
    ui.painter()
        .galley(Pos2::new(rect.left() + 32.0, rect.center().y - g.size().y / 2.0), g, col);
    resp.clicked()
}

fn toggle(ui: &mut egui::Ui, on: &mut bool, accent: Color32) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(44.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    // glide the knob + crossfade the colours instead of snapping
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(12), lerp_color(TRACK, accent, t));
    let r = rect.height() * 0.5 - 3.0;
    let off_x = rect.left() + r + 3.0;
    let on_x = rect.right() - r - 3.0;
    let cx = off_x + (on_x - off_x) * t;
    p.circle_filled(Pos2::new(cx, rect.center().y), r, lerp_color(KNOB_OFF, BG, t));
    resp
}

// cps without a trailing .0 — whole numbers show "13", decimals show "13.5"
fn fmt_cps(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i32)
    } else {
        format!("{:.1}", v)
    }
}

fn dual_range(ui: &mut egui::Ui, min: &mut f32, max: &mut f32, accent: Color32) {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click_and_drag());
    let (lo, hi) = (1.0_f32, 20.0_f32);
    let to_x = |v: f32| rect.left() + (v - lo) / (hi - lo) * rect.width();

    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let val = ((lo + t * (hi - lo)) * 10.0).round() / 10.0; // 0.1 steps: decimals + smooth
            if (pos.x - to_x(*min)).abs() <= (pos.x - to_x(*max)).abs() {
                *min = val.min(*max);
            } else {
                *max = val.max(*min);
            }
        }
    }

    let y = rect.center().y;
    let p = ui.painter();
    p.rect_filled(
        Rect::from_min_max(Pos2::new(rect.left(), y - 2.0), Pos2::new(rect.right(), y + 2.0)),
        CornerRadius::same(2),
        TRACK,
    );
    p.rect_filled(
        Rect::from_min_max(Pos2::new(to_x(*min), y - 2.0), Pos2::new(to_x(*max), y + 2.0)),
        CornerRadius::same(2),
        accent,
    );
    for v in [*min, *max] {
        p.circle_filled(Pos2::new(to_x(v), y), 9.0, accent);
        p.circle_filled(Pos2::new(to_x(v), y), 4.0, BG);
    }
}

fn single_slider(ui: &mut egui::Ui, value: &mut f32, min: f32, max: f32, accent: Color32) {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click_and_drag());
    let to_x = |v: f32| rect.left() + (v - min) / (max - min) * rect.width();
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            *value = min + t * (max - min);
        }
    }
    let y = rect.center().y;
    let p = ui.painter();
    p.rect_filled(
        Rect::from_min_max(Pos2::new(rect.left(), y - 2.0), Pos2::new(rect.right(), y + 2.0)),
        CornerRadius::same(2),
        TRACK,
    );
    let hx = to_x(*value);
    p.rect_filled(
        Rect::from_min_max(Pos2::new(rect.left(), y - 2.0), Pos2::new(hx, y + 2.0)),
        CornerRadius::same(2),
        accent,
    );
    p.circle_filled(Pos2::new(hx, y), 9.0, accent);
    p.circle_filled(Pos2::new(hx, y), 4.0, BG);
}

fn histogram(ui: &mut egui::Ui, histo: &[f32], accent: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::hover());
    let n = histo.len().max(1);
    let gap = 2.0;
    let bw = ((rect.width() - gap * (n as f32 - 1.0)) / n as f32).max(1.0);
    let p = ui.painter();
    for (i, h) in histo.iter().enumerate() {
        let x = rect.left() + i as f32 * (bw + gap);
        let bh = rect.height() * h;
        p.rect_filled(
            Rect::from_min_max(Pos2::new(x, rect.bottom() - bh), Pos2::new(x + bw, rect.bottom())),
            CornerRadius::same(1),
            accent.linear_multiply(0.4 + 0.6 * h),
        );
    }
}

fn avg_pill(ui: &mut egui::Ui, avg_value: f32, accent: Color32) {
    let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
    let avg = fmt_cps(avg_value);
    let p = ui.painter();
    let g_lbl = p.layout_no_wrap("Avg cps".to_string(), FontId::proportional(12.5), MUT);
    let g_val = p.layout_no_wrap(
        avg,
        FontId::new(12.5, egui::FontFamily::Name("semibold".into())),
        accent,
    );
    let (lbl_sz, val_sz) = (g_lbl.size(), g_val.size());
    let (pad, gap, icon_w) = (12.0, 6.0, 15.0);
    let content_w = icon_w + gap + lbl_sz.x + gap + val_sz.x;
    let pill = Rect::from_center_size(row.center(), Vec2::new(content_w + pad * 2.0, 26.0));
    p.rect_filled(pill, CornerRadius::same(9), PANEL2);
    let cy = row.center().y;
    let mut x = pill.left() + pad;
    paint_icon(p, Pos2::new(x + 7.0, cy), ic::CHART, 13.0, MUT);
    x += icon_w + gap;
    p.galley(Pos2::new(x, cy - lbl_sz.y / 2.0), g_lbl, MUT);
    x += lbl_sz.x + gap;
    p.galley(Pos2::new(x, cy - val_sz.y / 2.0), g_val, accent);
}

// a clickable chip: official svg icon + label, opens `url` in the browser
fn link_chip(ui: &mut egui::Ui, src: egui::ImageSource<'static>, label: &str, url: &str, accent: Color32) {
    let r = egui::Frame::default()
        .fill(PANEL2)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(11, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(src).fit_to_exact_size(Vec2::splat(15.0)).tint(accent));
                ui.add_space(7.0);
                ui.label(RichText::new(label).size(12.5).color(accent));
            });
        });
    let resp = r.response.interact(Sense::click());
    if resp.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
}

// small text button (used for the BTC copy action)
fn mini_btn(ui: &mut egui::Ui, label: &str, accent: Color32) -> bool {
    let font = FontId::new(11.5, egui::FontFamily::Name("semibold".into()));
    let g = ui.painter().layout_no_wrap(label.to_string(), font, accent);
    let (rect, resp) = ui.allocate_exact_size(g.size() + Vec2::new(18.0, 10.0), Sense::click());
    let bg = if resp.hovered() { accent } else { PANEL2 };
    let fg = if resp.hovered() { BG } else { accent };
    ui.painter().rect_filled(rect, CornerRadius::same(7), bg);
    let g2 = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::new(11.5, egui::FontFamily::Name("semibold".into())),
        fg,
    );
    ui.painter().galley(rect.center() - g2.size() / 2.0, g2, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn modal_btn(ui: &mut egui::Ui, label: &str, color: Color32, filled: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
    let (fill, txt) = if filled { (color, BG) } else { (PANEL2, color) };
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    let g = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::new(13.0, egui::FontFamily::Name("semibold".into())),
        txt,
    );
    ui.painter()
        .galley(rect.center() - g.size() / 2.0, g, txt);
    resp.clicked()
}

fn option_row(
    ui: &mut egui::Ui,
    icon: char,
    title: &str,
    sub: &str,
    accent: Color32,
    add: impl FnOnce(&mut egui::Ui),
) {
    row_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            icon_box(ui, icon, accent);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(if sub.is_empty() { 7.0 } else { 1.0 });
                ui.label(RichText::new(title).size(13.5).color(TXT));
                if !sub.is_empty() {
                    ui.label(RichText::new(sub).size(11.0).color(MUT));
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), add);
        });
    });
}

/// clickable keybind pill. shows "press a key…" while listening. returns clicked.
fn bind_chip(ui: &mut egui::Ui, label: &str, listening: bool, accent: Color32) -> bool {
    let txt = if listening { "Press a key\u{2026}" } else { label };
    let font = FontId::new(12.5, egui::FontFamily::Name("semibold".into()));
    let galley = ui.painter().layout_no_wrap(txt.to_string(), font.clone(), accent);
    let size = galley.size() + Vec2::new(24.0, 12.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    // resting is borderless; hover/listening get an accent ring
    let (fill, stroke, txt_col) = if listening {
        (accent, Stroke::new(1.0, accent), BG)
    } else if resp.hovered() {
        (PANEL2, Stroke::new(1.0, accent), accent)
    } else {
        (PANEL2, Stroke::NONE, accent)
    };
    ui.painter()
        .rect(rect, CornerRadius::same(8), fill, stroke, StrokeKind::Inside);
    let g = ui.painter().layout_no_wrap(txt.to_string(), font, txt_col);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, txt_col);
    resp.clicked()
}

fn listening_for(rb: Option<RebindTarget>, is_left: bool, slot: BindSlot) -> bool {
    matches!(rb, Some(RebindTarget::Clicker { is_left: l, slot: s }) if l == is_left && s == slot)
}

fn key_name(k: egui::Key) -> Option<&'static str> {
    use egui::Key::*;
    Some(match k {
        Escape => "None",
        Space => "Space",
        Tab => "Tab",
        A => "A", B => "B", C => "C", D => "D", E => "E", F => "F", G => "G", H => "H",
        I => "I", J => "J", K => "K", L => "L", M => "M", N => "N", O => "O", P => "P",
        Q => "Q", R => "R", S => "S", T => "T", U => "U", V => "V", W => "W", X => "X",
        Y => "Y", Z => "Z",
        Num0 => "0", Num1 => "1", Num2 => "2", Num3 => "3", Num4 => "4",
        Num5 => "5", Num6 => "6", Num7 => "7", Num8 => "8", Num9 => "9",
        F1 => "F1", F2 => "F2", F3 => "F3", F4 => "F4", F5 => "F5", F6 => "F6",
        F7 => "F7", F8 => "F8", F9 => "F9", F10 => "F10", F11 => "F11", F12 => "F12",
        _ => return None,
    })
}

fn button_name(b: egui::PointerButton) -> &'static str {
    use egui::PointerButton::*;
    match b {
        Primary => "Left Click",
        Secondary => "Right Click",
        Middle => "Middle Click",
        Extra1 => "Mouse 4",
        Extra2 => "Mouse 5",
    }
}

fn two_col(ui: &mut egui::Ui, a: impl FnOnce(&mut egui::Ui), b: impl FnOnce(&mut egui::Ui)) {
    ui.columns(2, |c| {
        a(&mut c[0]);
        b(&mut c[1]);
    });
}

fn line(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(0), LINE);
}

impl eframe::App for CitronApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let snap = self.snapshot();
        eframe::set_value(storage, "config", &snap);
        self.saved = Some(snap);
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(1)
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.ensure_logo(&ctx);
        let win = ctx.content_rect();
        ui.painter().rect(
            win.shrink(0.5),
            CornerRadius::same(16),
            BG,
            Stroke::new(1.0, WIN_BORDER),
            StrokeKind::Inside,
        );

        // drag the window from empty space. added before the panels so it sits under every widget
        // — clicks on toggles/sliders/tabs hit those first; a drag on empty space falls through
        // here and moves the window.
        let win_drag = ui.interact(win, egui::Id::new("window_drag"), Sense::click_and_drag());
        if win_drag.drag_started() && self.rebind.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        // rebind capture: while armed, grab the first key/mouse press and map to a name
        if let Some(target) = self.rebind {
            let armed_at = self.rebind_armed_at;
            let this_frame = ctx.cumulative_frame_nr();
            let mut captured: Option<String> = ctx.input(|i| {
                for ev in &i.events {
                    match ev {
                        egui::Event::Key { key, pressed: true, repeat: false, .. } => {
                            if let Some(name) = key_name(*key) {
                                return Some(name.to_string());
                            }
                        }
                        egui::Event::PointerButton { button, pressed: true, .. } => {
                            if this_frame == armed_at {
                                continue; // ignore the click that armed the rebind
                            }
                            return Some(button_name(*button).to_string());
                        }
                        _ => {}
                    }
                }
                None
            });
            // caps lock has no egui key event — poll it directly (skip the arming frame)
            if captured.is_none() && this_frame != armed_at && os::key_held(0x14) {
                captured = Some("Caps Lock".to_string());
            }
            if let Some(name) = captured {
                match target {
                    RebindTarget::Clicker { is_left, slot } => {
                        let ck = if is_left { &mut self.left } else { &mut self.right };
                        match slot {
                            BindSlot::Suspend => ck.suspend = name,
                            BindSlot::Hotkey => ck.hotkey = name,
                        }
                    }
                    RebindTarget::Panic => self.panic_key = name,
                }
                self.rebind = None;
            }
            ctx.request_repaint();
        }

        egui::Panel::top("titlebar")
            .frame(egui::Frame::default().fill(BG))
            .show_separator_line(false)
            .show_inside(ui, |ui| self.title_bar(&ctx, ui));
        egui::Panel::top("tabs")
            .frame(egui::Frame::default().fill(BG))
            .show_separator_line(false)
            .show_inside(ui, |ui| self.tab_bar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(BG).inner_margin(Margin::same(18)))
            .show_inside(ui, |ui| match self.tab {
                Tab::Left => self.clicker_tab(ui, true),
                Tab::Right => self.clicker_tab(ui, false),
                Tab::Sounds => self.sounds_tab(ui),
                Tab::Settings => self.settings_tab(ui),
            });

        self.humanize_modal(&ctx);
        self.sync_engine();

        // pack changed -> tell the audio thread which sound to load (custom is set at pick time)
        if self.pack != self.last_pack {
            if self.pack == Pack::Default {
                if let Some(a) = &self.audio {
                    a.set_default();
                }
            }
            self.last_pack = self.pack;
        }

        // pause the engine while a rebind is armed so the bound key can't toggle/click
        self.engine
            .signals
            .capturing
            .store(self.rebind.is_some(), Ordering::Relaxed);

        self.sync_system(&ctx);
        self.tray_menu_popup(&ctx);

        if self.saved.as_ref() != Some(&self.snapshot()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(1100));
        }
    }
}

impl CitronApp {
    fn title_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let active = self.engine.signals.mc_running.load(Ordering::Relaxed);
        egui::Frame::default()
            .inner_margin(Margin {
                left: 18,
                right: 14,
                top: 13,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let lh = LOGO_H;
                    let (lr, _) =
                        ui.allocate_exact_size(Vec2::new(lh * self.logo_aspect, lh), Sense::hover());
                    ui.painter().image(
                        self.logo.id(),
                        lr,
                        Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                        self.accent,
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // with "close to tray" on, the x tucks into the tray instead of quitting
                        // (exit via the tray's quit menu). minimize always does a normal taskbar
                        // minimize.
                        let to_tray = self.tray && self.tray_mgr.is_some();
                        if win_btn(ui, ic::CLOSE).clicked() {
                            if to_tray {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                                self.hidden = true;
                            } else {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        if win_btn(ui, ic::MINUS).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        ui.add_space(6.0);
                        status_pill(ui, active, self.accent);
                    });
                });
            });
        line(ui);
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let tabs = [
            (Tab::Left, "LEFT CLICK", ic::MOUSE),
            (Tab::Right, "RIGHT CLICK", ic::MOUSE),
            (Tab::Sounds, "SOUNDS", ic::VOLUME),
            (Tab::Settings, "SETTINGS", ic::SETTINGS),
        ];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let tw = ui.available_width() / tabs.len() as f32;
            for (t, label, icon) in tabs {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(tw, 48.0), Sense::click());
                let active = self.tab == t;
                let col = if active { self.accent } else { MUT };
                let galley = ui.painter().layout_no_wrap(
                    label.to_string(),
                    FontId::new(12.5, egui::FontFamily::Name("semibold".into())),
                    col,
                );
                let total = 18.0 + 8.0 + galley.size().x;
                let sx = rect.center().x - total / 2.0;
                let y = rect.center().y;
                paint_icon(ui.painter(), Pos2::new(sx + 9.0, y), icon, 16.0, col);
                ui.painter()
                    .galley(Pos2::new(sx + 26.0, y - galley.size().y / 2.0), galley, col);
                if active {
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            Pos2::new(rect.left() + tw * 0.26, rect.bottom() - 2.0),
                            Pos2::new(rect.right() - tw * 0.26, rect.bottom()),
                        ),
                        CornerRadius::same(0),
                        self.accent,
                    );
                }
                if resp.clicked() {
                    self.tab = t;
                }
            }
        });
        line(ui);
    }

    fn clicker_tab(&mut self, ui: &mut egui::Ui, is_left: bool) {
        let accent = self.accent;
        let histo = self.histo.clone();
        let rebind = self.rebind;
        let mut warn = false;
        let mut arm_susp = false;
        let mut arm_hot = false;
        let ck = if is_left { &mut self.left } else { &mut self.right };
        let title = if is_left { "LEFT CLICKER" } else { "RIGHT CLICKER" };

        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                ui.label(cap(title, accent));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    toggle(ui, &mut ck.enabled, accent);
                });
            });
            ui.add_space(8.0);
            if ck.humanize {
                ui.columns(2, |c| {
                    c[0].label(cap("MIN CPS", MUT));
                    c[0].label(semibold(&fmt_cps(ck.min_cps), 40.0, accent));
                    c[1].with_layout(Layout::top_down(Align::Max), |ui| {
                        ui.label(cap("MAX CPS", MUT));
                        ui.label(semibold(&fmt_cps(ck.max_cps), 40.0, accent));
                    });
                });
                histogram(ui, &histo, accent);
                ui.add_space(2.0);
                dual_range(ui, &mut ck.min_cps, &mut ck.max_cps, accent);
                ui.add_space(8.0);
                avg_pill(ui, (ck.min_cps + ck.max_cps) / 2.0, accent);
            } else {
                ui.vertical_centered(|ui| {
                    ui.label(cap("CPS", MUT));
                    ui.label(semibold(&fmt_cps(ck.cps), 40.0, accent));
                });
                histogram(ui, &histo, accent);
                ui.add_space(2.0);
                single_slider(ui, &mut ck.cps, 1.0, 20.0, accent);
                ck.cps = (ck.cps * 10.0).round() / 10.0; // 0.1 steps
                ui.add_space(8.0);
                let _ = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
            }
        });

        ui.add_space(12.0);

        two_col(
            ui,
            |ui| {
                option_row(ui, ic::PAUSE, "Suspend key", "Hold to pause", accent, |ui| {
                    if bind_chip(
                        ui,
                        ck.suspend.as_str(),
                        listening_for(rebind, is_left, BindSlot::Suspend),
                        accent,
                    ) {
                        arm_susp = true;
                    }
                })
            },
            |ui| {
                option_row(ui, ic::KEYBOARD, "Toggle hotkey", "Click to rebind", accent, |ui| {
                    if bind_chip(
                        ui,
                        ck.hotkey.as_str(),
                        listening_for(rebind, is_left, BindSlot::Hotkey),
                        accent,
                    ) {
                        arm_hot = true;
                    }
                })
            },
        );
        ui.add_space(10.0);
        let before_h = ck.humanize;
        two_col(
            ui,
            |ui| {
                option_row(ui, ic::EYE_OFF, "Avoid GUI", "Pause in menus", accent, |ui| {
                    toggle(ui, &mut ck.avoid_gui, accent);
                })
            },
            |ui| {
                option_row(ui, ic::SPARKLES, "Humanize", "Natural timing + bursts", accent, |ui| {
                    toggle(ui, &mut ck.humanize, accent);
                })
            },
        );
        if before_h && !ck.humanize {
            ck.humanize = true; // stay on until confirmed in the modal
            warn = true;
        }
        ui.add_space(10.0);
        two_col(
            ui,
            |ui| {
                // jitter on -> a compact strength slider sits left of the toggle (no extra row, so
                // the fixed-height window never overflows)
                option_row(ui, ic::ACTIVITY, "Jitter", "Aim shake", accent, |ui| {
                    toggle(ui, &mut ck.jitter, accent);
                    if ck.jitter {
                        ui.add_space(8.0);
                        ui.label(semibold(&format!("{}", ck.jitter_strength), 13.0, accent));
                        ui.add_space(6.0);
                        let mut v = ck.jitter_strength as f32;
                        single_slider(ui, &mut v, 1.0, 10.0, accent);
                        ck.jitter_strength = v.round() as i32;
                    }
                })
            },
            |ui| {
                option_row(ui, ic::GAMEPAD, "Only in-game", "Off = any window", accent, |ui| {
                    toggle(ui, &mut ck.only_ingame, accent);
                })
            },
        );

        if warn {
            self.humanize_warn = Some(is_left);
        }
        if arm_susp {
            self.rebind = Some(RebindTarget::Clicker { is_left, slot: BindSlot::Suspend });
            self.rebind_armed_at = ui.ctx().cumulative_frame_nr();
        } else if arm_hot {
            self.rebind = Some(RebindTarget::Clicker { is_left, slot: BindSlot::Hotkey });
            self.rebind_armed_at = ui.ctx().cumulative_frame_nr();
        }
    }

    fn sounds_tab(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent;
        option_row(ui, ic::VOLUME, "Click sounds", "Play a sound on every click", accent, |ui| {
            toggle(ui, &mut self.sounds_on, accent);
        });
        ui.add_space(12.0);
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(cap("SOUND PACK", MUT));
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                for (p, name, glyph) in [
                    (Pack::Default, "Default", ic::DISC),
                    (Pack::Custom, "Load custom .wav", ic::UPLOAD),
                ] {
                    let sel = self.pack == p;
                    let col = if sel { accent } else { MUT };
                    let r = egui::Frame::default()
                        .fill(if sel { PANEL2 } else { PANEL })
                        .stroke(if sel { Stroke::new(1.0, accent) } else { Stroke::NONE })
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(iconrt(glyph, 14.0, col));
                                ui.label(RichText::new(name).size(13.0).color(col));
                            });
                        });
                    if r.response.interact(Sense::click()).clicked() {
                        match p {
                            Pack::Default => self.pack = Pack::Default,
                            Pack::Custom => {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("WAV audio", &["wav"])
                                    .set_title("Choose click sound")
                                    .pick_file()
                                {
                                    match std::fs::read(&path) {
                                        Ok(bytes) if audio::validate_wav(&bytes).is_ok() => {
                                            if let Some(a) = &self.audio {
                                                a.set_custom(bytes);
                                            }
                                            self.custom_wav = Some(path);
                                            self.pack = Pack::Custom;
                                            self.last_pack = Pack::Custom;
                                        }
                                        _ => {} // unreadable / not a decodable wav: keep current
                                    }
                                }
                            }
                        }
                    }
                }
            });
        });
        ui.add_space(12.0);
        row_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                icon_box(ui, ic::SLIDERS, accent);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Volume").size(13.5).color(TXT));
                        ui.label(
                            semibold(&format!("{}%", self.volume as i32), 12.0, MUT),
                        );
                    });
                    ui.add_space(6.0);
                    single_slider(ui, &mut self.volume, 0.0, 100.0, accent);
                });
            });
        });
        ui.add_space(10.0);
        two_col(
            ui,
            |ui| {
                option_row(ui, ic::SPLIT, "Separate press / release", "Two-stage sound", accent, |ui| {
                    toggle(ui, &mut self.separate, accent);
                })
            },
            |ui| {
                option_row(ui, ic::WAVEFORM, "Pitch variance", "Less robotic", accent, |ui| {
                    toggle(ui, &mut self.pitch_var, accent);
                })
            },
        );
        ui.add_space(12.0);
        if accent_button(ui, ic::PLAY, "Preview click", accent, PANEL2).clicked() {
            if let Some(a) = &self.audio {
                a.preview();
            }
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent;
        let panic_listening = self.rebind == Some(RebindTarget::Panic);
        let panic_label = self.panic_key.clone();
        let mut arm_panic = false;
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(iconrt(ic::PALETTE, 14.0, MUT));
                ui.label(cap("ACCENT", MUT));
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                for c in [
                    Color32::from_rgb(216, 242, 74),
                    Color32::from_rgb(93, 214, 240),
                    Color32::from_rgb(255, 122, 209),
                    Color32::from_rgb(155, 140, 255),
                    Color32::from_rgb(255, 139, 74),
                ] {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
                    ui.painter().rect_filled(rect, CornerRadius::same(8), c);
                    if self.accent == c {
                        ui.painter().rect_stroke(
                            rect.expand(3.0),
                            CornerRadius::same(11),
                            Stroke::new(2.0, c),
                            StrokeKind::Outside,
                        );
                    }
                    if resp.clicked() {
                        self.accent = c;
                    }
                }
            });
        });
        ui.add_space(12.0);
        two_col(
            ui,
            |ui| {
                option_row(ui, ic::POWER, "Start with system", "", accent, |ui| {
                    toggle(ui, &mut self.start_system, accent);
                })
            },
            |ui| {
                option_row(ui, ic::TRAY, "Close to tray", "", accent, |ui| {
                    toggle(ui, &mut self.tray, accent);
                })
            },
        );
        ui.add_space(10.0);
        two_col(
            ui,
            |ui| {
                option_row(ui, ic::ZAP, "Panic key", "", accent, |ui| {
                    if bind_chip(ui, &panic_label, panic_listening, accent) {
                        arm_panic = true;
                    }
                })
            },
            |ui| {
                option_row(ui, ic::REFRESH, "Auto-update", "", accent, |ui| {
                    toggle(ui, &mut self.autoupdate, accent);
                })
            },
        );
        if arm_panic {
            self.rebind = Some(RebindTarget::Panic);
            self.rebind_armed_at = ui.ctx().cumulative_frame_nr();
        }

        ui.add_space(12.0);
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(cap("ABOUT", MUT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).size(12.0).color(MUT));
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                link_chip(
                    ui,
                    egui::include_image!("../assets/icons/github.svg"),
                    "GitHub",
                    "https://github.com/center2055/CitronClicker",
                    accent,
                );
                link_chip(
                    ui,
                    egui::include_image!("../assets/icons/discord.svg"),
                    "Discord",
                    "https://discord.gg/y3MVspPzKQ",
                    accent,
                );
                link_chip(
                    ui,
                    egui::include_image!("../assets/icons/kofi.svg"),
                    "Ko-fi",
                    "https://ko-fi.com/center2055",
                    accent,
                );
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(egui::include_image!("../assets/icons/bitcoin.svg"))
                        .fit_to_exact_size(Vec2::splat(15.0))
                        .tint(MUT),
                );
                ui.add_space(7.0);
                ui.label(RichText::new(BTC_ADDR).size(10.5).color(MUT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let copied = self
                        .btc_copied
                        .is_some_and(|t| t.elapsed().as_secs_f32() < 1.5);
                    if mini_btn(ui, if copied { "Copied!" } else { "Copy" }, accent) {
                        ui.ctx().copy_text(BTC_ADDR.to_string());
                        self.btc_copied = Some(std::time::Instant::now());
                    }
                    if copied {
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(300));
                    }
                });
            });
        });
    }

    fn humanize_modal(&mut self, ctx: &egui::Context) {
        let is_left = match self.humanize_warn {
            Some(v) => v,
            None => return,
        };
        let accent = self.accent;
        let mut keep = false;
        let mut disable = false;
        // egui::Modal owns the backdrop + keeps its content the top interactable layer, so a
        // click on the dim area can't bury the buttons (the old two-Area scrim+card could).
        let resp = egui::Modal::new(egui::Id::new("hz_modal"))
            .frame(
                egui::Frame::default()
                    .fill(PANEL)
                    .stroke(Stroke::new(1.0, accent))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.horizontal(|ui| {
                    ui.label(iconrt(ic::ZAP, 18.0, accent));
                    ui.add_space(2.0);
                    ui.label(semibold("Disable humanization?", 16.0, TXT));
                });
                ui.add_space(10.0);
                ui.label(
                    RichText::new(
                        "A perfectly periodic clicker is more effective \u{2014} but far \
                         easier to detect. Some servers' anti-cheat can flag the regular \
                         timing and ban your account. Humanized timing is strongly \
                         recommended.",
                    )
                    .size(12.5)
                    .color(MUT),
                );
                ui.add_space(16.0);
                ui.columns(2, |c| {
                    if modal_btn(&mut c[0], "Keep humanized", accent, true) {
                        keep = true;
                    }
                    if modal_btn(&mut c[1], "Disable anyway", MUT, false) {
                        disable = true;
                    }
                });
            });
        // click the dim backdrop or press escape = cancel (keep humanized)
        if resp.should_close() {
            keep = true;
        }
        if keep {
            self.humanize_warn = None;
        }
        if disable {
            if is_left {
                self.left.humanize = false;
            } else {
                self.right.humanize = false;
            }
            self.humanize_warn = None;
        }
    }
}

fn accent_button(
    ui: &mut egui::Ui,
    glyph: char,
    label: &str,
    accent: Color32,
    fill: Color32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
    ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::new(13.5, egui::FontFamily::Name("semibold".into())),
        accent,
    );
    let total = 18.0 + 8.0 + galley.size().x;
    let sx = rect.center().x - total / 2.0;
    let y = rect.center().y;
    paint_icon(ui.painter(), Pos2::new(sx + 9.0, y), glyph, 16.0, accent);
    ui.painter()
        .galley(Pos2::new(sx + 26.0, y - galley.size().y / 2.0), galley, accent);
    resp
}

fn win_btn(ui: &mut egui::Ui, glyph: char) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    let col = if resp.hovered() { TXT } else { MUT };
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(7), PANEL2);
    }
    paint_icon(ui.painter(), rect.center(), glyph, 16.0, col);
    resp
}

fn status_pill(ui: &mut egui::Ui, active: bool, accent: Color32) {
    let (label, col) = if active {
        ("INJECTED", accent)
    } else {
        ("WAITING FOR MC", MUT)
    };
    egui::Frame::default()
        .fill(PANEL2)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new(label).size(12.0).color(col).extra_letter_spacing(1.0));
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, col);
            });
        });
}
