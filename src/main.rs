#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, RichText, Sense,
    Stroke, StrokeKind, Vec2,
};

const BG: Color32 = Color32::from_rgb(10, 13, 8);
const PANEL: Color32 = Color32::from_rgb(16, 20, 13);
const PANEL2: Color32 = Color32::from_rgb(22, 27, 17);
const LINE: Color32 = Color32::from_rgb(38, 45, 29);
const TRACK: Color32 = Color32::from_rgb(42, 47, 36);
const TXT: Color32 = Color32::from_rgb(238, 243, 230);
const MUT: Color32 = Color32::from_rgb(142, 150, 138);
const KNOB_OFF: Color32 = Color32::from_rgb(207, 212, 198);

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 600.0])
            .with_min_inner_size([660.0, 540.0])
            .with_decorations(false)
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Citron Clicker Premium",
        options,
        Box::new(|cc| {
            setup_style(&cc.egui_ctx);
            Ok(Box::new(CitronApp::new()))
        }),
    )
}

fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(TXT);
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = PANEL2;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    style.visuals = v;
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.slider_width = 200.0;
    ctx.set_global_style(style);
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Left,
    Right,
    Sounds,
    Settings,
}

#[derive(PartialEq, Clone, Copy)]
enum Pack {
    Soft,
    Click,
    Mechanical,
    Pop,
    Custom,
}

struct Clicker {
    enabled: bool,
    min_cps: f32,
    max_cps: f32,
    suspend: String,
    hotkey: String,
    avoid_gui: bool,
    humanize: bool,
    jitter: bool,
    only_ingame: bool,
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
}

impl CitronApp {
    fn new() -> Self {
        let histo = (0..46)
            .map(|i| {
                let x = i as f32;
                let base = (x * 0.5).sin() * 0.5 + 0.5;
                let n = ((x * 12.9898).sin() * 43758.5453).fract().abs();
                0.28 + (base * 0.5 + n * 0.4).min(1.0) * 0.7
            })
            .collect();
        Self {
            tab: Tab::Left,
            left: Clicker {
                enabled: true,
                min_cps: 13.0,
                max_cps: 19.0,
                suspend: "Mouse 5".into(),
                hotkey: "V".into(),
                avoid_gui: true,
                humanize: true,
                jitter: false,
                only_ingame: true,
            },
            right: Clicker {
                enabled: false,
                min_cps: 8.0,
                max_cps: 12.0,
                suspend: "None".into(),
                hotkey: "None".into(),
                avoid_gui: true,
                humanize: true,
                jitter: false,
                only_ingame: true,
            },
            right_hold: true,
            sounds_on: true,
            pack: Pack::Soft,
            volume: 70.0,
            separate: false,
            pitch_var: true,
            accent: Color32::from_rgb(216, 242, 74),
            start_system: false,
            tray: true,
            autoupdate: true,
            histo,
        }
    }
}

fn card() -> egui::Frame {
    egui::Frame::default()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(16))
}

fn row_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(14, 12))
}

fn cap(text: &str, color: Color32) -> RichText {
    RichText::new(text).size(11.0).color(color).extra_letter_spacing(1.4)
}

fn toggle(ui: &mut egui::Ui, on: &mut bool, accent: Color32) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(44.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let p = ui.painter();
    let track = if *on { accent } else { TRACK };
    p.rect_filled(rect, CornerRadius::same(12), track);
    let r = rect.height() * 0.5 - 3.0;
    let cx = if *on {
        rect.right() - r - 3.0
    } else {
        rect.left() + r + 3.0
    };
    let knob = if *on { BG } else { KNOB_OFF };
    p.circle_filled(Pos2::new(cx, rect.center().y), r, knob);
    resp
}

fn dual_range(ui: &mut egui::Ui, min: &mut f32, max: &mut f32, accent: Color32) {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click_and_drag());
    let (lo, hi) = (1.0_f32, 20.0_f32);
    let to_x = |v: f32| rect.left() + (v - lo) / (hi - lo) * rect.width();

    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let val = (lo + t * (hi - lo)).round();
            if (pos.x - to_x(*min)).abs() <= (pos.x - to_x(*max)).abs() {
                *min = val.min(*max);
            } else {
                *max = val.max(*min);
            }
        }
    }

    let y = rect.center().y;
    let p = ui.painter();
    let track = Rect::from_min_max(Pos2::new(rect.left(), y - 2.0), Pos2::new(rect.right(), y + 2.0));
    p.rect_filled(track, CornerRadius::same(2), TRACK);
    let fill = Rect::from_min_max(Pos2::new(to_x(*min), y - 2.0), Pos2::new(to_x(*max), y + 2.0));
    p.rect_filled(fill, CornerRadius::same(2), accent);
    p.circle_filled(Pos2::new(to_x(*min), y), 9.0, accent);
    p.circle_filled(Pos2::new(to_x(*max), y), 9.0, accent);
}

fn histogram(ui: &mut egui::Ui, histo: &[f32], accent: Color32) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::hover());
    let n = histo.len().max(1);
    let gap = 2.0;
    let bw = ((rect.width() - gap * (n as f32 - 1.0)) / n as f32).max(1.0);
    let p = ui.painter();
    for (i, h) in histo.iter().enumerate() {
        let x = rect.left() + i as f32 * (bw + gap);
        let bh = rect.height() * h;
        let bar = Rect::from_min_max(Pos2::new(x, rect.bottom() - bh), Pos2::new(x + bw, rect.bottom()));
        p.rect_filled(bar, CornerRadius::same(1), accent.linear_multiply(0.45 + 0.55 * h));
    }
}

fn pill(ui: &mut egui::Ui, label: &str, value: &str, accent: Color32) {
    egui::Frame::default()
        .fill(PANEL2)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(12.5).color(MUT));
                ui.label(RichText::new(value).size(12.5).color(accent));
            });
        });
}

fn option_row(ui: &mut egui::Ui, title: &str, sub: &str, add: impl FnOnce(&mut egui::Ui)) {
    row_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add_space(1.0);
                ui.label(RichText::new(title).size(13.5).color(TXT));
                if !sub.is_empty() {
                    ui.label(RichText::new(sub).size(11.5).color(MUT));
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), add);
        });
    });
}

fn chip(ui: &mut egui::Ui, label: &str, accent: Color32) {
    egui::Frame::default()
        .fill(PANEL2)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(12.5).color(accent));
        });
}

fn win_btn(ui: &mut egui::Ui, glyph: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    let col = if resp.hovered() { TXT } else { MUT };
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(17.0), col);
    resp
}

impl eframe::App for CitronApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [
            BG.r() as f32 / 255.0,
            BG.g() as f32 / 255.0,
            BG.b() as f32 / 255.0,
            1.0,
        ]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.title_bar(&ctx, ui);
        self.tab_bar(ui);
        egui::Frame::default()
            .inner_margin(Margin::same(18))
            .show(ui, |ui| match self.tab {
                Tab::Left => self.clicker_tab(ui, true),
                Tab::Right => self.clicker_tab(ui, false),
                Tab::Sounds => self.sounds_tab(ui),
                Tab::Settings => self.settings_tab(ui),
            });
    }
}

impl CitronApp {
    fn title_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        egui::Frame::default()
            .inner_margin(Margin {
                left: 18,
                right: 14,
                top: 12,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let drag = ui
                        .scope(|ui| {
                            ui.label(RichText::new("citron").size(19.0).color(TXT));
                            crown_badge(ui, self.accent);
                        })
                        .response
                        .interact(Sense::click_and_drag());
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if win_btn(ui, "\u{00D7}").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if win_btn(ui, "\u{2013}").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        ui.add_space(6.0);
                        status_pill(ui);
                    });
                });
            });
        line(ui);
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let tabs = [
            (Tab::Left, "LEFT CLICK"),
            (Tab::Right, "RIGHT CLICK"),
            (Tab::Sounds, "SOUNDS"),
            (Tab::Settings, "SETTINGS"),
        ];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let tw = ui.available_width() / tabs.len() as f32;
            for (t, label) in tabs {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(tw, 46.0), Sense::click());
                let active = self.tab == t;
                let col = if active { self.accent } else { MUT };
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(12.5),
                    col,
                );
                if active {
                    let u = Rect::from_min_max(
                        Pos2::new(rect.left() + tw * 0.28, rect.bottom() - 2.0),
                        Pos2::new(rect.right() - tw * 0.28, rect.bottom()),
                    );
                    ui.painter().rect_filled(u, CornerRadius::same(0), self.accent);
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
        let ck = if is_left { &mut self.left } else { &mut self.right };
        let title = if is_left { "LEFT CLICKER" } else { "RIGHT CLICKER" };

        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(cap(title, accent));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    toggle(ui, &mut ck.enabled, accent);
                });
            });
            ui.add_space(8.0);
            ui.columns(2, |c| {
                c[0].label(cap("MIN CPS", MUT));
                c[0].label(
                    RichText::new(format!("{}", ck.min_cps as i32))
                        .size(44.0)
                        .color(accent)
                        .strong(),
                );
                c[1].with_layout(Layout::top_down(Align::Max), |ui| {
                    ui.label(cap("MAX CPS", MUT));
                    ui.label(
                        RichText::new(format!("{}", ck.max_cps as i32))
                            .size(44.0)
                            .color(accent)
                            .strong(),
                    );
                });
            });
            ui.add_space(6.0);
            histogram(ui, &histo, accent);
            dual_range(ui, &mut ck.min_cps, &mut ck.max_cps, accent);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("1").size(12.0).color(MUT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("20").size(12.0).color(MUT));
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let avg = (ck.min_cps + ck.max_cps) / 2.0;
                        pill(ui, "Avg cps", &format!("{:.1}", avg), accent);
                    });
                });
            });
        });

        ui.add_space(12.0);

        if !is_left {
            option_row(ui, "Hold mode", "Hold right button to place / eat", |ui| {
                toggle(ui, &mut self.right_hold, accent);
            });
            ui.add_space(10.0);
        }

        if is_left {
            two_col(
                ui,
                |ui| option_row(ui, "Suspend key", "Hold to pause", |ui| {
                    chip(ui, ck.suspend.as_str(), accent)
                }),
                |ui| option_row(ui, "Toggle hotkey", "Click to rebind", |ui| {
                    chip(ui, ck.hotkey.as_str(), accent)
                }),
            );
            ui.add_space(10.0);
            two_col(
                ui,
                |ui| option_row(ui, "Avoid GUI", "Pause in menus", |ui| {
                    toggle(ui, &mut ck.avoid_gui, accent);
                }),
                |ui| option_row(ui, "Humanize", "Natural timing + bursts", |ui| {
                    toggle(ui, &mut ck.humanize, accent);
                }),
            );
            ui.add_space(10.0);
            two_col(
                ui,
                |ui| option_row(ui, "Jitter", "Aim shake", |ui| {
                    toggle(ui, &mut ck.jitter, accent);
                }),
                |ui| option_row(ui, "Only in-game", "Active when focused", |ui| {
                    toggle(ui, &mut ck.only_ingame, accent);
                }),
            );
        } else {
            two_col(
                ui,
                |ui| option_row(ui, "Toggle hotkey", "Click to rebind", |ui| {
                    chip(ui, ck.hotkey.as_str(), accent)
                }),
                |ui| option_row(ui, "Avoid GUI", "Pause in menus", |ui| {
                    toggle(ui, &mut ck.avoid_gui, accent);
                }),
            );
        }

        ui.add_space(16.0);
        self.footer(ui);
    }

    fn sounds_tab(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent;
        option_row(ui, "Click sounds", "Play a sound on every click", |ui| {
            toggle(ui, &mut self.sounds_on, accent);
        });
        ui.add_space(12.0);
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(cap("SOUND PACK", MUT));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for (p, name) in [
                    (Pack::Soft, "Soft"),
                    (Pack::Click, "Click"),
                    (Pack::Mechanical, "Mechanical"),
                    (Pack::Pop, "Pop"),
                    (Pack::Custom, "Load custom .wav"),
                ] {
                    let sel = self.pack == p;
                    let col = if sel { accent } else { MUT };
                    let r = egui::Frame::default()
                        .fill(if sel { PANEL2 } else { PANEL })
                        .stroke(Stroke::new(1.0, if sel { accent } else { LINE }))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.label(RichText::new(name).size(13.0).color(col));
                        });
                    if r.response.interact(Sense::click()).clicked() {
                        self.pack = p;
                    }
                }
            });
        });
        ui.add_space(12.0);
        row_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Volume").size(13.5).color(TXT));
                    ui.label(RichText::new(format!("{}%", self.volume as i32)).size(12.0).color(MUT));
                });
                ui.add_space(4.0);
                ui.add(egui::Slider::new(&mut self.volume, 0.0..=100.0).show_value(false));
            });
        });
        ui.add_space(10.0);
        two_col(
            ui,
            |ui| option_row(ui, "Separate press / release", "Two-stage sound", |ui| {
                toggle(ui, &mut self.separate, accent);
            }),
            |ui| option_row(ui, "Pitch variance", "Less robotic", |ui| {
                toggle(ui, &mut self.pitch_var, accent);
            }),
        );
        ui.add_space(16.0);
        self.footer(ui);
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent;
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(cap("ACCENT", MUT));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for c in [
                    Color32::from_rgb(216, 242, 74),
                    Color32::from_rgb(93, 214, 240),
                    Color32::from_rgb(255, 122, 209),
                    Color32::from_rgb(155, 140, 255),
                    Color32::from_rgb(255, 139, 74),
                ] {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
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
            |ui| option_row(ui, "Start with system", "", |ui| {
                toggle(ui, &mut self.start_system, accent);
            }),
            |ui| option_row(ui, "Minimize to tray", "", |ui| {
                toggle(ui, &mut self.tray, accent);
            }),
        );
        ui.add_space(10.0);
        two_col(
            ui,
            |ui| option_row(ui, "Panic key", "Instantly disable all", |ui| chip(ui, "F8", accent)),
            |ui| option_row(ui, "Auto-update", "", |ui| {
                toggle(ui, &mut self.autoupdate, accent);
            }),
        );
        ui.add_space(16.0);
        self.footer(ui);
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent;
        ui.horizontal(|ui| {
            let save = ui.add_sized(
                Vec2::new(ui.available_width() - 62.0, 46.0),
                egui::Button::new(
                    RichText::new("SAVE CONFIGURATION")
                        .size(14.0)
                        .color(BG)
                        .strong(),
                )
                .fill(accent)
                .corner_radius(CornerRadius::same(12)),
            );
            if save.clicked() {
                // TODO: persist config
            }
            let exp = ui.add_sized(
                Vec2::new(52.0, 46.0),
                egui::Button::new(RichText::new("\u{2191}").size(18.0).color(accent))
                    .fill(PANEL)
                    .stroke(Stroke::new(1.0, LINE))
                    .corner_radius(CornerRadius::same(12)),
            );
            let _ = exp;
        });
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("made by center2055").size(12.0).color(MUT));
        });
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

fn crown_badge(ui: &mut egui::Ui, accent: Color32) {
    egui::Frame::default()
        .fill(accent)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(9, 4))
        .show(ui, |ui| {
            ui.label(RichText::new("PREMIUM").size(10.5).color(BG).strong().extra_letter_spacing(1.4));
        });
}

fn status_pill(ui: &mut egui::Ui) {
    egui::Frame::default()
        .fill(PANEL2)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, MUT);
                ui.label(RichText::new("WAITING FOR MC").size(12.0).color(MUT).extra_letter_spacing(1.2));
            });
        });
}
