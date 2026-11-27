//! The menu-bar panel: computer name, a carousel
//! of recommendations with SCAN, CPU/memory, sensors, disk usage, battery and an OPEN button.

use crate::clean::fmt_size;
use crate::kit::{self, b, dim, ink, line, muted, size_color, Icon};
use crate::{Page, Sweep};
use eframe::egui::{self, vec2, Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2};

/// Panel size in points.
pub const PANEL: [f32; 2] = [368.0, 900.0];

const RECS: [(Icon, &str, &str, Page); 4] = [
    (Icon::Broom, "Clean clutter", "Caches, logs and old downloads.", Page::Clutter),
    (Icon::Globe, "Clean browser", "Delete what your browsers keep about you.", Page::Browsers),
    (Icon::Duplicates, "Find duplicates", "Free up the space copies take.", Page::Duplicates),
    (Icon::Picture, "Analyze photos", "Find blurry and look-alike photos.", Page::Photos),
];

/// A section header: icon, capitals title, and a › that opens the System monitor.
fn section(ui: &mut egui::Ui, ic: Icon, title: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let ir = egui::Rect::from_min_size(r.left_center() + vec2(0.0, -12.0), Vec2::splat(24.0));
    kit::icon(ui.painter(), ir, ic, ink());
    ui.painter().text(r.left_center() + vec2(36.0, 0.0), egui::Align2::LEFT_CENTER, title, egui::FontId::new(15.5, kit::bold_family()), ink());
    ui.painter().text(r.right_center() - vec2(6.0, 0.0), egui::Align2::RIGHT_CENTER, "›", egui::FontId::proportional(22.0), dim());
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn row(ui: &mut egui::Ui, label: &str, value: String, warn: bool) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_space(36.0);
        ui.label(RichText::new(label).size(15.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(6.0);
            ui.label(b(value, 15.0).color(if warn { size_color() } else { ink() }));
        });
    })
    .response
}

fn divider(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let r = ui.cursor();
    ui.painter().hline((r.left() - 20.0)..=(r.right() + 20.0), r.top(), Stroke::new(1.0, line()));
    ui.add_space(6.0);
}

impl Sweep {
    pub fn tray_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let now = ui.input(|i| i.time);
        self.refresh_stats(now);
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        let mut go: Option<(Page, bool)> = None;
        let white = kit::bg();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(white)).show(ui, |ui| {
            // Header: computer name, "System monitor", settings.
            egui::Frame::new().inner_margin(Margin::symmetric(22, 14)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new(&self.host).size(14.0));
                        ui.label(muted("System monitor"));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
                        kit::icon(ui.painter(), r, Icon::Gear, ink());
                        if resp.on_hover_text("Settings").clicked() {
                            go = Some((Page::Settings, false));
                        }
                    });
                });
            });
            let r = ui.cursor();
            ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, line()));
            // The OPEN button stays at the bottom.
            egui::Panel::bottom("panel_open").frame(egui::Frame::new().fill(kit::rail()).inner_margin(Margin::symmetric(20, 16))).show(ui, |ui| {
                let btn = egui::Button::new(RichText::new("OPEN CLAYBROOM").family(kit::bold_family()).size(16.0).color(Color32::WHITE))
                    .fill(kit::accent())
                    .corner_radius(CornerRadius::same(kit::radius()))
                    .min_size(vec2(ui.available_width(), 46.0));
                if ui.add(btn).clicked() {
                    go = Some((Page::Home, false));
                }
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Frame::new().inner_margin(Margin { left: 22, right: 22, top: 10, bottom: 10 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // Recommendations carousel: one card, the next one peeking in.
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                        kit::icon(ui.painter(), r, Icon::Check, ink());
                        ui.add_space(6.0);
                        ui.label(b("RECOMMENDATIONS", 15.5));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            for (t, step) in [("›", 1usize), ("‹", RECS.len() - 1)] {
                                let btn = egui::Button::new(RichText::new(t).size(18.0)).fill(kit::rail_active().gamma_multiply(0.6)).min_size(Vec2::splat(26.0));
                                if ui.add(btn).clicked() {
                                    self.rec_idx = (self.rec_idx + step) % RECS.len();
                                }
                            }
                        });
                    });
                    ui.add_space(8.0);
                    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 130.0), Sense::hover());
                    let card_w = area.width() * 0.8;
                    let clip = ui.painter().clip_rect();
                    for k in 0..2 {
                        let (ic, title, text, page) = RECS[(self.rec_idx + k) % RECS.len()];
                        let r = egui::Rect::from_min_size(area.min + vec2(k as f32 * (card_w + 14.0), 0.0), vec2(card_w, area.height()));
                        let mut card = ui.new_child(egui::UiBuilder::new().max_rect(r));
                        card.set_clip_rect(clip.intersect(egui::Rect::from_min_max(area.min, egui::pos2(clip.right(), area.bottom()))));
                        card.painter().rect_filled(r, 22.0, kit::tile_fill());
                        egui::Frame::new().inner_margin(Margin::symmetric(16, 14)).show(&mut card, |ui| {
                            ui.horizontal(|ui| {
                                let (ir, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                                kit::icon(ui.painter(), ir, ic, kit::accent());
                                ui.add_space(4.0);
                                ui.label(b(title, 16.0));
                            });
                            ui.horizontal(|ui| {
                                ui.add_space(36.0);
                                ui.label(RichText::new(text).size(14.0));
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.add_space(36.0);
                                if kit::outline(ui, "SCAN") {
                                    go = Some((page, true));
                                }
                            });
                        });
                    }
                    ui.add_space(10.0);

                    if section(ui, Icon::Monitor, "CPU/MEMORY") {
                        go = Some((Page::Monitor, true));
                    }
                    let cpu = self.sys.global_cpu_usage();
                    let mem = self.sys.used_memory() as f32 / self.sys.total_memory().max(1) as f32 * 100.0;
                    let _ = row(ui, "CPU Usage", format!("{cpu:.0}%"), cpu > 70.0);
                    let _ = row(ui, "Memory", format!("{mem:.0}%"), mem > 70.0);
                    if let Some(g) = self.gpu {
                        let _ = row(ui, "GPU Usage", format!("{g:.0}%"), g > 85.0);
                    }
                    divider(ui);

                    if section(ui, Icon::Thermo, "SENSORS") {
                        go = Some((Page::Monitor, true));
                    }
                    let hottest = self.components.iter().flat_map(|c| c.iter()).filter_map(|c| c.temperature()).filter(|t| *t > 0.0).fold(None, |m: Option<f32>, t| Some(m.map_or(t, |m| m.max(t))));
                    match hottest {
                        Some(t) => _ = row(ui, "Hottest sensor", format!("{t:.0} °C"), t > 85.0),
                        None => _ = row(ui, "Temperatures", "not shared".into(), false),
                    }
                    // Fan speed: a percentage of top speed (the average of all fans).
                    let pcts: Vec<f32> = self.fans.iter().filter_map(|f| f.percent()).collect();
                    if !pcts.is_empty() {
                        let avg = pcts.iter().sum::<f32>() / pcts.len() as f32;
                        let _ = row(ui, "Fan Speed", format!("{avg:.0}%"), avg > 85.0);
                    } else if let Some(f) = self.fans.first() {
                        let _ = row(ui, "Fan Speed", format!("{:.0} rpm", f.rpm), false);
                    }
                    divider(ui);

                    if section(ui, Icon::Folder, "DISK USAGE") {
                        go = Some((Page::Monitor, true));
                    }
                    let mut disks: Vec<_> = self.disks.iter().filter(|d| d.total_space() > 0).collect();
                    disks.sort_by_key(|d| std::cmp::Reverse(d.total_space()));
                    let mut seen = std::collections::HashSet::new();
                    for d in disks.iter() {
                        let used = (d.total_space() - d.available_space()) as f32 / d.total_space() as f32 * 100.0;
                        let name = d.name().to_string_lossy().into_owned();
                        let name = if name.is_empty() || name.starts_with("/dev") { d.mount_point().display().to_string() } else { name };
                        // macOS lists its system and data volumes under one name; show it once.
                        if !seen.insert(name.clone()) || seen.len() > 4 {
                            continue;
                        }
                        row(ui, &kit::trunc_end(&name, 24), format!("{used:.0}%"), used > 90.0)
                            .on_hover_text(format!("{} free · {}", fmt_size(d.available_space()), d.mount_point().display()));
                    }
                    divider(ui);

                    if section(ui, Icon::Battery, "BATTERY") {
                        go = Some((Page::Monitor, true));
                    }
                    match &self.battery {
                        Some(bt) => _ = row(ui, "Battery", format!("{:.0}%", bt.percent), bt.percent < 20.0),
                        None => _ = row(ui, "Battery", "none".into(), false),
                    }
                });
            });
        });
        if let Some((page, start)) = go {
            self.open_main(&ctx, page);
            if start {
                match page {
                    Page::Clutter => self.start_clutter(&ctx),
                    Page::Browsers => self.start_browsers(&ctx),
                    Page::Monitor => self.monitor_on = true,
                    _ => {}
                }
            }
        }
    }
}
