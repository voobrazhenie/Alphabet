//! The console, the telemetry panel, the frame-rate graph and the benchmark report.
//!
//! Same controls as the page, in the same three folding groups, plus a fourth for
//! the things only a native build has: which graphics API is live, how frames are
//! presented, and the named templates on disk.

use crate::app::App;
use crate::camera;
use crate::state::{
    Backend, Present, AA_NAME, BND_NAME, OBJ_NAME, PATH_NAME, RES_NAME, STEPS_MAX, UM_PER_UNIT,
};
use egui::{Align2, Color32, FontId, RichText, Stroke, Vec2};

const FIBER: Color32 = Color32::from_rgb(0x2F, 0xD9, 0xC0);
const DIM: Color32 = Color32::from_rgb(0x6A, 0x7A, 0x88);
const CHROME: Color32 = Color32::from_rgb(0xC6, 0xD3, 0xDC);
const MEAN: Color32 = Color32::from_rgb(0xFF, 0xCE, 0x4A);
const PANEL: Color32 = Color32::from_rgba_premultiplied(6, 10, 16, 214);

const GRAPH_MS: f64 = 5000.0;
const G_TOPS: [f32; 4] = [60.0, 120.0, 240.0, 480.0];

pub fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, Color32::from_rgba_premultiplied(47, 217, 192, 56));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, CHROME);
    v.widgets.inactive.bg_fill = Color32::from_rgba_premultiplied(47, 217, 192, 26);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgba_premultiplied(47, 217, 192, 20);
    v.widgets.hovered.bg_fill = Color32::from_rgba_premultiplied(47, 217, 192, 60);
    v.widgets.active.bg_fill = Color32::from_rgba_premultiplied(47, 217, 192, 90);
    v.selection.bg_fill = Color32::from_rgba_premultiplied(47, 217, 192, 120);
    v.selection.stroke = Stroke::new(1.0, Color32::BLACK);
    v.window_shadow = egui::epaint::Shadow::NONE;
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.slider_width = 176.0;
        s.spacing.item_spacing = Vec2::new(6.0, 5.0);
        s.spacing.interact_size.y = 20.0;
    });
}

fn lbl(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(9.0).color(DIM).monospace());
}

/// A row of mutually exclusive buttons, the console's `.seg`.
fn seg(ui: &mut egui::Ui, cur: &mut usize, opts: &[&str]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for (i, name) in opts.iter().enumerate() {
            if ui.selectable_label(*cur == i, RichText::new(*name).size(11.0)).clicked()
                && *cur != i
            {
                *cur = i;
                changed = true;
            }
        }
    });
    changed
}

/// A row of independent on/off buttons, the console's `.transport`.
fn toggles(ui: &mut egui::Ui, flags: &mut [bool], names: &[&str]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for (i, name) in names.iter().enumerate() {
            if ui.selectable_label(flags[i], RichText::new(*name).size(11.0)).clicked() {
                flags[i] = !flags[i];
                changed = true;
            }
        }
    });
    changed
}

fn slider(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    fmt: impl Fn(f32) -> String,
) {
    ui.horizontal(|ui| {
        lbl(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(fmt(*v)).size(10.0).color(FIBER).monospace());
        });
    });
    ui.add(egui::Slider::new(v, range).step_by(step).show_value(false));
}

impl App {
    pub(crate) fn ui(&mut self, ctx: &egui::Context) {
        // During a run the console and the graph stand down, as they do on the page:
        // they would cost frame time, and a different amount of it on every machine.
        if self.hud && !self.bench.on {
            self.telemetry_panel(ctx);
            self.console(ctx);
        }
        self.report_window(ctx);
    }

    fn telemetry_panel(&mut self, ctx: &egui::Context) {
        let now = (std::time::Instant::now() - self.start).as_secs_f64() * 1000.0;
        let fps_now = 1000.0 / self.ema.max(1.0);
        let avg = self.hist.mean_fps(now, GRAPH_MS);
        egui::Window::new("telemetry")
            .title_bar(false)
            .resizable(false)
            .anchor(Align2::LEFT_BOTTOM, [16.0, -16.0])
            .fixed_size([248.0, 96.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let cell = |ui: &mut egui::Ui, k: &str, v: String, unit: &str| {
                        ui.vertical(|ui| {
                            lbl(ui, k);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(v).size(15.0).color(FIBER).monospace());
                                if !unit.is_empty() {
                                    ui.label(RichText::new(unit).size(9.0).color(DIM));
                                }
                            });
                        });
                    };
                    cell(ui, "Now", fmt_fps(fps_now), "fps");
                    ui.add_space(14.0);
                    cell(ui, "Avg 5s", fmt_fps(avg), "fps");
                    ui.add_space(14.0);
                    cell(ui, "Render", format!("{}\u{d7}{}", self.rw, self.rh), "");
                });
                self.graph(ui, now, avg);
            });
    }

    /// The frame-rate graph, drawn the way the page draws it: the x axis is time,
    /// not sample count, so the trace scrolls at a steady rate instead of stretching
    /// and squeezing as the frame rate changes.
    fn graph(&mut self, ui: &mut egui::Ui, now: f64, avg: f32) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(120.0), 62.0),
            egui::Sense::hover(),
        );
        let p = ui.painter_at(rect);
        let gut = 22.0;
        let (x0, x1) = (rect.left() + gut, rect.right() - 1.0);
        let (y0, y1) = (rect.top() + 3.0, rect.bottom() - 3.0);
        let ph = y1 - y0;
        if x1 <= x0 || ph <= 4.0 {
            return;
        }

        let n = (((x1 - x0) / 2.0) as usize).clamp(24, 200);
        let mut cols = vec![-1.0f32; n];
        let mut filled = 0;
        for i in 1..=self.hist.len {
            let k = (self.hist.head + crate::app::HN - i) % crate::app::HN;
            let age = now - self.hist.at[k];
            if age > GRAPH_MS {
                break;
            }
            let f = 1000.0 / self.hist.ms[k].max(0.5);
            let col = (((1.0 - age / GRAPH_MS) * (n - 1) as f64) as isize).clamp(0, n as isize - 1)
                as usize;
            // worst frame wins the column, so a stall cannot hide between samples
            if cols[col] < 0.0 {
                filled += 1;
                cols[col] = f;
            } else if f < cols[col] {
                cols[col] = f;
            }
        }

        // Scale to the 90th percentile of the columns, not the highest one: one freak
        // fast frame otherwise pushes the ceiling up a whole band and squashes
        // everything real into the bottom of the graph.
        let mut sorted: Vec<f32> = cols.iter().copied().filter(|v| *v >= 0.0).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let peak = if sorted.is_empty() {
            0.0
        } else {
            sorted[(0.90 * (sorted.len() - 1) as f32) as usize]
        };
        // steps up at once, comes back down only well clear of the boundary
        while self.graph_top < G_TOPS.len() - 1 && peak > G_TOPS[self.graph_top] {
            self.graph_top += 1;
        }
        while self.graph_top > 0 && peak < G_TOPS[self.graph_top - 1] * 0.85 {
            self.graph_top -= 1;
        }
        let top = G_TOPS[self.graph_top];
        let map_y = |v: f32| y1 - ph * v.clamp(0.0, top) / top;

        let font = FontId::monospace(8.5);
        for (i, mark) in [top, top / 2.0, 0.0].iter().enumerate() {
            let y = map_y(*mark).round() + 0.5;
            let c = if i == 2 {
                Color32::from_rgba_premultiplied(198, 211, 220, 56)
            } else {
                Color32::from_rgba_premultiplied(198, 211, 220, 33)
            };
            p.line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], Stroke::new(1.0, c));
            p.text(
                egui::pos2(x0 - 4.0, y),
                Align2::RIGHT_CENTER,
                format!("{}", *mark as i32),
                font.clone(),
                DIM,
            );
        }
        if filled < 2 {
            return;
        }

        // the trace, gaps carried forward so one dropped column is not a hole
        let mut pts: Vec<egui::Pos2> = Vec::with_capacity(n);
        let mut prev = 0.0f32;
        let mut started = false;
        for (j, v) in cols.iter().enumerate() {
            let mut v = *v;
            if v < 0.0 {
                if !started {
                    continue;
                }
                v = prev;
            }
            prev = v;
            started = true;
            pts.push(egui::pos2(x0 + j as f32 * (x1 - x0) / (n - 1) as f32, map_y(v)));
        }
        let fill = Color32::from_rgba_premultiplied(12, 55, 49, 90);
        for w in pts.windows(2) {
            p.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(w[0].x, w[0].y),
                    egui::pos2(w[1].x.max(w[0].x + 1.0), y1),
                ),
                0.0,
                fill,
            );
        }
        p.add(egui::Shape::line(pts, Stroke::new(1.0, FIBER)));

        // the five-second mean, in yellow, with its own reading
        if avg > 0.0 {
            let ay = map_y(avg).round() + 0.5;
            p.line_segment([egui::pos2(x0, ay), egui::pos2(x1, ay)], Stroke::new(1.0, MEAN));
            p.text(
                egui::pos2(x1 - 2.0, if ay < y0 + 9.0 { ay + 9.0 } else { ay - 7.0 }),
                Align2::RIGHT_CENTER,
                fmt_fps(avg),
                font,
                MEAN,
            );
        }
    }

    fn console(&mut self, ctx: &egui::Context) {
        let max_h = ctx.content_rect().height() - 40.0;
        egui::Window::new("console")
            .title_bar(false)
            .resizable(false)
            .anchor(Align2::RIGHT_TOP, [-16.0, 16.0])
            .max_height(max_h)
            .min_width(306.0)
            .max_width(306.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(max_h - 24.0).show(ui, |ui| {
                    self.group_construction(ui);
                    self.group_object(ui);
                    self.group_render(ui);
                    self.group_store(ui);
                    self.footer(ui);
                });
            });
    }

    /// The console's folding header: a click flips the group, and which groups are
    /// open is part of the saved settings.
    fn header(ui: &mut egui::Ui, title: &str, open: &mut bool) -> bool {
        let mark = if *open { "\u{2013}" } else { "+" };
        let r = ui.add_sized(
            [ui.available_width(), 22.0],
            egui::Button::new(
                RichText::new(format!("{mark}  {}", title.to_uppercase())).size(10.0).monospace(),
            )
            .fill(Color32::from_rgba_premultiplied(47, 217, 192, 18)),
        );
        if r.clicked() {
            *open = !*open;
        }
        *open
    }

    fn group_construction(&mut self, ui: &mut egui::Ui) {
        let mut open = self.st.groups[0];
        let show = Self::header(ui, "Construction", &mut open);
        self.st.groups[0] = open;
        if !show {
            return;
        }
        ui.add_space(2.0);
        lbl(ui, "Object");
        let mut scene = self.st.scene;
        let available: Vec<bool> = (0..3)
            .map(|i| self.gfx.as_ref().map(|g| g.scene_available(i)).unwrap_or(true))
            .collect();
        ui.horizontal_wrapped(|ui| {
            for i in 0..3 {
                let r = ui.add_enabled(
                    available[i],
                    egui::Button::selectable(scene == i, RichText::new(OBJ_NAME[i]).size(11.0)),
                );
                if r.clicked() {
                    scene = i;
                }
            }
        });
        if scene != self.st.scene {
            self.st.scene = scene;
        }

        lbl(ui, "Navigation");
        let mut fly = if self.st.fly { 1 } else { 0 };
        if seg(ui, &mut fly, &["Auto cam", "Fly"]) {
            let cam = self.cam;
            let mut held = self.held;
            camera::set_fly(&mut self.st, fly == 1, &cam, &mut held);
            self.held = held;
        }
        if !self.st.fly {
            lbl(ui, "Flight path");
            let mut mode = self.st.mode;
            if seg(ui, &mut mode, &PATH_NAME) {
                self.st.mode = mode;
                self.st.drag_az = 0.0;
                self.st.drag_el = 0.0;
                self.st.vel_az = 0.0;
                self.st.vel_el = 0.0;
            }
        }
        ui.add_space(4.0);
    }

    fn group_object(&mut self, ui: &mut egui::Ui) {
        let mut open = self.st.groups[1];
        let show = Self::header(ui, "Object modifications", &mut open);
        self.st.groups[1] = open;
        if !show {
            return;
        }
        ui.add_space(2.0);
        let scene = self.st.scene;

        if scene == 2 {
            lbl(ui, "Build steps");
            let mut lc = self.st.lc_on;
            toggles(ui, &mut lc, &["1 Slab", "2 Relief", "3 Copy", "4 Intersect", "5 Twirl"]);
            self.st.lc_on = lc;
            slider(ui, "Relief", &mut self.st.lc_relief, 0.0..=0.90, 0.01, |v| format!("{v:.2}"));
            slider(ui, "Terrain scale", &mut self.st.lc_freq, 0.60..=11.0, 0.10, |v| {
                format!("{v:.2}")
            });
            slider(ui, "Slab thickness", &mut self.st.lc_thick, 0.003..=0.120, 0.001, |v| {
                format!("{:.2}", v * 20.0)
            });
            slider(ui, "Twirl", &mut self.st.lc_twirl, 0.0..=3.0, 0.05, |v| format!("{v:.2} rad"));
        }

        let cap = self.march_cap;
        let mut steps = self.st.steps;
        slider(ui, "Ray steps", &mut steps, 48.0..=STEPS_MAX, 8.0, |v| format!("{}", v as i32));
        if steps != self.st.steps {
            self.st.steps = steps;
            self.st.steps_pin = true; // stop the frame-time controller trimming it
        }
        if !self.st.steps_pin {
            ui.label(
                RichText::new(format!("auto {}", self.st.march_steps(cap) as i32))
                    .size(9.0)
                    .color(DIM)
                    .monospace(),
            );
        }
        slider(ui, "Surface precision", &mut self.st.eps, 1.0..=12.0, 0.2, |v| format!("{v:.1}"));
        slider(ui, "Step relaxation", &mut self.st.omega, 1.00..=1.90, 0.02, |v| {
            format!("{v:.2}\u{d7}")
        });

        lbl(ui, "Bounds");
        let mut bound = self.st.bound;
        if seg(ui, &mut bound, &BND_NAME) {
            self.st.bound = bound;
        }
        slider(ui, "Bound padding", &mut self.st.bound_pad, 0.0..=0.60, 0.01, |v| {
            format!("{v:.2}")
        });

        if scene == 0 {
            slider(ui, "Cell density", &mut self.st.density, 9.0..=22.0, 0.5, |v| {
                format!("{} \u{b5}m pitch", (1.45 / v * UM_PER_UNIT).round() as i32)
            });
        }

        lbl(ui, "Domain warp");
        let mut warp = self.st.warp_on;
        toggles(ui, &mut warp, &["Noise", "Twist", "Bend"]);
        self.st.warp_on = warp;
        lbl(ui, "Noise warp");
        let mut wm = self.st.warp_mode;
        if seg(ui, &mut wm, &["Space", "Object"]) {
            self.st.warp_mode = wm;
        }
        slider(ui, "Warp strength", &mut self.st.warp_amt, 0.0..=1.0, 0.01, |v| format!("{v:.2}"));
        slider(ui, "Warp scale", &mut self.st.warp_freq, 0.30..=4.00, 0.05, |v| {
            format!("{v:.2} /unit")
        });

        if scene == 1 {
            slider(ui, "Line width", &mut self.st.thick, 0.006..=0.070, 0.001, |v| {
                format!("{:.1} \u{b5}m", v * 2.0 * UM_PER_UNIT)
            });
        }
        if scene != 2 {
            slider(ui, "Spike rate", &mut self.st.spike, 0.0..=1.0, 0.01, |v| {
                format!("{:.1} Hz", v * 11.0)
            });
        }

        ui.horizontal(|ui| {
            lbl(ui, "Structure");
            ui.color_edit_button_rgb(&mut self.st.colors[scene][0]);
            ui.add_space(8.0);
            lbl(ui, "Accent");
            ui.color_edit_button_rgb(&mut self.st.colors[scene][1]);
        });

        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(self.st.running, RichText::new("Flight").size(11.0)).clicked() {
                self.st.running = !self.st.running;
            }
            if ui.selectable_label(self.st.morph, RichText::new("Morph").size(11.0)).clicked() {
                self.st.morph = !self.st.morph;
            }
            if ui.selectable_label(self.st.spike_on, RichText::new("Impulses").size(11.0)).clicked()
            {
                self.st.spike_on = !self.st.spike_on;
            }
        });
        ui.add_space(4.0);
    }

    fn group_render(&mut self, ui: &mut egui::Ui) {
        let mut open = self.st.groups[2];
        let show = Self::header(ui, "Rendering", &mut open);
        self.st.groups[2] = open;
        if !show {
            return;
        }
        ui.add_space(2.0);
        lbl(ui, "Resolution");
        let mut res = self.st.res_pin;
        if seg(ui, &mut res, &RES_NAME) {
            self.set_res(res);
        }
        lbl(ui, "Antialiasing");
        let mut aa = self.st.aa;
        if seg(ui, &mut aa, &AA_NAME) {
            self.st.aa = aa;
        }

        // ---- native only ----
        lbl(ui, "Graphics API  (ctrl+1/2/3)");
        ui.horizontal_wrapped(|ui| {
            for b in Backend::ALL {
                let r = ui.add_enabled(
                    b.available(),
                    egui::Button::selectable(
                        self.st.backend == b,
                        RichText::new(b.label()).size(11.0),
                    ),
                );
                if r.clicked() && self.st.backend != b {
                    self.want_backend = Some(b);
                }
            }
        });
        lbl(ui, "Present  (V)");
        ui.horizontal_wrapped(|ui| {
            for p in Present::ALL {
                let honoured = self.gfx.as_ref().map(|g| g.present_honoured(p)).unwrap_or(true);
                let r = ui.add_enabled(
                    honoured,
                    egui::Button::selectable(
                        self.st.present == p,
                        RichText::new(p.label()).size(11.0),
                    ),
                );
                if r.clicked() && self.st.present != p {
                    self.apply_present(p);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.st.fullscreen, RichText::new("Full screen  F11").size(11.0))
                .clicked()
            {
                let on = !self.st.fullscreen;
                self.set_fullscreen(on);
            }
            if ui.checkbox(&mut self.st.exclusive, RichText::new("exclusive").size(10.0)).changed()
                && self.st.fullscreen
            {
                self.set_fullscreen(true); // re-enter in the other mode
            }
        });
        if let Some(g) = &self.gfx {
            ui.label(
                RichText::new(format!("{}  \u{b7}  post {}", g.adapter_name, g.post_path))
                    .size(9.0)
                    .color(DIM),
            );
        }
        if !self.gfx_note.is_empty() {
            ui.label(RichText::new(self.gfx_note.clone()).size(9.0).color(MEAN));
        }
        ui.add_space(4.0);
    }

    fn group_store(&mut self, ui: &mut egui::Ui) {
        let mut open = self.st.groups[3];
        let show = Self::header(ui, "Templates", &mut open);
        self.st.groups[3] = open;
        if !show {
            return;
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.tpl_name)
                    .hint_text("name")
                    .desired_width(186.0),
            );
            if ui.button(RichText::new("Save").size(11.0)).clicked() {
                self.save_template();
            }
        });
        if self.templates.is_empty() {
            ui.label(
                RichText::new("A template holds every setting and all six views.")
                    .size(9.0)
                    .color(DIM),
            );
        }
        let mut load: Option<usize> = None;
        let mut drop: Option<usize> = None;
        for (i, t) in self.templates.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.button(RichText::new(t.name.clone()).size(11.0)).clicked() {
                    load = Some(i);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("\u{d7}").on_hover_text("Delete").clicked() {
                        drop = Some(i);
                    }
                });
            });
        }
        if let Some(i) = load {
            self.load_template(i);
        }
        if let Some(i) = drop {
            self.delete_template(i);
        }
        ui.add_space(4.0);
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(RichText::new("Benchmark").size(11.0))
                .on_hover_text("A fixed camera run with the clock stepped by frame, not by time")
                .clicked()
            {
                self.start_bench();
            }
            if ui.button(RichText::new("Save as default").size(11.0)).clicked() {
                self.save_default();
            }
            if ui.button(RichText::new("Reset").size(11.0)).clicked() {
                self.reset_default();
            }
        });
        let hint = if self.st.fly {
            "WASD move \u{b7} Q/E down/up \u{b7} shift sprint \u{b7} right-drag look \u{b7} wheel speed \u{b7} R new view"
        } else {
            "Drag to steer \u{b7} wheel to range in \u{b7} R new view \u{b7} 1-4 resolution \u{b7} 5-0 views, shift to store \u{b7} shift+` fly"
        };
        let toast = self
            .toast
            .as_ref()
            .filter(|(_, at)| at.elapsed().as_secs_f32() < 1.4)
            .map(|(m, _)| m.clone());
        ui.label(RichText::new(toast.unwrap_or_else(|| hint.to_string())).size(9.0).color(DIM));
        ui.label(RichText::new(self.note.clone()).size(9.0).color(DIM));
    }

    fn report_window(&mut self, ctx: &egui::Context) {
        let Some(rep) = &self.report else { return };
        let (title, big, unit, body, text, can_copy) = (
            rep.title.clone(),
            rep.big.clone(),
            rep.unit.clone(),
            rep.body.clone(),
            rep.text.clone(),
            rep.can_copy,
        );
        let mut close = false;
        let mut copy = false;
        let mut save = false;
        egui::Window::new(RichText::new(title).size(11.0).monospace())
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(big).size(30.0).color(FIBER).monospace());
                    ui.label(RichText::new(unit).size(10.0).color(DIM));
                });
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    ui.label(RichText::new(body).size(11.0).monospace().color(CHROME));
                });
                ui.horizontal(|ui| {
                    if can_copy {
                        copy = ui.button(RichText::new("Copy").size(11.0)).clicked();
                        save = ui.button(RichText::new("Save to file").size(11.0)).clicked();
                    }
                    close = ui
                        .button(
                            RichText::new(if self.bench.on { "Stop" } else { "Close" }).size(11.0),
                        )
                        .clicked();
                });
            });
        if copy {
            match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())) {
                Ok(()) => self.toast("Copied"),
                Err(_) => self.toast("Could not reach the clipboard"),
            }
        }
        if save {
            match crate::store::save_report(&text) {
                Ok(p) => self.toast(format!("Saved {}", p.display())),
                Err(e) => self.toast(format!("Could not save: {e}")),
            }
        }
        if close {
            if self.bench.on {
                self.stop_bench();
            } else {
                self.report = None;
            }
        }
    }
}

fn fmt_fps(v: f32) -> String {
    if v < 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.0}")
    }
}
