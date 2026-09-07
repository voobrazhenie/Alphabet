//! Window, input and the frame loop.

use crate::bench::{Bench, RunInfo};
use crate::camera::{self, Cam, Held, Rng};
use crate::gfx::{EguiFrame, Frame, Gfx};
use crate::state::{Backend, Present, State, FHD_H, FHD_W, STEPS_MAX};
use crate::store::{self, Template};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

/// Every frame time of the last five seconds, in a ring so nothing is reallocated
/// or shifted per frame. 1024 slots covers five seconds at any rate a display can
/// actually show.
pub const HN: usize = 1024;

pub struct History {
    pub ms: [f32; HN],
    pub at: [f64; HN],
    pub head: usize,
    pub len: usize,
}

impl Default for History {
    fn default() -> Self {
        History { ms: [0.0; HN], at: [0.0; HN], head: 0, len: 0 }
    }
}

impl History {
    pub fn push(&mut self, now: f64, ms: f32) {
        self.ms[self.head] = ms;
        self.at[self.head] = now;
        self.head = (self.head + 1) % HN;
        if self.len < HN {
            self.len += 1;
        }
    }

    /// frames in the window over the time they took — a true mean rate, not the mean
    /// of per-frame rates, which a single stall would skew
    pub fn mean_fps(&self, now: f64, window_ms: f64) -> f32 {
        let (mut count, mut span) = (0.0f32, 0.0f32);
        for i in 1..=self.len {
            let k = (self.head + HN - i) % HN;
            if now - self.at[k] > window_ms {
                break;
            }
            span += self.ms[k];
            count += 1.0;
        }
        if span > 0.0 {
            count * 1000.0 / span
        } else {
            0.0
        }
    }
}

pub struct ReportView {
    pub title: String,
    pub big: String,
    pub unit: String,
    pub body: String,
    pub text: String,
    pub can_copy: bool,
}

pub struct App {
    pub(crate) window: Option<Arc<Window>>,
    pub(crate) gfx: Option<Gfx>,
    pub(crate) st: State,
    pub(crate) builtin: State,
    pub(crate) cam: Cam,
    pub(crate) held: Held,
    pub(crate) rng: Rng,

    pub(crate) egui_ctx: egui::Context,
    pub(crate) egui_state: Option<egui_winit::State>,

    pub(crate) start: Instant,
    pub(crate) last: Instant,
    pub(crate) ema: f32,
    pub(crate) frames: u32,
    pub(crate) hist: History,
    pub(crate) warm: i32,
    pub(crate) march_cap: f32,

    /// what the scene was marched at last frame
    pub(crate) rw: u32,
    pub(crate) rh: u32,

    pub(crate) hud: bool,
    pub(crate) toast: Option<(String, Instant)>,
    pub(crate) report: Option<ReportView>,
    pub(crate) bench: Bench,
    pub(crate) templates: Vec<Template>,
    pub(crate) tpl_name: String,
    pub(crate) note: String,
    pub(crate) gfx_note: String,

    pub(crate) dragging: bool,
    pub(crate) looking: bool,
    pub(crate) mods: ModifiersState,
    pub(crate) want_backend: Option<Backend>,
    /// the ceiling the fps graph is currently drawn against
    pub(crate) graph_top: usize,
    pub(crate) quit: bool,
}

impl App {
    pub fn new(backend_arg: Option<Backend>) -> App {
        let mut st = State::default();
        let builtin = st.clone();
        let mut note = "Settings are saved on this computer".to_string();
        if let Some(saved) = store::load_settings() {
            st = saved;
            note = format!(
                "Loaded {}",
                store::settings_path().map(|p| p.display().to_string()).unwrap_or_default()
            );
        }
        if let Some(b) = backend_arg {
            st.backend = b;
        }
        if !st.backend.available() {
            st.backend = crate::state::default_backend();
        }
        // the render scale is measured, not saved, so a restored Half or Native has
        // to be put back into it or the first frames come up at the wrong size
        st.scale_q = match st.res_pin {
            2 | 3 => 1.0,
            _ => 0.5,
        };
        let templates = store::load_templates();
        App {
            window: None,
            gfx: None,
            st,
            builtin,
            cam: Cam::default(),
            held: Held::default(),
            rng: Rng::new(0x2FD9C0 ^ store::now_ms()),
            egui_ctx: egui::Context::default(),
            egui_state: None,
            start: Instant::now(),
            last: Instant::now(),
            ema: 16.7,
            frames: 0,
            hist: History::default(),
            // The opening frames are capped hard. Nothing has been measured yet, and
            // the first frame also carries the shader build.
            warm: 5,
            march_cap: STEPS_MAX,
            rw: 0,
            rh: 0,
            hud: true,
            toast: None,
            report: None,
            bench: Bench::default(),
            templates,
            tpl_name: String::new(),
            note,
            gfx_note: String::new(),
            dragging: false,
            looking: false,
            mods: ModifiersState::empty(),
            want_backend: None,
            graph_top: 0,
            quit: false,
        }
    }

    pub(crate) fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn make_window(
        &self,
        el: &ActiveEventLoop,
        size: Option<winit::dpi::PhysicalSize<u32>>,
    ) -> Option<Arc<Window>> {
        let mut attrs = Window::default_attributes().with_title("Cortical Flythrough");
        attrs = match size {
            Some(s) if s.width > 0 && s.height > 0 => attrs.with_inner_size(s),
            _ => attrs.with_inner_size(winit::dpi::LogicalSize::new(1440.0, 900.0)),
        };
        match el.create_window(attrs) {
            Ok(w) => Some(Arc::new(w)),
            Err(e) => {
                log::error!("no window: {e}");
                None
            }
        }
    }

    /// Bring up a device on `backend`, and the console with it.
    ///
    /// Windows lets a window's pixel format be set only once, so a window that has
    /// already carried an OpenGL context may refuse the next one. A failed switch
    /// therefore gets one attempt on a fresh window before falling back to another
    /// API — otherwise "try OpenGL" would be a one-way door.
    fn build_gfx(&mut self, el: &ActiveEventLoop, backend: Backend) {
        let Some(window) = self.window.clone() else { return };
        let size = window.inner_size();
        let was_fullscreen = self.st.fullscreen;
        self.gfx = None; // the old device must let go of the window first

        let mut window = window;
        let mut err = match Gfx::new(window.clone(), backend, self.st.present) {
            Ok(g) => {
                self.adopt(g, backend, window);
                return;
            }
            Err(e) => e,
        };

        if let Some(fresh) = self.make_window(el, Some(size)) {
            self.window = Some(fresh.clone());
            window = fresh;
            match Gfx::new(window.clone(), backend, self.st.present) {
                Ok(g) => {
                    self.adopt(g, backend, window);
                    if was_fullscreen {
                        self.set_fullscreen(true);
                    }
                    return;
                }
                Err(e) => err = e,
            }
        }

        // whatever is left that this machine will actually run
        for b in Backend::ALL {
            if b == backend || !b.available() {
                continue;
            }
            if let Ok(g) = Gfx::new(window.clone(), b, self.st.present) {
                self.adopt(g, b, window.clone());
                self.gfx_note = format!("{} \u{2014} using {} instead", err, b.label());
                self.toast(format!("{} is not available here", backend.label()));
                if was_fullscreen {
                    self.set_fullscreen(true);
                }
                return;
            }
        }
        self.gfx_note = err.clone();
        self.toast(err);
    }

    /// Take on a freshly built device: rebuild the console against it, and let a
    /// software adapter bring the march budget down before the first frame.
    fn adopt(&mut self, mut g: Gfx, backend: Backend, window: Arc<Window>) {
        // The console's renderer lives inside Gfx, and egui hands over its font
        // atlas exactly once — so a new device needs a new context to be given one.
        self.egui_ctx = egui::Context::default();
        crate::ui::style(&self.egui_ctx);
        self.egui_state = Some(egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        ));
        self.gfx_note.clear();
        if !g.scene_pipe(self.st.scene) {
            self.gfx_note = format!("{} would not build this object", backend.label());
        }
        if g.is_software {
            // no GPU behind this adapter: the march has to come down or the first
            // frame takes minutes
            self.march_cap = 96.0;
            self.st.steps = self.st.steps.min(56.0);
            self.st.steps_pin = false;
            self.gfx_note =
                format!("Software renderer ({}) \u{2014} this will be slow", g.adapter_name);
        } else {
            self.march_cap = STEPS_MAX;
        }
        self.st.backend = backend;
        // the title says which API is driving, so a screenshot of a run carries it
        window.set_title(&format!("Cortical Flythrough \u{2014} {}", backend.label()));
        self.warm = 5;
        self.rw = 0;
        self.rh = 0;
        self.gfx = Some(g);
    }

    /// The size the scene is marched at. The swapchain is always the window; the
    /// post pass fits one to the other, which is what the browser's canvas scaling
    /// did on the page.
    fn render_size(&mut self, win_w: u32, win_h: u32) -> (u32, u32) {
        // SSAA renders the target at 2x and lets the post pass filter it down, so
        // the budget applies to that, not to the window
        let ss = if self.st.aa == 2 { 2 } else { 1 };
        let (mut w, mut h);
        if self.st.res_pin == 3 && self.warm <= 0 {
            w = FHD_W;
            h = FHD_H;
        } else {
            // Half and Native pin the scale; Auto hands it to the frame-time
            // controller. Either way it is the same field, so a pinned resolution
            // can still be given up when the frame time becomes untenable.
            let q = self.st.scale_q;
            w = ((win_w as f32 * q).round() as u32).max(2);
            h = ((win_h as f32 * q).round() as u32).max(2);
            let max_px: f32 = if self.warm > 0 {
                1.1e5
            } else if self.march_cap <= 96.0 {
                4.0e5 // software
            } else if self.st.res_pin != 0 {
                3.4e7 // pinned by hand: let the GPU stretch
            } else {
                3.0e6 // auto stays inside what a per-pixel marcher can sustain
            };
            let px = (w * h * ss * ss) as f32;
            if px > max_px {
                let k = (max_px / px).sqrt();
                w = (((w as f32) * k).round() as u32).max(2);
                h = (((h as f32) * k).round() as u32).max(2);
            }
        }
        self.st.fit = if win_h as f32 > win_w as f32 * 1.25 { 0.92 } else { 1.0 };
        (w * ss, h * ss)
    }

    pub(crate) fn set_fullscreen(&mut self, on: bool) {
        let Some(win) = &self.window else { return };
        self.st.fullscreen = on;
        if !on {
            win.set_fullscreen(None);
            return;
        }
        let mode = if self.st.exclusive {
            win.current_monitor().and_then(|m| {
                m.video_modes()
                    .max_by_key(|v| (v.size().width * v.size().height, v.refresh_rate_millihertz()))
            })
        } else {
            None
        };
        match mode {
            Some(m) => win.set_fullscreen(Some(Fullscreen::Exclusive(m))),
            None => win.set_fullscreen(Some(Fullscreen::Borderless(None))),
        }
    }

    pub(crate) fn apply_present(&mut self, p: Present) {
        self.st.present = p;
        if let Some(g) = &mut self.gfx {
            g.set_present(p);
        }
    }

    pub(crate) fn start_bench(&mut self) {
        let steps = self.st.march_steps(self.march_cap);
        // uncapped for the run where the driver has it: a frame time sitting on the
        // refresh rate measures the display, not the render
        if self.st.present != Present::Immediate {
            if let Some(g) = &self.gfx {
                if g.present_honoured(Present::Immediate) {
                    self.bench.present_was = Some(self.st.present);
                    let p = Present::Immediate;
                    self.apply_present(p);
                }
            }
        }
        self.bench.start(&self.st, steps);
        self.report = Some(ReportView {
            title: "Benchmark \u{b7} running".into(),
            big: "0".into(),
            unit: "% done".into(),
            body: self.bench.progress_text(),
            text: String::new(),
            can_copy: false,
        });
    }

    pub(crate) fn stop_bench(&mut self) {
        if !self.bench.on {
            return;
        }
        self.bench.stop(&mut self.st);
        self.finish_bench();
    }

    fn finish_bench(&mut self) {
        // the run may have switched to uncapped presentation; the report has to name
        // what it ran with, not what it puts back
        let ran_with = self.st.present;
        if let Some(p) = self.bench.present_was.take() {
            self.apply_present(p);
        }
        let (adapter, driver, post) = match &self.gfx {
            Some(g) => (g.adapter_name.clone(), g.driver.clone(), g.post_path),
            None => (String::new(), String::new(), ""),
        };
        let (win_w, win_h, scale) = match &self.window {
            Some(w) => {
                let s = w.inner_size();
                (s.width, s.height, w.scale_factor())
            }
            None => (0, 0, 1.0),
        };
        let info = RunInfo {
            rw: self.rw,
            rh: self.rh,
            win_w,
            win_h,
            scale_factor: scale,
            adapter: &adapter,
            driver: &driver,
            backend: self.st.backend,
            present: ran_with,
            post,
        };
        let rep = self.bench.report(&self.st, &info);
        self.report = Some(if rep.enough {
            ReportView {
                title: format!(
                    "Benchmark \u{b7} {}",
                    if rep.complete { "done" } else { "partial" }
                ),
                big: format!("{:.1}", rep.avg),
                unit: "avg fps".into(),
                body: rep.text.clone(),
                text: rep.text,
                can_copy: true,
            }
        } else {
            ReportView {
                title: "Benchmark \u{b7} stopped".into(),
                big: "\u{2014}".into(),
                unit: String::new(),
                body: rep.text,
                text: String::new(),
                can_copy: false,
            }
        });
    }

    fn draw(&mut self) {
        let Some(window) = self.window.clone() else { return };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let now = Instant::now();
        let raw = (now - self.last).as_secs_f32() * 1000.0; // true cost of the last frame
        let dt = (raw / 1000.0).min(0.05); // clamped, only for the clocks
        self.last = now;
        self.hist.push((now - self.start).as_secs_f64() * 1000.0, raw);

        // a run drives the camera and the clocks itself, and may hand control back
        // mid-frame when it finishes
        if self.bench.on {
            let done = self.bench.step(now, raw, &mut self.st, &mut self.cam);
            if done {
                self.finish_bench();
            } else if let Some(r) = &mut self.report {
                let pc = self.bench.percent().to_string();
                if r.big != pc {
                    r.big = pc;
                    r.body = self.bench.progress_text();
                }
            }
        }
        if !self.bench.on {
            if self.st.running {
                self.st.clock += dt;
            }
            if self.st.morph {
                self.st.m_clock += dt;
            }
            if self.st.running || self.st.morph {
                self.st.p_clock += dt;
            }
            self.st.drag_az += self.st.vel_az;
            self.st.drag_el += self.st.vel_el;
            self.st.vel_az *= 0.90;
            self.st.vel_el *= 0.90;
            self.st.drag_el = self.st.drag_el.clamp(-1.0, 1.0);
            // the clocks clamp dt hard to stay stable; flight tolerates a longer
            // frame than that before it starts under-travelling
            if self.st.fly {
                camera::fly_step(&mut self.st, (raw / 1000.0).min(0.10), &self.held);
            }
            self.cam = camera::camera(&self.st, self.st.clock);
        }

        let (rw, rh) = self.render_size(size.width, size.height);
        self.rw = rw;
        self.rh = rh;

        // ---- the console ----
        let egui_ctx = self.egui_ctx.clone();
        let raw_input = match &mut self.egui_state {
            Some(s) => s.take_egui_input(&window),
            None => return,
        };
        let full = egui_ctx.run_ui(raw_input, |ui| {
            let ctx = ui.ctx().clone();
            self.ui(&ctx);
        });
        if let Some(s) = &mut self.egui_state {
            s.handle_platform_output(&window, full.platform_output);
        }
        let ppp = egui_ctx.pixels_per_point();
        let jobs = egui_ctx.tessellate(full.shapes, ppp);

        let steps =
            if self.bench.on { self.bench.steps } else { self.st.march_steps(self.march_cap) };
        let uniforms = crate::gfx::uniforms(&self.st, &self.cam, self.rw, self.rh, steps);

        if let Some(g) = &mut self.gfx {
            g.resize_surface(size.width, size.height);
            let scene = self.st.scene;
            g.scene_pipe(scene);
            let outcome = g.render(
                scene,
                &uniforms,
                rw,
                rh,
                self.st.aa == 1,
                EguiFrame { jobs, delta: full.textures_delta, pixels_per_point: ppp },
            );
            if outcome == Frame::Skipped {
                return;
            }
        }

        // snap up on a spike so one bad frame is enough to trigger a correction
        self.ema = if raw > self.ema * 2.0 { raw } else { self.ema * 0.90 + raw * 0.10 };
        self.frames += 1;
        // correct within a few frames when the frame time is over budget. Never
        // during a run: a controller that changes the resolution or the march budget
        // half way through would be measuring two different things.
        let trigger = if self.ema > 34.0 {
            3
        } else if self.ema < 13.0 {
            25
        } else {
            40
        };
        if !self.bench.on && self.frames >= trigger {
            if self.st.res_pin != 0 && self.ema < 250.0 {
                // a pinned resolution only gives way when the frame time gets untenable
            } else if self.ema > 22.0 && self.st.scale_q > 0.30 {
                // cost is quadratic in the scale, so aim straight at the budget
                let k = (17.0 / self.ema).sqrt();
                self.st.scale_q = (self.st.scale_q * k).min(self.st.scale_q - 0.04).max(0.30);
            } else if self.ema < 13.0 && self.st.scale_q < 1.0 {
                self.st.scale_q =
                    (self.st.scale_q * (15.0 / self.ema.max(1.0)).sqrt().min(1.25)).min(1.0);
            }
            if !self.st.steps_pin {
                if self.ema > 40.0 && self.st.steps > 44.0 {
                    self.st.steps -= 8.0;
                } else if self.ema < 12.0 && self.st.steps < 192.0 {
                    self.st.steps += 8.0;
                }
            }
            self.frames = 0;
        }
        if self.warm > 0 {
            self.warm -= 1;
        }
    }

    // ---- input ----------------------------------------------------------

    fn key(&mut self, code: KeyCode, pressed: bool, el: &ActiveEventLoop) {
        let shift = self.mods.shift_key();
        let ctrl = self.mods.control_key() || self.mods.super_key();

        // WASDQE belong to fly mode while it is on; A would otherwise cycle the AA
        if self.st.fly && !ctrl {
            let held = &mut self.held;
            let mut hit = true;
            match code {
                KeyCode::KeyW => held.w = pressed,
                KeyCode::KeyA => held.a = pressed,
                KeyCode::KeyS => held.s = pressed,
                KeyCode::KeyD => held.d = pressed,
                KeyCode::KeyQ => held.q = pressed,
                KeyCode::KeyE => held.e = pressed,
                _ => hit = false,
            }
            held.shift = shift;
            if hit {
                return;
            }
        }
        if !pressed {
            return;
        }

        // NATIVE: which graphics API is driving the frame
        if ctrl {
            match code {
                KeyCode::Digit1 | KeyCode::Numpad1 => self.want_backend = Some(Backend::Vulkan),
                KeyCode::Digit2 | KeyCode::Numpad2 => self.want_backend = Some(Backend::Dx12),
                KeyCode::Digit3 | KeyCode::Numpad3 => self.want_backend = Some(Backend::Gl),
                KeyCode::KeyQ => el.exit(),
                KeyCode::KeyS => self.save_default(),
                _ => {}
            }
            return;
        }

        match code {
            // space is the one "freeze everything" key: camera and field together
            KeyCode::Space => {
                let go = !self.st.running;
                self.st.running = go;
                self.st.morph = go;
            }
            KeyCode::KeyM => self.st.morph = !self.st.morph,
            KeyCode::KeyI => self.st.spike_on = !self.st.spike_on,
            KeyCode::KeyR => {
                let cam = self.cam;
                camera::random_view(&mut self.st, &cam, &mut self.rng);
            }
            KeyCode::Digit1 | KeyCode::Numpad1 => self.set_res(0),
            KeyCode::Digit2 | KeyCode::Numpad2 => self.set_res(1),
            KeyCode::Digit3 | KeyCode::Numpad3 => self.set_res(2),
            KeyCode::Digit4 | KeyCode::Numpad4 => self.set_res(3),
            KeyCode::KeyA => self.st.aa = (self.st.aa + 1) % 3,
            KeyCode::KeyF | KeyCode::F11 => {
                let on = !self.st.fullscreen;
                self.set_fullscreen(on);
            }
            KeyCode::KeyH | KeyCode::KeyU => self.hud = !self.hud,
            KeyCode::KeyV => {
                // NATIVE: vsync, mailbox, uncapped
                let i = Present::ALL.iter().position(|p| *p == self.st.present).unwrap_or(0);
                let p = Present::ALL[(i + 1) % Present::ALL.len()];
                self.apply_present(p);
                self.toast(format!("Present: {}", p.label()));
            }
            KeyCode::Escape => {
                if self.bench.on {
                    self.stop_bench();
                } else if self.report.is_some() {
                    self.report = None;
                } else if self.st.fullscreen {
                    self.set_fullscreen(false);
                }
            }
            // shift+` hands the camera over without reaching for the console
            KeyCode::Backquote if shift => {
                let (cam, on) = (self.cam, !self.st.fly);
                camera::set_fly(&mut self.st, on, &cam, &mut self.held);
            }
            KeyCode::Digit5 | KeyCode::Numpad5 => self.fav(0, shift),
            KeyCode::Digit6 | KeyCode::Numpad6 => self.fav(1, shift),
            KeyCode::Digit7 | KeyCode::Numpad7 => self.fav(2, shift),
            KeyCode::Digit8 | KeyCode::Numpad8 => self.fav(3, shift),
            KeyCode::Digit9 | KeyCode::Numpad9 => self.fav(4, shift),
            KeyCode::Digit0 | KeyCode::Numpad0 => self.fav(5, shift),
            _ => {}
        }
    }

    pub(crate) fn fav(&mut self, i: usize, store_it: bool) {
        const LBL: [&str; 6] = ["5", "6", "7", "8", "9", "0"];
        if store_it {
            camera::fav_store(&mut self.st, i);
            self.toast(format!("View {} stored", LBL[i]));
        } else {
            let cam = self.cam;
            let mut held = self.held;
            let ok = camera::fav_recall(&mut self.st, i, &cam, &mut held);
            self.held = held;
            if ok {
                self.cam = camera::camera(&self.st, self.st.clock);
                self.toast(format!("View {}", LBL[i]));
            } else {
                self.toast(format!("View {} is empty", LBL[i]));
            }
        }
    }

    pub(crate) fn set_res(&mut self, v: usize) {
        self.st.res_pin = v;
        if v == 1 {
            self.st.scale_q = 0.5;
        } else if v == 2 {
            self.st.scale_q = 1.0;
        }
        // NATIVE: a window can be resized to the render, which a browser tab cannot.
        // One render pixel per screen pixel, and the post pass has nothing to fit.
        if v == 3 {
            if let Some(w) = &self.window {
                if !self.st.fullscreen {
                    let _ = w.request_inner_size(winit::dpi::PhysicalSize::new(FHD_W, FHD_H));
                }
            }
            self.toast("Rendering 1920 \u{d7} 1080");
        }
    }

    pub(crate) fn save_default(&mut self) {
        match store::save_settings(&self.st) {
            Ok(p) => {
                self.note = format!("Saved {}", p.display());
                self.toast("Saved as default");
            }
            Err(e) => {
                self.note = format!("Could not save: {e}");
                self.toast("Could not save");
            }
        }
    }

    pub(crate) fn reset_default(&mut self) {
        let keep = (self.st.backend, self.st.present, self.st.fullscreen, self.st.exclusive);
        self.st = self.builtin.clone();
        self.st.backend = keep.0;
        self.st.present = keep.1;
        self.st.fullscreen = keep.2;
        self.st.exclusive = keep.3;
        match store::clear_settings() {
            Ok(()) => self.note = "Settings reset".into(),
            Err(e) => self.note = format!("Could not clear the file: {e}"),
        }
        self.toast("Reset");
    }

    pub(crate) fn save_template(&mut self) {
        let name = self.tpl_name.trim().to_string();
        if name.is_empty() {
            self.toast("Give the template a name first");
            return;
        }
        let t = Template { name: name.clone(), saved_at: store::now_ms(), data: self.st.clone() };
        if let Some(slot) = self.templates.iter_mut().find(|x| x.name == name) {
            *slot = t;
        } else {
            self.templates.push(t);
        }
        match store::save_templates(&self.templates) {
            Ok(()) => {
                self.note = format!("Template \u{201c}{name}\u{201d} saved");
                self.tpl_name.clear();
            }
            Err(e) => self.note = format!("Could not save the template: {e}"),
        }
    }

    pub(crate) fn load_template(&mut self, i: usize) {
        let Some(t) = self.templates.get(i) else { return };
        let name = t.name.clone();
        let keep = (self.st.backend, self.st.present, self.st.fullscreen, self.st.exclusive);
        self.st = t.data.clone();
        self.st.backend = keep.0;
        self.st.present = keep.1;
        self.st.fullscreen = keep.2;
        self.st.exclusive = keep.3;
        self.toast(format!("Loaded \u{201c}{name}\u{201d}"));
    }

    pub(crate) fn delete_template(&mut self, i: usize) {
        if i >= self.templates.len() {
            return;
        }
        let name = self.templates.remove(i).name;
        match store::save_templates(&self.templates) {
            Ok(()) => self.note = format!("Deleted \u{201c}{name}\u{201d}"),
            Err(e) => self.note = format!("Could not save the template list: {e}"),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let Some(window) = self.make_window(el, None) else {
            el.exit();
            return;
        };
        self.window = Some(window);
        let backend = self.st.backend;
        self.build_gfx(el, backend); // brings up the device and the console with it
        if self.st.fullscreen {
            self.set_fullscreen(true);
        }
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else { return };
        let consumed = match &mut self.egui_state {
            Some(s) => s.on_window_event(&window, &event).consumed,
            None => false,
        };

        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.gfx {
                    g.resize_surface(size.width, size.height);
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(g) = &mut self.gfx {
                    g.reconfigure();
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                self.held.shift = self.mods.shift_key();
            }
            WindowEvent::Focused(false) => {
                // a key held while the window loses focus never sends its release
                self.held.clear();
                self.dragging = false;
                self.looking = false;
            }
            WindowEvent::KeyboardInput { event, is_synthetic: false, .. } => {
                // keys are ignored while a text field has focus
                if self.egui_ctx.egui_wants_keyboard_input() && event.state == ElementState::Pressed
                {
                    return;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.key(code, event.state == ElementState::Pressed, el);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let down = state == ElementState::Pressed;
                if down && consumed {
                    return;
                }
                match button {
                    MouseButton::Left => self.dragging = down && !self.st.fly,
                    MouseButton::Right => self.looking = down && self.st.fly,
                    _ => {}
                }
                if !down {
                    self.dragging = false;
                    self.looking = false;
                }
                let grab = self.looking;
                window.set_cursor_visible(!grab);
                let _ = window.set_cursor_grab(if grab {
                    winit::window::CursorGrabMode::Confined
                } else {
                    winit::window::CursorGrabMode::None
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if consumed {
                    return;
                }
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if dy != 0.0 {
                    // spreading two fingers means "closer" when orbiting, "faster" when flying
                    if self.st.fly {
                        let v = self.st.fly_speed * (dy.signum() * 0.14).exp();
                        camera::set_fly_speed(&mut self.st, v);
                        self.toast(format!("{:.2} units/s", self.st.fly_speed));
                    } else {
                        let z = self.st.zoom * (1.0 - dy.signum() * 0.07);
                        camera::set_zoom(&mut self.st, z);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(b) = self.want_backend.take() {
                    if self.bench.on {
                        // swapping the device mid-run would measure two machines
                        self.toast("Not while a benchmark is running");
                    } else if b != self.st.backend && b.available() {
                        self.build_gfx(el, b);
                        self.toast(format!("{}", self.st.backend.label()));
                    } else if !b.available() {
                        self.toast(format!("{} is not available on this system", b.label()));
                    }
                }
                self.draw();
                if self.quit {
                    el.exit();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            let (dx, dy) = (delta.0 as f32, delta.1 as f32);
            if self.looking && self.st.fly {
                camera::fly_look(&mut self.st, dx, dy);
            } else if self.dragging && !self.st.fly {
                self.st.vel_az = -dx * 0.0032;
                self.st.vel_el = dy * 0.0024;
                self.st.drag_az += self.st.vel_az * 3.0;
                self.st.drag_el = (self.st.drag_el + self.st.vel_el * 3.0).clamp(-1.0, 1.0);
            }
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}
