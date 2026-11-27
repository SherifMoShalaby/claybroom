//! Flat illustrations for the intro pages, drawn with shapes (Claybroom's own artwork:
//! devices, yellow highlights, soft clouds, in clay tones).
//! Scenes are designed on a 100 × 75 grid and scale to any size, and move gently: sparkles
//! twinkle, clouds drift, and each scene's main object does its job in a slow loop (the magnifier
//! sweeps, the globe's rings turn, the rocket hovers). Motion is code, not video: `t` is seconds.

use crate::kit::dark;
use eframe::egui::{self, pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Scene {
    Quick,
    Clutter,
    Browser,
    Duplicates,
    Photos,
    Apps,
    Startup,
    Monitor,
    Super,
    /// The first-run Full Disk Access guide: a drive behind a padlock that opens.
    Access,
}

/// Which picture an intro page shows, by its title.
pub fn scene_for(title: &str) -> Option<Scene> {
    Some(match title {
        "Quick Clean" => Scene::Quick,
        "Clean clutter" => Scene::Clutter,
        "Browser data" => Scene::Browser,
        "Duplicate files" => Scene::Duplicates,
        "Photo analyzer" => Scene::Photos,
        "App uninstaller" => Scene::Apps,
        "Startup manager" => Scene::Startup,
        "System monitor" => Scene::Monitor,
        "Super Clean" => Scene::Super,
        _ => return None,
    })
}

struct Pal {
    deep: Color32,
    main: Color32,
    light: Color32,
    pale: Color32,
    gold: Color32,
    accent: Color32,
    paper: Color32,
    line: Color32,
    cloud: Color32,
}

fn pal() -> Pal {
    let c = Color32::from_rgb;
    Pal { deep: c(0x7a, 0x3b, 0x26), main: c(0xc6, 0x61, 0x3f), light: c(0xe8, 0xa3, 0x87), pale: c(0xf4, 0xe1, 0xd6), gold: c(0xe3, 0xb2, 0x5b), accent: c(0x8a, 0x9a, 0x5b), paper: c(0xff, 0xfd, 0xf8), line: c(0xb9, 0xae, 0x9c), cloud: if dark() { Color32::from_white_alpha(18) } else { Color32::from_white_alpha(150) } }
}

/// Drawing on the 100 × 75 grid.
struct Canvas<'a> {
    p: &'a Painter,
    o: Pos2,
    s: f32,
    t: f32,
}

/// A slow wave between -1 and 1: `speed` in turns per second, `phase` in turns.
fn wave(t: f32, speed: f32, phase: f32) -> f32 {
    ((t * speed + phase) * TAU).sin()
}

impl<'a> Canvas<'a> {
    /// The same canvas moved by (dx, dy) grid units, for a part that floats.
    fn shifted(&self, dx: f32, dy: f32) -> Canvas<'a> {
        Canvas { p: self.p, o: self.o + vec2(dx, dy) * self.s, s: self.s, t: self.t }
    }
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.o + vec2(x, y) * self.s
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, fill: Color32) {
        self.p.rect_filled(Rect::from_min_max(self.at(x0, y0), self.at(x1, y1)), r * self.s, fill);
    }
    fn frame(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, w: f32, c: Color32) {
        self.p.rect_stroke(Rect::from_min_max(self.at(x0, y0), self.at(x1, y1)), r * self.s, Stroke::new(w * self.s, c), egui::StrokeKind::Inside);
    }
    fn circle(&self, x: f32, y: f32, r: f32, fill: Color32) {
        self.p.circle_filled(self.at(x, y), r * self.s, fill);
    }
    fn ring(&self, x: f32, y: f32, r: f32, w: f32, c: Color32) {
        self.p.circle_stroke(self.at(x, y), r * self.s, Stroke::new(w * self.s, c));
    }
    fn poly(&self, pts: &[(f32, f32)], fill: Color32) {
        self.p.add(Shape::convex_polygon(pts.iter().map(|&(x, y)| self.at(x, y)).collect(), fill, Stroke::NONE));
    }
    fn line(&self, pts: &[(f32, f32)], w: f32, c: Color32) {
        self.p.add(Shape::line(pts.iter().map(|&(x, y)| self.at(x, y)).collect(), Stroke::new(w * self.s, c)));
    }
    /// An ellipse turned by `angle` (radians): filled when `w` is 0, else outlined.
    fn ellipse(&self, x: f32, y: f32, rx: f32, ry: f32, angle: f32, fill: Color32, w: f32) {
        let pts: Vec<Pos2> = (0..64)
            .map(|i| {
                let t = i as f32 / 64.0 * TAU;
                let (ex, ey) = (rx * t.cos(), ry * t.sin());
                self.at(x + ex * angle.cos() - ey * angle.sin(), y + ex * angle.sin() + ey * angle.cos())
            })
            .collect();
        if w > 0.0 {
            self.p.add(Shape::closed_line(pts, Stroke::new(w * self.s, fill)));
        } else {
            self.p.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
        }
    }
    /// A four-pointed sparkle that twinkles (each at its own pace, from where it sits).
    fn sparkle(&self, x: f32, y: f32, r: f32, c: Color32) {
        let r = r * (0.78 + 0.22 * wave(self.t, 0.45, x * 0.037 + y * 0.051));
        self.poly(&[(x, y - r), (x + r * 0.28, y), (x, y + r), (x - r * 0.28, y)], c);
        self.poly(&[(x - r, y), (x, y - r * 0.28), (x + r, y), (x, y + r * 0.28)], c);
    }
    /// A soft cloud behind the scene.
    fn cloud(&self, x: f32, y: f32, w: f32, c: Color32) {
        self.ellipse(x, y, w * 0.5, w * 0.22, 0.0, c, 0.0);
        self.circle(x - w * 0.18, y - w * 0.1, w * 0.17, c);
        self.circle(x + w * 0.1, y - w * 0.16, w * 0.22, c);
    }
    /// A document card with text lines.
    fn doc(&self, x0: f32, y0: f32, x1: f32, y1: f32, k: &Pal) {
        self.rect(x0, y0, x1, y1, 1.5, k.paper);
        self.frame(x0, y0, x1, y1, 1.5, 0.5, k.line);
        let mut y = y0 + 4.0;
        while y < y1 - 3.0 {
            let end = if (y as i32) % 3 == 0 { x1 - 6.0 } else { x1 - 3.0 };
            self.line(&[(x0 + 3.0, y), (end, y)], 0.8, k.line);
            y += 3.2;
        }
    }
    /// A photo card: frame, sky, hills and sun.
    fn photo(&self, x0: f32, y0: f32, x1: f32, y1: f32, k: &Pal, faded: bool) {
        let a = if faded { 0.45 } else { 1.0 };
        self.rect(x0, y0, x1, y1, 1.2, k.paper);
        self.rect(x0 + 1.5, y0 + 1.5, x1 - 1.5, y1 - 4.0, 0.8, k.light.gamma_multiply(a));
        let (w, b) = (x1 - x0, y1 - 4.0);
        self.poly(&[(x0 + 1.5, b), (x0 + w * 0.42, y0 + (b - y0) * 0.4), (x0 + w * 0.7, b)], k.main.gamma_multiply(a));
        self.poly(&[(x0 + w * 0.45, b), (x0 + w * 0.72, y0 + (b - y0) * 0.55), (x1 - 1.5, b)], k.deep.gamma_multiply(a));
        self.circle(x0 + w * 0.75, y0 + (b - y0) * 0.3, w * 0.09, k.gold.gamma_multiply(a));
    }
    /// A laptop: lid with screen and a base.
    fn laptop(&self, x0: f32, y0: f32, x1: f32, y1: f32, k: &Pal) -> Rect {
        self.rect(x0, y0, x1, y1, 2.5, k.main);
        self.rect(x0 + 2.5, y0 + 2.5, x1 - 2.5, y1 - 1.0, 1.0, k.deep);
        self.poly(&[(x0 - 7.0, y1), (x1 + 7.0, y1), (x1 + 4.0, y1 + 4.0), (x0 - 4.0, y1 + 4.0)], k.main);
        self.rect(x0 - 4.0, y1 + 4.0, x1 + 4.0, y1 + 5.5, 1.0, k.deep.gamma_multiply(0.6));
        Rect::from_min_max(self.at(x0 + 2.5, y0 + 2.5), self.at(x1 - 2.5, y1 - 1.0))
    }
    /// A light cone from a laptop screen up to its contents.
    fn glow(&self, bx: f32, by: f32, top_w: f32, top_y: f32, k: &Pal) {
        self.poly(&[(bx - 12.0, by), (bx + 12.0, by), (bx + top_w / 2.0, top_y), (bx - top_w / 2.0, top_y)], k.gold.gamma_multiply(0.22));
        self.ellipse(bx, by, 13.0, 2.2, 0.0, k.gold.gamma_multiply(0.75), 0.0);
    }
    /// A folder.
    fn folder(&self, x0: f32, y0: f32, x1: f32, y1: f32, k: &Pal) {
        self.rect(x0, y0, x0 + (x1 - x0) * 0.42, y0 + 4.0, 1.2, k.gold);
        self.rect(x0, y0 + 2.5, x1, y1, 1.5, k.gold);
        self.rect(x0, y0 + 5.5, x1, y0 + 6.5, 0.0, Color32::from_black_alpha(25));
    }
}

/// Draw `scene` filling `r` (keeps the 4:3 shape, anchored at the bottom right), `t` seconds
/// into its loop.
pub fn draw(p: &Painter, r: Rect, scene: Scene, t: f32) {
    let s = (r.width() / 100.0).min(r.height() / 75.0);
    let o = pos2(r.right() - 100.0 * s, r.bottom() - 75.0 * s);
    let c = Canvas { p, o, s, t };
    let k = pal();
    // Backdrop shared by every scene: drifting clouds, a softly breathing disc.
    c.cloud(78.0 + 2.0 * wave(t, 0.05, 0.0), 66.0, 40.0, k.cloud);
    c.cloud(18.0 + 1.5 * wave(t, 0.06, 0.4), 70.0, 26.0, k.cloud);
    c.circle(52.0, 40.0, 30.0 * (1.0 + 0.015 * wave(t, 0.1, 0.0)), k.pale.gamma_multiply(0.8));
    // The main object hovers a little.
    let f = c.shifted(0.0, 1.2 * wave(t, 0.18, 0.0));
    match scene {
        Scene::Quick => {
            f.doc(24.0, 14.0, 56.0, 62.0, &k);
            f.rect(62.0, 10.0, 88.0, 22.0, 3.0, k.paper);
            f.line(&[(66.0, 14.5), (83.0, 14.5)], 1.0, k.light);
            f.line(&[(66.0, 18.0), (78.0, 18.0)], 1.0, k.light);
            // The magnifier sweeps over the page, scanning.
            let m = c.shifted(-6.0 + 6.0 * wave(t, 0.12, 0.0), -4.0 + 4.0 * wave(t, 0.24, 0.25));
            m.circle(60.0, 44.0, 14.0, Color32::from_white_alpha(90));
            m.ring(60.0, 44.0, 14.0, 3.5, k.main);
            m.line(&[(70.0, 54.0), (82.0, 66.0)], 6.0, k.deep);
            c.sparkle(14.0, 18.0, 4.0, k.gold);
            c.sparkle(90.0, 34.0, 3.0, k.accent);
            c.sparkle(46.0, 6.0, 2.5, k.light);
        }
        Scene::Clutter => {
            // Crumpled papers bobbing out of a bin, and a check badge that pulses.
            for (i, (x, y, rr)) in [(38.0, 26.0, 7.0), (50.0, 22.0, 8.0), (61.0, 27.0, 6.5), (45.0, 30.0, 5.0)].into_iter().enumerate() {
                let y = y + 1.5 * wave(t, 0.3, i as f32 * 0.27);
                c.circle(x, y, rr, k.paper);
                c.ring(x, y, rr, 0.6, k.line);
                c.line(&[(x - rr * 0.5, y - rr * 0.2), (x, y + rr * 0.3), (x + rr * 0.4, y - rr * 0.3)], 0.6, k.line);
            }
            c.poly(&[(28.0, 32.0), (72.0, 32.0), (67.0, 72.0), (33.0, 72.0)], k.main);
            c.ellipse(50.0, 32.0, 22.0, 3.0, 0.0, k.deep, 0.0);
            for x in [40.0, 50.0, 60.0] {
                c.line(&[(x, 38.0), (x - (x - 50.0) * 0.12, 66.0)], 1.6, k.light.gamma_multiply(0.6));
            }
            let pop = 1.0 + 0.08 * wave(t, 0.5, 0.0).max(0.0);
            c.circle(72.0, 58.0, 9.0 * pop, k.gold);
            c.line(&[(67.5, 58.0), (71.0, 61.5), (77.0, 54.5)], 2.2, k.paper);
            c.sparkle(20.0, 20.0, 4.0, k.gold);
            c.sparkle(84.0, 26.0, 3.0, k.light);
        }
        Scene::Browser => {
            // The globe hovers while its rings turn.
            f.circle(50.0, 38.0, 22.0, k.accent);
            f.ellipse(50.0, 38.0, 9.0, 22.0, 0.0, k.paper.gamma_multiply(0.55), 1.2);
            f.ellipse(50.0, 38.0, 22.0, 22.0, 0.0, k.paper.gamma_multiply(0.55), 1.2);
            for dy in [-11.0, 0.0, 11.0] {
                let half = (22.0f32.powi(2) - (dy as f32).powi(2)).sqrt();
                f.line(&[(50.0 - half, 38.0 + dy), (50.0 + half, 38.0 + dy)], 1.2, k.paper.gamma_multiply(0.55));
            }
            f.ellipse(50.0, 38.0, 38.0, 9.0, -0.35 + 0.12 * wave(t, 0.07, 0.0), k.gold, 2.4);
            f.ellipse(50.0, 38.0, 33.0, 7.0, 0.5 - 0.12 * wave(t, 0.06, 0.3), k.gold.gamma_multiply(0.7), 1.6);
            // A little moon travelling round the outer ring.
            let a = t * 0.35 * TAU;
            let (ex, ey) = (38.0 * a.cos(), 9.0 * a.sin());
            let ang: f32 = -0.35;
            f.circle(50.0 + ex * ang.cos() - ey * ang.sin(), 38.0 + ex * ang.sin() + ey * ang.cos(), 2.4, k.gold);
            c.circle(18.0, 52.0, 2.2, k.light);
            c.sparkle(22.0, 14.0, 3.5, k.gold);
            c.sparkle(82.0, 60.0, 3.0, k.light);
        }
        Scene::Duplicates => {
            c.laptop(28.0, 36.0, 78.0, 66.0, &k);
            // The screen's light breathes; the files float in it at their own pace.
            let k2 = Pal { gold: k.gold.gamma_multiply(0.8 + 0.2 * wave(t, 0.25, 0.0)), ..pal() };
            c.glow(53.0, 63.0, 46.0, 10.0, &k2);
            c.shifted(0.0, 1.6 * wave(t, 0.2, 0.0)).photo(30.0, 16.0, 48.0, 38.0, &k, false);
            c.shifted(0.0, 1.6 * wave(t, 0.2, 0.33)).doc(56.0, 10.0, 72.0, 32.0, &k);
            c.shifted(0.0, 1.0 * wave(t, 0.2, 0.66)).folder(40.0, 36.0, 64.0, 56.0, &k);
            c.sparkle(18.0, 26.0, 3.5, k.gold);
        }
        Scene::Photos => {
            // The balance tips gently, weighing the photos.
            let tilt = 3.0 * wave(t, 0.12, 0.0);
            c.rect(48.5, 22.0, 51.5, 66.0, 1.0, k.deep);
            c.poly(&[(38.0, 66.0), (62.0, 66.0), (66.0, 71.0), (34.0, 71.0)], k.main);
            let ends = [(18.0, 26.0 + tilt), (82.0, 20.0 - tilt)];
            c.line(&[ends[0], ends[1]], 3.0, k.deep);
            c.circle(50.0, 22.0 + (ends[0].1 + ends[1].1) / 2.0 - 23.0, 3.0, k.gold);
            for (x, y) in [(20.0, 26.0 + tilt * 0.94), (80.0, 20.0 - tilt * 0.94)] {
                c.line(&[(x, y), (x - 9.0, y + 16.0)], 0.8, k.line);
                c.line(&[(x, y), (x + 9.0, y + 16.0)], 0.8, k.line);
                c.poly(&[(x - 12.0, y + 16.0), (x + 12.0, y + 16.0), (x + 8.0, y + 21.0), (x - 8.0, y + 21.0)], k.main);
            }
            c.photo(11.0, 28.0 + tilt * 0.94, 29.0, 42.5 + tilt * 0.94, &k, true);
            c.photo(71.0, 22.0 - tilt * 0.94, 89.0, 36.5 - tilt * 0.94, &k, false);
            c.sparkle(88.0, 10.0, 3.0, k.gold);
        }
        Scene::Apps => {
            let scr = c.laptop(26.0, 30.0, 76.0, 62.0, &k);
            // A gauge on the screen; its needle swings up as space frees.
            let cx = scr.center().x;
            let cy = scr.bottom() - 4.0 * s;
            let arc: Vec<Pos2> = (0..=24).map(|i| pos2(cx, cy) + Vec2::angled(std::f32::consts::PI * (1.0 + i as f32 / 24.0)) * 13.0 * s).collect();
            p.add(Shape::line(arc, Stroke::new(3.0 * s, k.light)));
            let needle = -0.7 - 0.6 * (0.5 + 0.5 * wave(t, 0.15, 0.0));
            p.add(Shape::line(vec![pos2(cx, cy), pos2(cx, cy) + Vec2::angled(needle) * 11.0 * s], Stroke::new(2.0 * s, k.gold)));
            c.circle((cx - o.x) / s, (cy - o.y) / s, 2.0, k.gold);
            for (i, (x, col)) in [(20.0, k.accent), (36.0, k.gold), (52.0, k.light)].into_iter().enumerate() {
                let y = 12.0 + i as f32 * 2.0 + 1.2 * wave(t, 0.25, i as f32 * 0.2);
                c.rect(x, y, x + 11.0, y + 11.0, 2.5, col);
                c.rect(x + 3.0, y + 3.0, x + 8.0, y + 8.0, 1.0, k.paper.gamma_multiply(0.7));
            }
            // The unused app floats up and fades out, again and again.
            let u = (t * 0.2).fract();
            let (dy, fade) = (-10.0 * u, 1.0 - u);
            c.rect(70.0, 8.0 + dy, 81.0, 19.0 + dy, 2.5, k.main.gamma_multiply(0.45 * fade));
            c.line(&[(72.5, 10.5 + dy), (78.5, 16.5 + dy)], 1.5, k.paper.gamma_multiply(fade));
            c.line(&[(78.5, 10.5 + dy), (72.5, 16.5 + dy)], 1.5, k.paper.gamma_multiply(fade));
            c.sparkle(90.0, 30.0, 3.0, k.gold);
        }
        Scene::Startup => {
            // A rocket hovering on a flickering flame, speed lines streaming past.
            let flame = 1.0 + 0.18 * wave(t, 2.2, 0.0);
            f.poly(&[(44.0, 52.0), (56.0, 52.0), (50.0, 52.0 + 18.0 * flame)], k.gold);
            f.poly(&[(46.0, 52.0), (54.0, 52.0), (50.0, 52.0 + 11.0 * flame)], Color32::from_rgb(0xf6, 0x8b, 0x3c));
            f.poly(&[(42.0, 40.0), (42.0, 54.0), (34.0, 58.0), (36.0, 46.0)], k.main);
            f.poly(&[(58.0, 40.0), (58.0, 54.0), (66.0, 58.0), (64.0, 46.0)], k.main);
            f.rect(42.0, 18.0, 58.0, 54.0, 6.0, k.paper);
            f.frame(42.0, 18.0, 58.0, 54.0, 6.0, 0.6, k.line);
            f.poly(&[(43.0, 22.0), (57.0, 22.0), (50.0, 6.0)], k.main);
            f.circle(50.0, 31.0, 4.5, k.deep);
            f.circle(50.0, 31.0, 3.0, k.light);
            for (i, (x, y0)) in [(30.0, 30.0), (70.0, 26.0), (24.0, 48.0), (76.0, 46.0)].into_iter().enumerate() {
                let d = ((t * 0.6 + i as f32 * 0.25).fract()) * 16.0;
                c.line(&[(x, y0 + d - 6.0), (x, y0 + d + 6.0)], 1.2, k.light.gamma_multiply(1.0 - d / 16.0));
            }
            c.cloud(50.0, 72.0, 30.0, k.paper.gamma_multiply(0.9));
            c.sparkle(84.0, 14.0, 3.5, k.gold);
            c.sparkle(16.0, 20.0, 2.5, k.accent);
        }
        Scene::Monitor => {
            c.rect(16.0, 12.0, 84.0, 56.0, 3.0, k.main);
            c.rect(19.0, 15.0, 81.0, 52.0, 1.5, k.paper);
            c.rect(46.0, 56.0, 54.0, 64.0, 0.0, k.deep);
            c.rect(36.0, 64.0, 64.0, 67.0, 1.5, k.deep);
            // Live bars and a pulse line, like the System monitor itself.
            for (i, h) in [10.0, 16.0, 8.0, 20.0, 13.0].into_iter().enumerate() {
                let x = 24.0 + i as f32 * 6.0;
                let h = h * (0.75 + 0.25 * wave(t, 0.4, i as f32 * 0.31));
                c.rect(x, 48.0 - h, x + 4.0, 48.0, 1.0, if i == 3 { k.gold } else { k.light });
            }
            let shift = 3.0 * wave(t, 0.3, 0.0);
            c.line(&[(54.0, 36.0), (58.0, 36.0), (61.0, 26.0 - shift), (65.0, 44.0 + shift), (68.0, 32.0), (71.0, 36.0), (77.0, 36.0)], 1.8, k.accent);
            c.rect(86.0, 24.0, 91.0, 50.0, 2.5, k.paper);
            let level = 34.0 - 4.0 * wave(t, 0.1, 0.0);
            c.rect(87.5, level, 89.5, 48.0, 1.0, k.gold);
            c.circle(88.5, 50.0, 3.5, k.gold);
            c.sparkle(10.0, 18.0, 3.0, k.gold);
        }
        Scene::Access => {
            // A drive behind a shield; the padlock in front opens and closes, a key turning it.
            let open = (0.5 + 0.5 * wave(t, 0.22, 0.0)).powf(2.0);
            c.poly(&[(50.0, 8.0), (74.0, 16.0), (72.0, 42.0), (50.0, 62.0), (28.0, 42.0), (26.0, 16.0)], k.pale);
            f.rect(30.0, 24.0, 70.0, 50.0, 4.0, k.main);
            f.rect(33.0, 27.0, 67.0, 39.0, 2.0, k.deep);
            for i in 0..4 {
                let x = 37.0 + i as f32 * 7.0;
                f.line(&[(x, 30.0), (x, 36.0)], 1.0, k.light.gamma_multiply(0.7));
            }
            f.circle(62.0, 44.5, 1.6, if open > 0.6 { k.gold } else { k.light });
            // Padlock: the shackle lifts as it opens, light spills out.
            let lift = 6.0 * open;
            let (lx, ly) = (66.0, 50.0);
            let shackle: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + i as f32 / 16.0);
                    (lx + 6.0 * a.cos(), ly - 2.0 - lift + 6.0 * a.sin())
                })
                .collect();
            let mut path = vec![(lx - 6.0, ly + 1.0)];
            path.extend(shackle);
            path.push((lx + 6.0, ly + 1.0 - lift));
            c.line(&path, 2.2, k.deep);
            c.rect(lx - 8.5, ly, lx + 8.5, ly + 13.0, 2.5, k.gold);
            c.circle(lx, ly + 5.5, 1.8, k.deep);
            c.rect(lx - 0.7, ly + 6.0, lx + 0.7, ly + 9.5, 0.3, k.deep);
            for (i, a) in [-0.9f32, -0.45, 0.0, 0.45, 0.9].into_iter().enumerate() {
                let len = 5.0 * open;
                let (dx, dy) = (a.sin(), -a.cos());
                let r0 = 12.0 + 1.5 * wave(t, 0.5, i as f32 * 0.2);
                c.line(&[(lx + dx * r0, ly + 6.0 + dy * r0), (lx + dx * (r0 + len), ly + 6.0 + dy * (r0 + len))], 1.2, k.gold.gamma_multiply(open));
            }
            c.sparkle(18.0, 16.0, 3.5, k.gold);
            c.sparkle(86.0, 26.0, 3.0, k.accent);
            c.sparkle(22.0, 58.0, 2.5, k.light);
        }
        Scene::Super => {
            // Every module circling one computer: junk, browser, duplicates, photos and speed,
            // on a tilted orbit; the ones behind pass behind the laptop.
            let (cx, cy, rx, ry) = (52.0, 40.0, 43.0, 25.0);
            let badges: Vec<(f32, usize)> = (0..5).map(|i| ((t * 0.07 + i as f32 / 5.0) * TAU, i)).collect();
            let place = |a: f32| (cx + rx * a.cos(), cy + ry * a.sin());
            c.ellipse(cx, cy, rx, ry, 0.0, k.line.gamma_multiply(0.6), 0.6);
            for &(a, i) in badges.iter().filter(|(a, _)| a.sin() < 0.0) {
                let (x, y) = place(a);
                badge(&c, x, y, 0.8, i, &k, true);
            }
            f.laptop(32.0, 30.0, 72.0, 56.0, &k);
            // The screen's sparkle glows and turns slowly.
            let glow = 0.5 + 0.5 * wave(t, 0.3, 0.0);
            f.circle(52.0, 42.5, 8.0 + 2.0 * glow, k.gold.gamma_multiply(0.18 + 0.12 * glow));
            let r = 7.0 * (0.9 + 0.1 * glow);
            f.poly(&[(52.0, 42.5 - r), (52.0 + r * 0.3, 42.5), (52.0, 42.5 + r), (52.0 - r * 0.3, 42.5)], k.gold);
            f.poly(&[(52.0 - r, 42.5), (52.0, 42.5 - r * 0.3), (52.0 + r, 42.5), (52.0, 42.5 + r * 0.3)], k.gold);
            f.circle(60.0, 37.0, 1.4 * (0.6 + 0.4 * glow), k.paper);
            for &(a, i) in badges.iter().filter(|(a, _)| a.sin() >= 0.0) {
                let (x, y) = place(a);
                badge(&c, x, y, 1.0, i, &k, false);
            }
            c.sparkle(12.0, 14.0, 4.0, k.gold);
            c.sparkle(90.0, 12.0, 3.0, k.accent);
            c.sparkle(88.0, 58.0, 2.5, k.light);
        }
    }
}

/// One module's badge on Super Clean's orbit: a disc with a tiny picture of what it cleans.
fn badge(c: &Canvas, x: f32, y: f32, scale: f32, which: usize, k: &Pal, behind: bool) {
    let r = 7.0 * scale;
    let fade = if behind { 0.55 } else { 1.0 };
    let fill = [k.main, k.accent, k.gold, k.light, k.deep][which % 5].gamma_multiply(fade);
    c.circle(x, y + 0.8, r, Color32::from_black_alpha(if behind { 10 } else { 22 }));
    c.circle(x, y, r, fill);
    let ink = k.paper.gamma_multiply(fade);
    let u = r / 7.0;
    match which % 5 {
        // Junk: a crumpled ball.
        0 => {
            c.circle(x, y, 3.2 * u, ink);
            c.line(&[(x - 1.8 * u, y - 0.6 * u), (x, y + 1.0 * u), (x + 1.6 * u, y - 1.0 * u)], 0.5, fill);
        }
        // Browser: a globe.
        1 => {
            c.ring(x, y, 3.4 * u, 0.7, ink);
            c.ellipse(x, y, 1.4 * u, 3.4 * u, 0.0, ink, 0.6);
            c.line(&[(x - 3.4 * u, y), (x + 3.4 * u, y)], 0.6, ink);
        }
        // Duplicates: two cards.
        2 => {
            c.rect(x - 3.2 * u, y - 2.6 * u, x + 0.8 * u, y + 2.2 * u, 0.6, ink.gamma_multiply(0.7));
            c.rect(x - 1.0 * u, y - 1.6 * u, x + 3.0 * u, y + 3.2 * u, 0.6, ink);
        }
        // Photos: hills and a sun.
        3 => {
            c.rect(x - 3.4 * u, y - 2.6 * u, x + 3.4 * u, y + 2.6 * u, 0.6, ink);
            c.poly(&[(x - 3.0 * u, y + 2.2 * u), (x - 0.6 * u, y - 0.8 * u), (x + 1.6 * u, y + 2.2 * u)], fill);
            c.circle(x + 1.8 * u, y - 1.2 * u, 0.8 * u, fill);
        }
        // Speed: a gauge.
        _ => {
            let pts: Vec<(f32, f32)> = (0..=10).map(|i| {
                let a = std::f32::consts::PI * (1.0 + i as f32 / 10.0);
                (x + 3.2 * u * a.cos(), y + 1.4 * u + 3.2 * u * a.sin())
            }).collect();
            c.line(&pts, 0.8, ink);
            c.line(&[(x, y + 1.4 * u), (x + 2.0 * u, y - 0.8 * u)], 0.8, ink);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_intro_page_has_a_picture() {
        for t in ["Quick Clean", "Clean clutter", "Browser data", "Duplicate files", "Photo analyzer", "App uninstaller", "Startup manager", "System monitor", "Super Clean"] {
            assert!(super::scene_for(t).is_some(), "{t}");
        }
        assert!(super::scene_for("Settings").is_none());
    }
}
