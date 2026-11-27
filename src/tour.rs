//! The first run: on macOS a Full Disk
//! Access guide that notices by itself when access is given, then the Overview walks through
//! Clean clutter, Clean browser and the deeper finders before showing the normal dashboard.
//! Each step moves on when its cleaning finishes, or with Skip.

use crate::kit::{self, b, dim, ink, muted, primary, Icon};
use crate::{clean, Confirm, Page, Sweep};
use crate::illus::{self, Scene};
use eframe::egui::{self, pos2, text::LayoutJob, vec2, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, TextFormat, Vec2};
use std::f32::consts::TAU;

/// Slow at the end, like something settling into place.
fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

/// 0 → 1 over `secs`, starting `delay` seconds after `since`.
fn appear(now: f64, since: f64, delay: f32, secs: f32) -> f32 {
    ease_out(((now - since) as f32 - delay) / secs)
}

/// A cheap repeatable "random" number in 0..1 for confetti.
fn rnd(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(747_796_405).wrapping_add(salt.wrapping_mul(2_891_336_453));
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    (x % 10_000) as f32 / 10_000.0
}

/// Confetti drifting down over `r`, behind the content.
fn confetti(p: &egui::Painter, r: Rect, t: f32, alpha: f32) {
    let cols = [kit::accent(), Color32::from_rgb(0xe3, 0xb2, 0x5b), Color32::from_rgb(0x8a, 0x9a, 0x5b), Color32::from_rgb(0xe8, 0xa3, 0x87)];
    for i in 0..46u32 {
        let x = r.left() + rnd(i, 1) * r.width();
        let speed = 0.08 + 0.1 * rnd(i, 2);
        let y = r.top() + ((t * speed + rnd(i, 3)).fract()) * r.height();
        let sway = 10.0 * (t * (0.6 + rnd(i, 4)) + rnd(i, 5) * TAU).sin();
        let a = t * (1.0 + 2.0 * rnd(i, 6)) + rnd(i, 7) * TAU;
        let (w, h) = (6.0, 3.0 + 3.0 * a.cos().abs());
        let c = pos2(x + sway, y);
        let d = Vec2::angled(a);
        let n = vec2(-d.y, d.x);
        let pts = vec![c - d * w / 2.0 - n * h / 2.0, c + d * w / 2.0 - n * h / 2.0, c + d * w / 2.0 + n * h / 2.0, c - d * w / 2.0 + n * h / 2.0];
        // Fade near the bottom so pieces don't pile onto the button.
        let fade = (1.0 - (y - r.top()) / r.height()).clamp(0.0, 1.0).sqrt();
        p.add(Shape::convex_polygon(pts, cols[i as usize % 4].gamma_multiply(alpha * fade), Stroke::NONE));
    }
}

/// The check that draws itself: the ring sweeps round, then the tick is drawn, then it pulses.
fn drawn_check(ui: &mut egui::Ui, progress: f32, t: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(130.0), Sense::hover());
    let p = ui.painter();
    let c = r.center();
    let ring = (progress / 0.6).min(1.0);
    let pulse = if progress >= 1.0 { 1.0 + 0.03 * (t * 2.0).sin() } else { 1.0 };
    p.circle_filled(c, 58.0 * pulse * ring, kit::accent().gamma_multiply(0.10));
    let arc: Vec<Pos2> = (0..=48).filter(|i| (*i as f32) / 48.0 <= ring).map(|i| c + Vec2::angled(-TAU / 4.0 + TAU * i as f32 / 48.0) * 58.0 * pulse).collect();
    if arc.len() > 1 {
        p.add(Shape::line(arc, Stroke::new(3.0, kit::accent())));
    }
    let tick = ((progress - 0.55) / 0.45).clamp(0.0, 1.0);
    if tick > 0.0 {
        let (a, m, z) = (c + vec2(-24.0, 2.0), c + vec2(-7.0, 19.0), c + vec2(26.0, -18.0));
        let first = (tick / 0.4).min(1.0);
        p.line_segment([a, a + (m - a) * first], Stroke::new(6.0, kit::accent()));
        if tick > 0.4 {
            let second = (tick - 0.4) / 0.6;
            p.line_segment([m, m + (z - m) * second], Stroke::new(6.0, kit::accent()));
        }
    }
}

/// Tour stages, saved: Full Disk Access guide, clutter, browsers, deeper, done.
pub const FDA: u8 = 0;
pub const CLUTTER: u8 = 1;
pub const BROWSERS: u8 = 2;
pub const DEEPER: u8 = 3;
pub const DONE: u8 = 4;

/// The guide's steps; [b]…[/b] is bold.
const STEPS: [&str; 5] = [
    "Open the [b]Privacy & Security[/b] section in [b]System Settings[/b] (the button below does it)",
    "Click [b]Full Disk Access[/b]",
    "Turn on the switch for [b]Claybroom[/b] (don't see it? Click [b]+[/b] and pick it from Applications)",
    "Enter your password if asked",
    "If prompted, click [b]Quit & Reopen[/b]",
];

/// Text with [b]bold[/b] parts.
fn rich(text: &str, size: f32, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (i, part) in text.split("[b]").enumerate() {
        let (bold, rest) = if i == 0 { ("", part) } else { part.split_once("[/b]").unwrap_or((part, "")) };
        let fmt = |family: egui::FontFamily| TextFormat { font_id: egui::FontId::new(size, family), color: ink(), ..Default::default() };
        if !bold.is_empty() {
            job.append(bold, 0.0, fmt(kit::bold_family()));
        }
        job.append(rest, 0.0, fmt(egui::FontFamily::Proportional));
    }
    job.wrap.max_width = width;
    job
}

impl Sweep {
    /// When an entrance starts: now, or long ago for a picture (its frames come faster than real
    /// time, so the picture shows the settled screen).
    fn anim_start(&self, now: f64) -> f64 {
        if self.shot.is_some() { now - 100.0 } else { now }
    }

    /// Whether the Full Disk Access guide takes the Overview (first run on macOS, or SEE HOW).
    pub fn fda_guide_showing(&self) -> bool {
        cfg!(target_os = "macos") && (self.fda_guide || self.saved.tour == FDA)
    }

    /// The Full Disk Access guide, then "All set!" once access is given.
    pub fn fda_guide_ui(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        // Watch for the switch being turned on (it often needs a restart: step 5).
        if self.needs_fda && now - self.fda_checked > 1.5 {
            self.fda_checked = now;
            self.needs_fda = clean::needs_full_disk_access();
        }
        // Motion: each view plays its entrance when it first shows.
        let view = if self.needs_fda { 10 } else { 11 };
        if self.tour_anim.0 != view {
            self.tour_anim = (view, self.anim_start(now));
        }
        let since = self.tour_anim.1;
        let t = now as f32;
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            if !self.needs_fda {
                confetti(ui.painter(), ui.max_rect().shrink2(vec2(20.0, 0.0)), t, appear(now, since, 0.3, 0.8));
                ui.scope(|ui| {
                    ui.multiply_opacity(appear(now, since, 0.0, 0.4));
                    ui.label(RichText::new("All set! Enjoy cleaning with Claybroom").family(kit::head_family()).size(34.0).color(ink()));
                });
                ui.add_space(14.0);
                drawn_check(ui, ((now - since) as f32 / 1.2).min(1.0), t);
                ui.add_space(18.0);
                for (i, (ic, text)) in [
                    (Icon::Broom, "Clear out caches, logs and leftovers"),
                    (Icon::Picture, "Spot duplicate files and weak photos"),
                    (Icon::Trash, "Remove apps you've stopped using"),
                ]
                .into_iter()
                .enumerate()
                {
                    let e = appear(now, since, 1.0 + i as f32 * 0.18, 0.45);
                    ui.horizontal(|ui| {
                        ui.multiply_opacity(e);
                        ui.add_space((ui.available_width() - 420.0).max(0.0) / 2.0 + 30.0 * (1.0 - e));
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                        kit::icon(ui.painter(), r, ic, kit::accent());
                        ui.label(RichText::new(text).size(17.0));
                    });
                    ui.add_space(4.0);
                }
                ui.add_space(20.0);
                let go = ui
                    .scope(|ui| {
                        ui.multiply_opacity(appear(now, since, 1.6, 0.4).max(0.05));
                        primary(ui, "CONTINUE", true)
                    })
                    .inner;
                if go {
                    self.fda_guide = false;
                    self.saved.tour = self.saved.tour.max(CLUTTER);
                }
                return;
            }
            ui.label(RichText::new("Let Claybroom see your disk").family(kit::head_family()).size(34.0).color(ink()));
            ui.label(RichText::new("Claybroom needs to see your whole disk to find the Trash, Mail downloads and every browser's data.").size(16.0).color(dim()));
            ui.add_space(18.0);
            kit::card(ui, |ui| {
                let art_w = 240.0;
                let steps_w = (ui.available_width() - art_w - 40.0).max(320.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(steps_w);
                        for (n, step) in STEPS.iter().enumerate() {
                            // Steps slide in one after another.
                            let e = appear(now, since, 0.15 + n as f32 * 0.12, 0.45);
                            ui.horizontal(|ui| {
                                ui.multiply_opacity(e);
                                ui.add_space(24.0 * (1.0 - e));
                                let (r, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                                ui.painter().circle_filled(r.center(), 13.0 * (0.6 + 0.4 * e), kit::accent());
                                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, (n + 1).to_string(), egui::FontId::new(14.0, kit::bold_family()), Color32::WHITE);
                                ui.add_space(6.0);
                                let w = ui.available_width();
                                ui.add(egui::Label::new(rich(step, 15.5, w)).wrap());
                            });
                            ui.add_space(6.0);
                        }
                    });
                    let (art, _) = ui.allocate_exact_size(vec2(art_w, art_w * 0.75), Sense::hover());
                    illus::draw(ui.painter(), art, Scene::Access, t);
                });
            });
            ui.add_space(16.0);
            if primary(ui, "OPEN PRIVACY SETTINGS", true) {
                clean::open_full_disk_access_settings();
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 260.0).max(0.0) / 2.0);
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(muted("Waiting for you to allow us…"));
            });
            ui.add_space(8.0);
            if ui.link(RichText::new("Skip for now").color(dim())).clicked() {
                self.confirm = Some(Confirm::SkipFda);
            }
        });
    }

    /// Skipping the guide (after "Are you sure?").
    pub fn skip_fda_guide(&mut self) {
        self.fda_guide = false;
        self.saved.tour = self.saved.tour.max(CLUTTER);
    }

    /// The Overview's tour card in place of the top row, while the tour runs. Returns the page to
    /// start (its scan), if a button was pressed.
    pub fn tour_card(&mut self, ui: &mut egui::Ui, busy: bool) -> Option<Page> {
        let mut go = None;
        let stage = self.saved.tour;
        let (scene, title, sub) = match stage {
            CLUTTER => (Scene::Clutter, "First: a clutter scan", "Caches, logs, the Trash and old downloads, in one list."),
            BROWSERS => (Scene::Browser, "Next: your browsers", "Caches, history and cookies of every browser you use."),
            _ => (Scene::Duplicates, "Now let's go deeper", "Copies of the same files, and blurry or look-alike photos."),
        };
        // Each new step slides up into place; the pictures keep moving.
        let now = ui.input(|i| i.time);
        if self.tour_anim.0 != stage {
            self.tour_anim = (stage, self.anim_start(now));
        }
        let e = appear(now, self.tour_anim.1, 0.0, 0.55);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        ui.scope(|ui| {
        ui.add_space(18.0 * (1.0 - e));
        ui.multiply_opacity(e);
        egui::Frame::new()
            .fill(kit::tile_fill())
            .stroke(egui::Stroke::new(1.5, kit::accent().gamma_multiply(0.7)))
            .corner_radius(egui::CornerRadius::same(26))
            .inner_margin(egui::Margin { left: 10, right: 26, top: 8, bottom: 8 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (art, _) = ui.allocate_exact_size(vec2(124.0, 93.0), Sense::hover());
                    illus::draw(ui.painter(), art, scene, now as f32);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            // Progress: done steps filled, the current one a stretched pill.
                            for k in 1..=3u8 {
                                let w = ui.ctx().animate_value_with_time(ui.scope_id().with(("tour_dot", k)), if k == stage { 22.0 } else { 8.0 }, 0.35);
                                let (r, _) = ui.allocate_exact_size(vec2(w, 8.0), Sense::hover());
                                let col = if k <= stage { kit::accent() } else { kit::faint() };
                                ui.painter().rect_filled(r, 4.0, col);
                            }
                            ui.add_space(4.0);
                            ui.label(RichText::new(format!("Step {stage} of 3")).size(13.0).color(kit::accent()));
                            ui.label(RichText::new("·").size(13.0).color(dim()));
                            if ui.link(RichText::new("Skip the tour").size(13.0).color(dim())).clicked() {
                                self.saved.tour = DONE;
                            }
                        });
                        ui.label(b(title, 19.0));
                        ui.label(RichText::new(sub).size(15.0).color(dim()));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        match stage {
                            CLUTTER => {
                                if primary(ui, "SCAN", !busy) {
                                    go = Some(Page::Clutter);
                                }
                            }
                            BROWSERS => {
                                if primary(ui, "SCAN", !busy) {
                                    go = Some(Page::Browsers);
                                }
                                if kit::outline(ui, "SKIP") {
                                    self.saved.tour = DEEPER;
                                }
                            }
                            _ => {
                                if primary(ui, "PHOTOS", !busy) {
                                    go = Some(Page::Photos);
                                }
                                if primary(ui, "DUPLICATES", !busy) {
                                    go = Some(Page::Duplicates);
                                }
                                if kit::outline(ui, "SKIP") {
                                    self.saved.tour = DONE;
                                }
                            }
                        }
                    });
                });
            });
        });
        go
    }
}

/// The step a finished cleaning moves the tour to.
pub fn after(stage: u8, page: Page) -> u8 {
    match (stage, page) {
        (CLUTTER, Page::Clutter) => BROWSERS,
        (BROWSERS, Page::Browsers) => DEEPER,
        (DEEPER, Page::Duplicates | Page::Photos) => DONE,
        (s, Page::Super) if s < DONE => DONE,
        (s, _) => s,
    }
}

#[cfg(test)]
mod tests {
    use crate::Page;

    #[test]
    fn each_cleaning_moves_the_tour_on_only_at_its_step() {
        assert_eq!(super::after(super::CLUTTER, Page::Clutter), super::BROWSERS);
        assert_eq!(super::after(super::CLUTTER, Page::Browsers), super::CLUTTER);
        assert_eq!(super::after(super::BROWSERS, Page::Browsers), super::DEEPER);
        assert_eq!(super::after(super::DEEPER, Page::Photos), super::DONE);
        assert_eq!(super::after(super::DONE, Page::Clutter), super::DONE);
    }

    #[test]
    fn bold_parts_are_split() {
        let j = super::rich("Open the [b]Privacy[/b] tab", 14.0, 400.0);
        assert_eq!(j.text, "Open the Privacy tab");
        assert_eq!(j.sections.len(), 3);
    }
}

