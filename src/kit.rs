//! Look and feel: colours, fonts, line icons, cards and buttons.

use eframe::egui::{self, pos2, vec2, Color32, CornerRadius, FontFamily, Margin, Pos2, Rect, RichText, Stroke, Vec2};
use std::f32::consts::TAU;

/// Claybroom's warm, Claude-inspired look (cream paper, clay accent, serif headings) in light
/// (0) or dark (1). Chosen in Settings › General › Appearance.
static THEME: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// Settings › General › Appearance choices (Settings.appearance indexes them).
pub const THEMES: [&str; 3] = ["Match system", "Light", "Dark"];

pub fn set_theme(t: u8) {
    THEME.store(t.min(1), std::sync::atomic::Ordering::Relaxed);
}

pub fn dark() -> bool {
    THEME.load(std::sync::atomic::Ordering::Relaxed) == 1
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn pick(light: Color32, dark_c: Color32) -> Color32 {
    if dark() { dark_c } else { light }
}

/// Body text.
pub fn ink() -> Color32 {
    pick(rgb(0x14, 0x14, 0x13), rgb(0xf3, 0xf1, 0xea))
}
/// Secondary text.
pub fn dim() -> Color32 {
    pick(rgb(0x6b, 0x6a, 0x64), rgb(0xa6, 0xa3, 0x9a))
}
/// Buttons, selection, links: clay.
pub fn accent() -> Color32 {
    pick(rgb(0xc6, 0x61, 0x3f), rgb(0xd9, 0x77, 0x57))
}
/// The page background.
pub fn bg() -> Color32 {
    pick(rgb(0xfa, 0xf9, 0xf5), rgb(0x26, 0x26, 0x24))
}
/// Cards, boxes and the menu-bar panel: paper white, or a raised grey in dark.
pub fn paper() -> Color32 {
    pick(Color32::WHITE, rgb(0x30, 0x30, 0x2e))
}
/// Quiet fills: icon wells, tracks, placeholders.
pub fn soft() -> Color32 {
    pick(rgb(0xe9, 0xe6, 0xdc), rgb(0x3a, 0x39, 0x35))
}
/// Faint marks: empty ticks, unselected counts.
pub fn faint() -> Color32 {
    pick(rgb(0xc9, 0xc5, 0xba), rgb(0x5a, 0x58, 0x52))
}
pub fn rail() -> Color32 {
    pick(rgb(0xf0, 0xee, 0xe6), rgb(0x1f, 0x1e, 0x1d))
}
pub fn rail_active() -> Color32 {
    pick(rgb(0xe3, 0xdf, 0xd2), rgb(0x3a, 0x39, 0x35))
}
/// Tile fill (premultiplied in light).
pub fn tile_fill() -> Color32 {
    pick(Color32::from_rgba_premultiplied(0xc4, 0xc3, 0xbf, 0xc8), rgb(0x30, 0x30, 0x2e))
}
pub fn good() -> Color32 {
    pick(rgb(0x5a, 0x8a, 0x4e), rgb(0x86, 0xb3, 0x6f))
}
pub fn bad() -> Color32 {
    pick(rgb(0xb8, 0x45, 0x3a), rgb(0xe0, 0x6c, 0x5e))
}
pub fn warn() -> Color32 {
    pick(rgb(0xa8, 0x55, 0x1c), rgb(0xe0, 0x9a, 0x5a))
}
/// Sizes in results.
pub fn size_color() -> Color32 {
    pick(rgb(0xd9, 0x77, 0x57), rgb(0xe8, 0x8c, 0x6c))
}
/// The scanning ring.
pub fn ring() -> Color32 {
    rgb(0xd9, 0x77, 0x57)
}
pub fn disabled() -> Color32 {
    pick(rgb(0xe8, 0xe6, 0xdc), rgb(0x3a, 0x39, 0x35))
}
pub fn line() -> Color32 {
    pick(rgb(0xe8, 0xe6, 0xdc), rgb(0x3e, 0x3d, 0x38))
}
pub fn radius() -> u8 {
    8
}

/// Big headings: a bold sans, or a serif on Warm.
pub fn head_family() -> FontFamily {
    FontFamily::Name("serif".into())
}

pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Bold text in the heading font.
pub fn b(t: impl Into<String>, size: f32) -> RichText {
    RichText::new(t).family(bold_family()).size(size).color(ink())
}

pub fn muted(t: impl Into<String>) -> RichText {
    RichText::new(t).color(dim())
}

pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("roboto".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/Roboto-Regular.ttf"))));
    fonts.font_data.insert("roboto-bold".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/Roboto-Bold.ttf"))));
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "roboto".into());
    fonts.font_data.insert("serif".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/SourceSerif4-Semibold.ttf"))));
    let mut bold = vec!["roboto-bold".to_string()];
    bold.extend(fallback.clone());
    fonts.families.insert(bold_family(), bold);
    let mut serif = vec!["serif".to_string()];
    serif.extend(fallback);
    fonts.families.insert(FontFamily::Name("serif".into()), serif);
    ctx.set_fonts(fonts);
    apply_style(ctx);
}

/// Colours and shapes of egui's own widgets for the current theme.
pub fn apply_style(ctx: &egui::Context) {
    // Pin egui to our choice so it doesn't swap styles on its own.
    ctx.set_theme(if dark() { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.set_visuals(if dark() { egui::Visuals::dark() } else { egui::Visuals::light() });
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(10.0, 8.0);
        s.spacing.button_padding = vec2(14.0, 7.0);
        s.spacing.interact_size.y = 26.0;
        s.visuals.override_text_color = Some(ink());
        s.visuals.panel_fill = bg();
        s.visuals.window_fill = paper();
        s.visuals.extreme_bg_color = paper();
        s.visuals.selection.bg_fill = accent();
        s.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        s.visuals.hyperlink_color = accent();
        for w in [&mut s.visuals.widgets.inactive, &mut s.visuals.widgets.hovered, &mut s.visuals.widgets.active] {
            w.corner_radius = CornerRadius::same(8);
        }
        for (t, size) in [(egui::TextStyle::Body, 15.0), (egui::TextStyle::Button, 14.0), (egui::TextStyle::Small, 12.5), (egui::TextStyle::Heading, 26.0)] {
            if let Some(f) = s.text_styles.get_mut(&t) {
                f.size = size;
            }
        }
    });
}

/// The soft paper gradient of the main area.
pub fn gradient(painter: &egui::Painter, r: Rect) {
    let mut m = egui::epaint::Mesh::default();
    let tl = pick(rgb(0xfa, 0xf9, 0xf5), rgb(0x26, 0x26, 0x24));
    let tr = pick(rgb(0xf7, 0xf5, 0xee), rgb(0x28, 0x27, 0x25));
    let bl = pick(rgb(0xf6, 0xf4, 0xed), rgb(0x24, 0x24, 0x22));
    let br = pick(rgb(0xec, 0xe7, 0xda), rgb(0x2e, 0x2b, 0x27));
    for (p, c) in [(r.left_top(), tl), (r.right_top(), tr), (r.right_bottom(), br), (r.left_bottom(), bl)] {
        m.colored_vertex(p, c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    painter.add(m);
}

/// A rounded, translucent panel.
pub fn tile<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(tile_fill())
        .corner_radius(CornerRadius::same(26))
        .inner_margin(Margin::symmetric(26, 20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A white panel for lists and results.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(pick(Color32::from_white_alpha(0xd8), paper()))
        .corner_radius(CornerRadius::same(14))
        .stroke(Stroke::new(1.0, line()))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// The main button: square-ish, bold capitals.
pub fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    let (fill, fg) = if enabled { (accent(), Color32::WHITE) } else { (disabled(), Color32::from_rgb(0x8a, 0x8d, 0x96)) };
    let btn = egui::Button::new(RichText::new(text).family(bold_family()).size(14.0).color(fg))
        .fill(fill)
        .corner_radius(CornerRadius::same(radius()))
        .min_size(vec2(84.0, 30.0));
    ui.add_enabled(enabled, btn).clicked()
}

/// The larger call-to-action button of intro pages.
pub fn cta(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    let (fill, fg) = if enabled { (accent(), Color32::WHITE) } else { (disabled(), Color32::from_rgb(0x8a, 0x8d, 0x96)) };
    let btn = egui::Button::new(RichText::new(text).family(bold_family()).size(16.0).color(fg))
        .fill(fill)
        .corner_radius(CornerRadius::same(radius()))
        .min_size(vec2(190.0, 42.0));
    ui.add_enabled(enabled, btn).clicked()
}

pub fn outline(ui: &mut egui::Ui, text: &str) -> bool {
    let btn = egui::Button::new(RichText::new(text).family(bold_family()).size(14.0).color(ink()))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.2, Color32::from_rgb(0x7a, 0x80, 0x8c)))
        .corner_radius(CornerRadius::same(radius()))
        .min_size(vec2(100.0, 34.0));
    ui.add(btn).clicked()
}

/// Centered page heading and subtitle.
pub fn heading(ui: &mut egui::Ui, t: &str, sub: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(10.0);
        ui.label(RichText::new(t).family(head_family()).size(36.0).color(ink()));
        if !sub.is_empty() {
            ui.add_space(2.0);
            ui.label(RichText::new(sub).size(18.0).color(dim()));
        }
    });
    ui.add_space(18.0);
}

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Home,
    Quick,
    Clutter,
    Globe,
    Duplicates,
    Trash,
    Startup,
    Monitor,
    Stats,
    Gear,
    Folder,
    Speed,
    Shield,
    Broom,
    Check,
    Picture,
    Video,
    Music,
    Lock,
    Battery,
    Thermo,
    Sparkle,
}

fn ellipse(c: Pos2, rx: f32, ry: f32) -> Vec<Pos2> {
    (0..32).map(|i| i as f32 / 32.0 * TAU).map(|a| pos2(c.x + rx * a.cos(), c.y + ry * a.sin())).collect()
}

/// Draw a thin line icon inside `r` (designed on a 24×24 grid).
pub fn icon(p: &egui::Painter, r: Rect, which: Icon, color: Color32) {
    let s = r.width() / 24.0;
    let at = |x: f32, y: f32| r.min + vec2(x * s, y * s);
    let st = Stroke::new(1.6 * s.max(0.8), color);
    let line = |pts: &[(f32, f32)]| p.add(egui::Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), st));
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32, rad: f32| {
        p.rect_stroke(Rect::from_min_max(at(x0, y0), at(x1, y1)), rad * s, st, egui::StrokeKind::Middle)
    };
    match which {
        Icon::Home => {
            line(&[(2.5, 11.5), (12.0, 3.0), (21.5, 11.5)]);
            line(&[(5.0, 9.5), (5.0, 21.0), (19.0, 21.0), (19.0, 9.5)]);
            line(&[(10.0, 21.0), (10.0, 15.0), (14.0, 15.0), (14.0, 21.0)]);
        }
        Icon::Quick => {
            rect(3.0, 4.0, 21.0, 16.0, 1.5);
            line(&[(1.0, 19.0), (23.0, 19.0)]);
            p.add(egui::Shape::closed_line(ellipse(at(12.0, 10.0), 5.0 * s, 3.0 * s), st));
            p.circle_filled(at(12.0, 10.0), 1.6 * s, color);
        }
        Icon::Clutter => {
            rect(2.5, 3.5, 21.5, 8.0, 1.0);
            line(&[(4.5, 8.0), (4.5, 21.0), (19.5, 21.0), (19.5, 8.0)]);
            line(&[(10.0, 12.5), (14.0, 16.5)]);
            line(&[(14.0, 12.5), (10.0, 16.5)]);
        }
        Icon::Globe => {
            p.circle_stroke(at(12.0, 12.0), 9.5 * s, st);
            p.add(egui::Shape::closed_line(ellipse(at(12.0, 12.0), 4.2 * s, 9.5 * s), st));
            line(&[(2.5, 12.0), (21.5, 12.0)]);
            line(&[(12.0, 2.5), (12.0, 21.5)]);
            line(&[(4.0, 7.0), (20.0, 7.0)]);
            line(&[(4.0, 17.0), (20.0, 17.0)]);
        }
        Icon::Duplicates => {
            rect(3.0, 3.0, 11.0, 21.0, 1.0);
            rect(13.0, 3.0, 21.0, 21.0, 1.0);
            line(&[(4.0, 18.0), (7.0, 14.0), (10.0, 18.0)]);
            p.circle_stroke(at(17.0, 8.0), 1.6 * s, st);
        }
        Icon::Trash => {
            line(&[(3.0, 5.5), (21.0, 5.5)]);
            line(&[(9.0, 5.5), (9.0, 3.0), (15.0, 3.0), (15.0, 5.5)]);
            line(&[(5.0, 5.5), (6.0, 21.0), (18.0, 21.0), (19.0, 5.5)]);
            line(&[(10.0, 9.0), (10.0, 18.0)]);
            line(&[(14.0, 9.0), (14.0, 18.0)]);
        }
        Icon::Startup => {
            rect(5.0, 2.5, 21.0, 18.5, 1.5);
            line(&[(3.0, 6.0), (3.0, 21.0), (17.5, 21.0)]);
            for (y, x) in [(7.0, 15.0), (10.5, 10.0), (14.0, 16.0)] {
                line(&[(8.0, y), (18.0, y)]);
                line(&[(x, y - 1.5), (x, y + 1.5)]);
            }
        }
        Icon::Monitor => {
            rect(2.0, 4.0, 22.0, 17.0, 1.5);
            line(&[(1.0, 20.0), (23.0, 20.0)]);
            for (y, w) in [(8.0, 14.0), (11.0, 11.0), (14.0, 8.0)] {
                line(&[(5.0, y), (5.0 + w, y)]);
            }
        }
        Icon::Stats => {
            line(&[(2.0, 21.0), (22.0, 21.0)]);
            for (x, top) in [(4.5, 9.0), (8.5, 13.0), (12.5, 4.0), (16.5, 15.0), (20.0, 11.0)] {
                line(&[(x, 21.0), (x, top)]);
            }
        }
        Icon::Gear => {
            p.circle_stroke(at(12.0, 12.0), 3.2 * s, st);
            let pts: Vec<Pos2> = (0..48)
                .map(|i| {
                    let a = i as f32 / 48.0 * TAU;
                    let tooth = if (i / 3) % 2 == 0 { 9.5 } else { 7.5 };
                    at(12.0, 12.0) + Vec2::angled(a) * tooth * s
                })
                .collect();
            p.add(egui::Shape::closed_line(pts, st));
        }
        Icon::Folder => {
            line(&[(2.5, 19.5), (2.5, 5.0), (9.0, 5.0), (11.0, 7.5), (21.5, 7.5), (21.5, 19.5), (2.5, 19.5)]);
            line(&[(2.5, 10.5), (21.5, 10.5)]);
        }
        Icon::Speed => {
            let arc: Vec<Pos2> = (0..=24).map(|i| at(12.0, 14.0) + Vec2::angled(TAU * (0.4 + 0.7 * i as f32 / 24.0)) * 9.0 * s).collect();
            p.add(egui::Shape::line(arc, st));
            line(&[(12.0, 14.0), (16.0, 9.0)]);
        }
        Icon::Shield => {
            line(&[(12.0, 2.5), (20.0, 5.5), (20.0, 11.0), (12.0, 21.5), (4.0, 11.0), (4.0, 5.5), (12.0, 2.5)]);
            line(&[(8.5, 11.5), (11.0, 14.0), (15.5, 9.0)]);
        }
        Icon::Broom => {
            line(&[(12.0, 2.5), (12.0, 10.0)]);
            line(&[(7.0, 10.0), (17.0, 10.0), (19.0, 21.0), (5.0, 21.0), (7.0, 10.0)]);
            line(&[(9.5, 15.0), (9.0, 21.0)]);
            line(&[(14.5, 15.0), (15.0, 21.0)]);
        }
        Icon::Check => {
            p.circle_stroke(at(12.0, 12.0), 9.5 * s, st);
            line(&[(7.5, 12.0), (10.8, 15.3), (16.5, 9.0)]);
        }
        Icon::Picture => {
            rect(2.0, 4.0, 22.0, 20.0, 1.5);
            line(&[(2.5, 17.0), (8.0, 11.0), (12.0, 15.0), (15.0, 12.0), (21.5, 18.0)]);
            p.circle_stroke(at(16.0, 8.5), 1.8 * s, st);
        }
        Icon::Video => {
            rect(2.0, 4.0, 22.0, 20.0, 1.5);
            line(&[(2.0, 7.5), (22.0, 7.5)]);
            line(&[(2.0, 16.5), (22.0, 16.5)]);
            line(&[(10.0, 9.5), (10.0, 14.5), (14.5, 12.0), (10.0, 9.5)]);
        }
        Icon::Music => {
            line(&[(9.0, 18.0), (9.0, 5.0), (20.0, 3.0), (20.0, 16.0)]);
            line(&[(9.0, 8.5), (20.0, 6.5)]);
            p.circle_stroke(at(6.5, 18.0), 2.6 * s, st);
            p.circle_stroke(at(17.5, 16.0), 2.6 * s, st);
        }
        Icon::Lock => {
            rect(5.0, 10.5, 19.0, 21.0, 1.5);
            line(&[(8.0, 10.5), (8.0, 7.0), (10.0, 4.0), (14.0, 4.0), (16.0, 7.0), (16.0, 10.5)]);
        }
        Icon::Battery => {
            rect(2.0, 7.0, 20.0, 17.0, 2.0);
            line(&[(22.0, 10.5), (22.0, 13.5)]);
            line(&[(5.0, 10.0), (5.0, 14.0)]);
            line(&[(8.0, 10.0), (8.0, 14.0)]);
        }
        Icon::Thermo => {
            line(&[(10.0, 15.0), (10.0, 4.0), (12.0, 2.5), (14.0, 4.0), (14.0, 15.0)]);
            p.circle_stroke(at(12.0, 18.0), 3.5 * s, st);
        }
        Icon::Sparkle => {
            line(&[(10.0, 3.0), (11.8, 9.2), (18.0, 11.0), (11.8, 12.8), (10.0, 19.0), (8.2, 12.8), (2.0, 11.0), (8.2, 9.2), (10.0, 3.0)]);
            line(&[(18.5, 15.0), (19.3, 17.7), (22.0, 18.5), (19.3, 19.3), (18.5, 22.0), (17.7, 19.3), (15.0, 18.5), (17.7, 17.7), (18.5, 15.0)]);
        }
    }
}

/// An icon in a rounded light square.
pub fn icon_box(ui: &mut egui::Ui, which: Icon, size: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let fill = pick(rgb(0xec, 0xe8, 0xdd), rgb(0x3a, 0x39, 0x35));
    ui.painter().rect_filled(r, 10.0, fill);
    icon(ui.painter(), r.shrink(size * 0.27), which, ink());
}

/// A soft placeholder picture for the empty bottom-right of intro pages (no artwork copied).
pub fn placeholder_art(p: &egui::Painter, r: Rect, accent: Color32) {
    // Every page keeps the clay tone.
    let _ = accent;
    let accent = rgb(0xd9, 0x77, 0x57);
    let c = r.center();
    p.circle_filled(c + vec2(30.0, 30.0), r.width() * 0.42, accent.gamma_multiply(0.10));
    p.circle_filled(c + vec2(-60.0, -40.0), r.width() * 0.16, accent.gamma_multiply(0.18));
    p.circle_filled(c + vec2(70.0, -80.0), r.width() * 0.08, accent.gamma_multiply(0.25));
    p.circle_filled(c + vec2(20.0, 20.0), r.width() * 0.22, accent.gamma_multiply(0.55));
}

/// The scanning ring: tick marks, a pale disc, green ticks and arc for
/// progress clockwise from 12 o'clock, slowly turning ellipses and the percentage in the middle.
pub fn progress_ring(ui: &mut egui::Ui, frac: f32, time: f64) {
    let size = 250.0;
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter();
    let c = r.center();
    let outer = size / 2.0;
    p.circle_filled(c, outer - 12.0, soft());
    let ticks = 90;
    for i in 0..ticks {
        let a = i as f32 / ticks as f32 * TAU - TAU / 4.0;
        let done = (i as f32 / ticks as f32) < frac;
        let col = if done { ring() } else { faint() };
        let d = Vec2::angled(a);
        p.line_segment([c + d * (outer - 8.0), c + d * outer], Stroke::new(1.4, col));
    }
    if frac > 0.0 {
        let n = (frac * 96.0).ceil() as usize;
        let arc: Vec<Pos2> = (0..=n).map(|i| c + Vec2::angled(i as f32 / 96.0 * TAU - TAU / 4.0) * (outer - 14.0)).collect();
        p.add(egui::Shape::line(arc, Stroke::new(3.0, ring())));
    }
    for k in 0..3 {
        let rot = time as f32 * 0.8 + k as f32 * TAU / 3.0;
        let pts: Vec<Pos2> = (0..48)
            .map(|i| {
                let t = i as f32 / 48.0 * TAU;
                let v = vec2(62.0 * t.cos(), 26.0 * t.sin());
                c + vec2(v.x * rot.cos() - v.y * rot.sin(), v.x * rot.sin() + v.y * rot.cos())
            })
            .collect();
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.0, ring().gamma_multiply(0.7))));
    }
    p.text(c, egui::Align2::CENTER_CENTER, format!("{:.0}%", frac * 100.0), egui::FontId::new(36.0, bold_family()), ink());
}

/// The big green check of the done screen.
pub fn done_check(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(130.0), egui::Sense::hover());
    let c = r.center();
    ui.painter().circle_stroke(c, 62.0, Stroke::new(1.6, ring()));
    let pts = vec![c + vec2(-26.0, 2.0), c + vec2(-8.0, 20.0), c + vec2(28.0, -18.0)];
    ui.painter().add(egui::Shape::line(pts, Stroke::new(5.0, ring())));
}

/// A circled "?" that explains the screen when hovered (top-right of results screens).
pub fn help(ui: &mut egui::Ui, text: &str) {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::hover());
    ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(1.2, dim()));
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "?", egui::FontId::new(14.0, bold_family()), dim());
    resp.on_hover_text(text);
}

/// The orange circled "!" before a results heading.
pub fn alert_badge(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
    ui.painter().circle_stroke(r.center(), 15.0, Stroke::new(2.0, size_color()));
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "!", egui::FontId::new(20.0, bold_family()), size_color());
}

/// "Name with a long middle…end" so both ends of a file name stay readable.
pub fn trunc_mid(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let keep = (max - 1) / 2;
    let head: String = chars[..keep].iter().collect();
    let tail: String = chars[chars.len() - keep..].iter().collect();
    format!("{head}…{tail}")
}

pub fn trunc_end(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max - 1).collect::<String>())
}

/// "25 Nov 2026 at 2:32 PM", in local time.
pub fn date_time(t: std::time::SystemTime) -> String {
    let d: chrono::DateTime<chrono::Local> = t.into();
    d.format("%-d %b %Y at %-I:%M %p").to_string()
}

pub fn date_only(t: std::time::SystemTime) -> String {
    let d: chrono::DateTime<chrono::Local> = t.into();
    d.format("%-d %b %Y").to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_icon_has_a_transparent_corner_and_a_cream_broom() {
        let px = super::app_icon(64);
        assert_eq!(px.len(), 64 * 64 * 4);
        assert_eq!(px[3], 0, "corner is transparent");
        let at = |x: usize, y: usize| &px[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert!(at(32, 12)[0] > at(32, 12)[2] + 40 && at(32, 12)[3] == 255, "clay tile near the top");
        assert!(at(38, 25)[..3].iter().all(|&c| c > 220), "cream broom handle");
    }

    #[test]
    fn truncation_keeps_both_ends() {
        assert_eq!(super::trunc_mid("drive-download-20260418T155753Z-3-001", 21), "drive-down…753Z-3-001");
        assert_eq!(super::trunc_mid("short", 21), "short");
        assert_eq!(super::trunc_end("/Users/me/Library/Application Support", 12), "/Users/me/L…");
    }
}

/// A checkbox: solid accent with a white tick (or dash when mixed), grey outline when
/// empty. Returns true when clicked.
pub fn check(ui: &mut egui::Ui, on: bool, mixed: bool, enabled: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let b = Rect::from_center_size(r.center(), Vec2::splat(17.0));
    let p = ui.painter();
    if !enabled {
        p.rect_filled(b, 3.0, disabled().gamma_multiply(0.7));
    } else if on || mixed {
        p.rect_filled(b, 3.0, accent());
        let st = Stroke::new(2.0, Color32::WHITE);
        if mixed {
            p.line_segment([b.left_center() + vec2(4.0, 0.0), b.right_center() - vec2(4.0, 0.0)], st);
        } else {
            let pts = vec![b.min + vec2(4.0, 8.5), b.min + vec2(7.3, 12.0), b.min + vec2(13.2, 5.0)];
            p.add(egui::Shape::line(pts, st));
        }
    } else {
        p.rect_filled(b, 3.0, paper());
        p.rect_stroke(b, 3.0, Stroke::new(1.2, dim()), egui::StrokeKind::Inside);
    }
    resp.clicked()
}

/// An On/Off switch. Returns true when flipped.
pub fn switch(ui: &mut egui::Ui, on: bool, enabled: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 22.0), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let t = ui.ctx().animate_bool(resp.id, on);
    let fill = if !enabled { disabled() } else { faint().lerp_to_gamma(accent(), t) };
    ui.painter().rect_filled(r, 11.0, fill);
    let x = egui::lerp((r.left() + 11.0)..=(r.right() - 11.0), t);
    ui.painter().circle_filled(egui::pos2(x, r.center().y), 8.5, Color32::WHITE);
    resp.clicked()
}

/// A small live graph of the last values (0..=1), newest on the right.
pub fn sparkline(ui: &mut egui::Ui, values: &[f32], color: Color32, height: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::hover());
    ui.painter().rect_filled(r, 6.0, soft().gamma_multiply(0.6));
    if values.len() < 2 {
        return;
    }
    let n = 60.max(values.len()) as f32;
    let pts: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| pos2(r.right() - (values.len() - 1 - i) as f32 / (n - 1.0) * r.width(), r.bottom() - v.clamp(0.0, 1.0) * (r.height() - 4.0) - 2.0))
        .collect();
    let mut fill = pts.clone();
    fill.push(pos2(pts.last().unwrap().x, r.bottom()));
    fill.push(pos2(pts[0].x, r.bottom()));
    let mut mesh = egui::epaint::Mesh::default();
    for (i, w) in pts.windows(2).enumerate() {
        let base = (i * 4) as u32;
        for q in [w[0], w[1], pos2(w[1].x, r.bottom()), pos2(w[0].x, r.bottom())] {
            mesh.colored_vertex(q, color.gamma_multiply(0.18));
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    ui.painter().add(mesh);
    ui.painter().add(egui::Shape::line(pts, Stroke::new(1.8, color)));
}

/// Two smooth filled series over labelled columns.
/// Each series is scaled to its own maximum.
pub fn area_chart(ui: &mut egui::Ui, labels: &[String], series: [(&[f64], Color32); 2], height: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::hover());
    let plot = Rect::from_min_max(r.min, pos2(r.max.x, r.max.y - 26.0));
    let n = labels.len().max(2);
    let col_w = plot.width() / n as f32;
    for (i, l) in labels.iter().enumerate() {
        let x = plot.left() + col_w * i as f32;
        ui.painter().vline(x, plot.y_range(), Stroke::new(1.0, Color32::from_black_alpha(18)));
        ui.painter().text(pos2(x + col_w / 2.0, r.bottom() - 10.0), egui::Align2::CENTER_CENTER, l, egui::FontId::proportional(14.0), dim());
    }
    ui.painter().hline(plot.x_range(), plot.bottom(), Stroke::new(1.0, Color32::from_black_alpha(30)));
    for (vals, color) in series {
        let max = vals.iter().cloned().fold(0.0, f64::max).max(1e-9);
        // Sample a smooth curve through the column centres (cosine interpolation).
        let at = |i: usize| pos2(plot.left() + col_w * (i as f32 + 0.5), plot.bottom() - (vals.get(i).copied().unwrap_or(0.0) / max) as f32 * (plot.height() - 8.0));
        let mut pts = vec![pos2(plot.left(), plot.bottom())];
        for i in 0..vals.len().saturating_sub(1) {
            let (a, b) = (at(i), at(i + 1));
            for k in 0..16 {
                let t = k as f32 / 16.0;
                let e = (1.0 - (t * std::f32::consts::PI).cos()) / 2.0;
                pts.push(pos2(egui::lerp(a.x..=b.x, t), egui::lerp(a.y..=b.y, e)));
            }
        }
        if let Some(i) = vals.len().checked_sub(1) {
            pts.push(at(i));
        }
        pts.push(pos2(plot.right(), plot.bottom()));
        let mut mesh = egui::epaint::Mesh::default();
        for w in pts.windows(2) {
            let base = mesh.vertices.len() as u32;
            for q in [w[0], w[1], pos2(w[1].x, plot.bottom()), pos2(w[0].x, plot.bottom())] {
                mesh.colored_vertex(q, color.gamma_multiply(0.16));
            }
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
        ui.painter().add(mesh);
        ui.painter().add(egui::Shape::line(pts, Stroke::new(1.6, color)));
    }
}

/// A coloured circle with the first letter of a name, in place of an app's or browser's logo.
pub fn badge(ui: &mut egui::Ui, name: &str, size: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let hue = name.bytes().fold(0u32, |a, x| a.wrapping_mul(31).wrapping_add(x as u32));
    let col = egui::ecolor::Hsva::new((hue % 360) as f32 / 360.0, 0.45, 0.75, 1.0);
    ui.painter().circle_filled(r.center(), size / 2.0 - 1.0, col);
    let letter: String = name.chars().find(|c| c.is_alphanumeric()).unwrap_or('?').to_uppercase().collect();
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, letter, egui::FontId::new(size * 0.47, bold_family()), Color32::WHITE);
}

/// Claybroom's app icon: a cream broom on a clay tile (source: `assets/icon.svg`, rendered to
/// `assets/icon-1024.png`), as straight RGBA pixels of `size`×`size`.
/// Used for the window, the macOS .icns, Windows and the tray.
pub fn app_icon(size: u32) -> Vec<u8> {
    let master = image::load_from_memory_with_format(include_bytes!("../assets/icon-1024.png"), image::ImageFormat::Png)
        .expect("bundled icon")
        .into_rgba8();
    image::imageops::resize(&master, size, size, image::imageops::FilterType::Lanczos3).into_raw()
}

/// A settings checkbox: the filled accent box and its label, both clickable.
pub fn tick(ui: &mut egui::Ui, v: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.horizontal(|ui| {
        let boxed = check(ui, *v, false, true);
        let r = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
        if boxed || r.clicked() {
            *v = !*v;
        }
        r
    })
    .inner
}
