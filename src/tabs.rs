//! Analyze photos, Uninstall apps, Startup manager, System monitor, Statistics and Settings,
//! styled.

use crate::clean::{self, fmt_size};
use crate::kit::{self, b, muted, tile, Icon, bad, line, dim, accent, size_color, rail_active, ink};
use crate::screens::{checkbox, mixed, reveal, results_heading, selected_parts};
use crate::{apps, intro, now_secs, AppSort, AppTab, Confirm, Entry, StartSort, Sweep};
use eframe::egui::{self, vec2, Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2};
use std::time::{Duration, SystemTime};

/// A battery reading for the System monitor.
#[derive(Clone)]
pub struct BatteryInfo {
    pub percent: f32,
    pub health: f32,
    pub cycles: Option<u32>,
    pub state: String,
    pub temp: Option<f32>,
}

pub fn read_battery() -> Option<BatteryInfo> {
    let m = starship_battery::Manager::new().ok()?;
    let b = m.batteries().ok()?.flatten().next()?;
    Some(BatteryInfo {
        percent: b.state_of_charge().value * 100.0,
        health: b.state_of_health().value * 100.0,
        cycles: b.cycle_count(),
        state: format!("{:?}", b.state()),
        temp: b.temperature().map(|t| t.value - 273.15),
    })
}

/// Two side-by-side columns with a vertical line between.
fn split<L: FnOnce(&mut egui::Ui), R: FnOnce(&mut egui::Ui)>(ui: &mut egui::Ui, left_w: f32, left: L, right: R) {
    let h = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(left_w, h), egui::Layout::top_down(egui::Align::Min), left);
        let r = ui.cursor();
        ui.painter().vline(r.left(), r.top()..=r.top() + h, Stroke::new(1.0, line()));
        ui.add_space(14.0);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), h), egui::Layout::top_down(egui::Align::Min), right);
    });
}

/// A row of the left category list: title, subtitle, blue when chosen. Returns true when clicked.
fn side_row(ui: &mut egui::Ui, w: f32, title: &str, sub: &str, active: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 62.0), Sense::click());
    if active {
        ui.painter().rect_filled(rect, 0.0, accent());
        let tip = rect.right_center();
        ui.painter().add(egui::Shape::convex_polygon(vec![tip + vec2(-1.0, -8.0), tip + vec2(8.0, 0.0), tip + vec2(-1.0, 8.0)], accent(), Stroke::NONE));
    } else {
        ui.painter().hline((rect.left() + 14.0)..=rect.right(), rect.bottom(), Stroke::new(1.0, line()));
        ui.painter().text(rect.right_center() - vec2(12.0, 0.0), egui::Align2::CENTER_CENTER, "›", egui::FontId::proportional(20.0), dim());
    }
    let fg = if active { Color32::WHITE } else { ink() };
    ui.painter().text(rect.left_center() + vec2(16.0, -9.0), egui::Align2::LEFT_CENTER, title, egui::FontId::new(15.0, kit::bold_family()), fg);
    let col = if active { Color32::from_white_alpha(210) } else { dim() };
    ui.painter().text(rect.left_center() + vec2(16.0, 11.0), egui::Align2::LEFT_CENTER, sub, egui::FontId::proportional(13.0), col);
    resp.clicked()
}

fn info_line(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.label(RichText::new(label).size(12.5).color(dim()));
    ui.label(RichText::new(value.into()).size(13.5));
    ui.add_space(4.0);
}

pub const SETTINGS_TABS: [(Icon, &str); 7] = [
    (Icon::Monitor, "General"),
    (Icon::Quick, "Scanning"),
    (Icon::Globe, "Browser Cleaning"),
    (Icon::Trash, "Trash"),
    (Icon::Check, "Alerts"),
    (Icon::Stats, "About"),
    (Icon::Shield, "Privacy"),
];

impl Sweep {
    // ---------- Analyze photos ----------

    pub fn start_photos(&mut self, ctx: &egui::Context) {
        clean::reset(&self.progress);
        self.done = None;
        self.photo_tex.clear();
        let p = self.progress.clone();
        let roots = crate::photos::roots(&self.saved.settings);
        self.photos_job = Some(crate::spawn(ctx, move || crate::photos::scan(&roots, &p)));
        self.page = crate::Page::Photos;
    }

    pub fn photos_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.clean_job.is_some() {
            self.scanning(ui, "Cleaning...", false);
            return;
        }
        if self.photos_job.is_some() {
            if self.scanning(ui, "Analyzing your photos...", true) {
                self.photos_job = None;
            }
            return;
        }
        let Some(res) = &self.photos else {
            let rows = [
                (Icon::Picture, "Weak shots", "Blurry, badly lit and poor-contrast shots."),
                (Icon::Duplicates, "Look-alikes", "Bursts and several shots of the same moment."),
                (Icon::Check, "Your pick", "You choose which ones to delete; the sharpest is kept."),
                (Icon::Folder, "Where it looks", if cfg!(target_os = "macos") { "Pictures, Desktop, Documents, Downloads and your Photos library." } else { "Pictures, Desktop, Documents and Downloads." }),
            ];
            if intro(ui, "Photo analyzer", "Blurry, dark and near-identical photos, ready to review.", rows, "FIND PHOTOS", true, Color32::from_rgb(0xd2, 0x7a, 0x2c)) {
                self.start_photos(&ctx);
            }
            return;
        };
        if res.all().next().is_none() {
            self.photos = None;
            return self.nothing_to_clean();
        }
        let total: u64 = res.all().map(|p| p.size).sum();
        let sel: Vec<&crate::photos::Photo> = res.all().filter(|p| p.selected).collect();
        let (n, size) = (sel.len(), sel.iter().map(|p| p.size).sum::<u64>());
        let groups = res.similar.len();
        let bad = res.bad.len();
        let tsize = fmt_size(total);
        results_heading(
            ui,
            &[("We've found ", ink()), (&tsize, size_color()), (" of photos to review", ink())],
            true,
            "Similar photos look alike (bursts, several shots of the same thing): the sharpest of each group stays unticked. Bad photos are blurry, too dark, too bright or flat. Ticked photos go to the Trash.",
        );
        let summary = if n == 0 { selected_parts(0, 0) } else { vec![(format!("{n} photos selected ({})", fmt_size(size)), ink())] };
        let mut pick_none = false;
        let clean = self.bottom_bar(ui, "photos_bar", |ui| pick_none = kit::outline(ui, "SELECT NONE"), summary, "CLEAN", n > 0);
        if pick_none {
            self.photos.as_mut().unwrap().all_mut().for_each(|p| p.selected = false);
        }
        if clean {
            self.confirm = Some(Confirm::Photos);
        }
        let view = self.photo_view;
        let mut new_view = view;
        split(
            ui,
            220.0,
            |ui| {
                if side_row(ui, 220.0, "Similar photos", &format!("{groups} groups"), view == 0) {
                    new_view = 0;
                }
                if side_row(ui, 220.0, "Bad photos", &format!("{bad} photos"), view == 1) {
                    new_view = 1;
                }
            },
            |ui| {
                let tex = &mut self.photo_tex;
                let res = self.photos.as_mut().unwrap();
                egui::ScrollArea::vertical().id_salt("photos").show(ui, |ui| {
                    let mut thumb = |ui: &mut egui::Ui, ph: &mut crate::photos::Photo, caption: &str| {
                        // Off screen: keep the space, skip the picture (thousands of photos stay smooth).
                        let cell = egui::Rect::from_min_size(ui.cursor().min, vec2(130.0, 150.0));
                        if !ui.is_rect_visible(cell) {
                            ui.allocate_exact_size(cell.size(), Sense::hover());
                            return;
                        }
                        ui.vertical(|ui| {
                            ui.set_width(130.0);
                            ui.set_height(150.0);
                            let t = tex.entry(ph.path.clone()).or_insert_with(|| {
                                let img = egui::ColorImage::from_rgba_unmultiplied([ph.thumb.w, ph.thumb.h], &ph.thumb.rgba);
                                ui.ctx().load_texture(ph.path.display().to_string(), img, Default::default())
                            });
                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(126.0), Sense::click());
                            let sz = t.size_vec2();
                            let scale = (126.0 / sz.x).min(126.0 / sz.y);
                            let img_r = egui::Rect::from_center_size(r.center(), sz * scale);
                            ui.painter().rect_filled(r, 8.0, kit::soft().gamma_multiply(0.7));
                            ui.painter().image(t.id(), img_r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                            if ph.selected {
                                ui.painter().rect_stroke(r, 8.0, Stroke::new(2.5, accent()), egui::StrokeKind::Inside);
                            }
                            let cb = egui::Rect::from_min_size(r.min + vec2(6.0, 6.0), Vec2::splat(20.0));
                            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(cb));
                            checkbox(&mut child, &mut ph.selected);
                            if resp.clicked() {
                                ph.selected = !ph.selected;
                            }
                            resp.on_hover_text(ph.path.display().to_string());
                            ui.label(RichText::new(caption).size(12.5).color(dim()));
                        });
                    };
                    let cols = ((ui.available_width() + 10.0) / 140.0).floor().max(1.0) as usize;
                    if view == 0 {
                        for g in res.similar.iter_mut() {
                            let size: u64 = g.iter().map(|p| p.size).sum();
                            ui.label(b(format!("{} similar photos · {}", g.len(), fmt_size(size)), 15.0));
                            for (r, row) in g.chunks_mut(cols).enumerate() {
                                ui.horizontal(|ui| {
                                    for (i, ph) in row.iter_mut().enumerate() {
                                        let cap = if r == 0 && i == 0 { "Sharpest".to_string() } else { fmt_size(ph.size) };
                                        let cap = if ph.library_id.is_some() { format!("{cap} · Photos") } else { cap };
                                        thumb(ui, ph, &cap);
                                    }
                                });
                            }
                            ui.add_space(8.0);
                        }
                    } else {
                        for row in res.bad.chunks_mut(cols) {
                            ui.horizontal(|ui| {
                                for ph in row.iter_mut() {
                                    let cap = if ph.library_id.is_some() { format!("{} · Photos", ph.reason) } else { ph.reason.to_string() };
                                    let cap = cap.as_str();
                                    thumb(ui, ph, cap);
                                }
                            });
                        }
                    }
                });
            },
        );
        self.photo_view = new_view;
    }

    // ---------- Uninstall apps ----------

    pub fn apps_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.clean_job.is_some() {
            self.scanning(ui, "Uninstalling apps...", false);
            return;
        }
        if self.apps_job.is_some() {
            if self.scanning(ui, "Finding your apps...", true) {
                self.apps_job = None;
            }
            return;
        }
        let Some(list) = self.apps.as_mut() else {
            let rows = [
                (Icon::Clutter, "Nothing left behind", "Apps go with their caches, preferences and support files."),
                (Icon::Check, "Last opened", "See when you last opened each app."),
                (Icon::Trash, "Biggest first", "Find the apps taking the most space."),
                (Icon::Folder, "Room for what you use", "Make room for what you actually use."),
            ];
            if intro(ui, "App uninstaller", "Apps you've stopped opening, and their leftovers.", rows, "SHOW APPS", true, Color32::from_rgb(0xe8, 0x6a, 0x3a)) {
                self.start_apps(&ctx);
            }
            return;
        };
        let s = &self.saved.settings;
        let large = clean::LARGE_APP_SIZES[s.large_app_idx.min(5)];
        let (unused_days, unused_label) = clean::UNUSED_DAYS[s.unused_idx.min(3)];
        let now = SystemTime::now();
        let is_unused = move |a: &apps::App| a.last_used.is_some_and(|t| now.duration_since(t).is_ok_and(|d| d.as_secs() > unused_days * 86400));
        match self.app_sort {
            AppSort::Name => list.sort_by_key(|a| a.name.to_lowercase()),
            AppSort::Size => list.sort_by_key(|a| std::cmp::Reverse(a.total())),
            AppSort::LastUsed => list.sort_by_key(|a| a.last_used),
        }
        let count = format!("{} app(s)", list.len());
        let pending = list.iter().filter(|a| !a.measured).count();
        results_heading(
            ui,
            &[("You have ", ink()), (&count, size_color())],
            false,
            "The uninstaller removes the app and the files it created: caches, preferences, support files, logs and saved state. On Windows it runs the app's own uninstaller.",
        );
        if pending > 0 {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(muted(format!("Measuring app sizes: {} of {} done", list.len() - pending, list.len())));
            });
        }
        let sel: Vec<&apps::App> = list.iter().filter(|a| a.selected).collect();
        let (n, size) = (sel.len(), sel.iter().map(|a| a.total()).sum::<u64>());
        let summary = if n == 0 { selected_parts(0, 0) } else { vec![(fmt_size(size), size_color()), (format!(" selected · {n} app(s)"), ink())] };
        if self.bottom_bar(ui, "apps_bar", |_| {}, summary, "UNINSTALL", n > 0) {
            self.confirm = Some(Confirm::Apps);
        }
        let list = self.apps.as_ref().unwrap();
        let (n_all, n_large, n_unused) = (list.len(), list.iter().filter(|a| a.total() >= large).count(), list.iter().filter(|a| is_unused(a)).count());
        let mut tab = self.app_tab;
        let mut sort = self.app_sort;
        let mut show = self.app_sel.clone();
        let tab_now = self.app_tab;
        let apps = self.apps.as_mut().unwrap();
        split(
            ui,
            210.0,
            |ui| {
                for (t, title, sub) in [
                    (AppTab::All, "All apps".to_string(), format!("{n_all} app(s)")),
                    (AppTab::Large, "Large apps".to_string(), format!("{} or more · {n_large}", fmt_size(large))),
                    (AppTab::Unused, "Unused apps".to_string(), format!("{unused_label} or more · {n_unused}")),
                ] {
                    if side_row(ui, 210.0, &title, &sub, tab == t) {
                        tab = t;
                    }
                }
            },
            |ui| {
                let right_w = 170.0;
                ui.horizontal_top(|ui| {
                    let h = ui.available_height();
                    ui.allocate_ui_with_layout(vec2(ui.available_width() - right_w - 12.0, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("app_sort").selected_text("Sort by").width(110.0).show_ui(ui, |ui| {
                                ui.selectable_value(&mut sort, AppSort::Name, "Name");
                                ui.selectable_value(&mut sort, AppSort::Size, "Size");
                                ui.selectable_value(&mut sort, AppSort::LastUsed, "Last used");
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_sized([96.0, 18.0], egui::Label::new(muted("Last used")));
                                ui.add_sized([76.0, 18.0], egui::Label::new(muted("Size")));
                            });
                        });
                        let r = ui.cursor();
                        ui.painter().hline(r.x_range(), r.top() + 2.0, Stroke::new(1.0, line()));
                        egui::ScrollArea::vertical().id_salt("apps_list").show(ui, |ui| {
                            for a in apps.iter_mut().filter(|a| match tab_now {
                                AppTab::All => true,
                                AppTab::Large => a.total() >= large,
                                AppTab::Unused => is_unused(a),
                            }) {
                                ui.horizontal(|ui| {
                                    ui.set_min_height(42.0);
                                    if a.can_uninstall() {
                                        checkbox(ui, &mut a.selected);
                                    } else {
                                        kit::check(ui, false, false, false);
                                    }
                                    kit::badge(ui, &a.name, 28.0);
                                    let r = ui.add(egui::Label::new(RichText::new(kit::trunc_end(&a.name, 30)).size(15.0)).sense(Sense::click()));
                                    if r.clicked() {
                                        show = Some(a.name.clone());
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if !a.measured {
                                            ui.add_space(96.0 + ui.spacing().item_spacing.x);
                                            ui.allocate_ui(vec2(76.0, 18.0), |ui| ui.add(egui::Spinner::new().size(14.0)));
                                            return;
                                        }
                                        ui.add_sized([96.0, 18.0], egui::Label::new(RichText::new(a.last_used.map(kit::date_only).unwrap_or("—".into())).size(13.0)));
                                        ui.add_sized([76.0, 18.0], egui::Label::new(b(a.size.map(|_| fmt_size(a.total())).unwrap_or("—".into()), 14.0)));
                                    });
                                });
                            }
                        });
                    });
                    ui.allocate_ui_with_layout(vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.label(muted("App files"));
                        ui.add_space(6.0);
                        let Some(a) = show.as_ref().and_then(|n| apps.iter().find(|a| &a.name == n)) else {
                            ui.label(RichText::new("Click an app to see the files it keeps.").size(13.0).color(dim()));
                            return;
                        };
                        kit::badge(ui, &a.name, 56.0);
                        ui.label(b(kit::trunc_end(&a.name, 22), 15.0));
                        if !a.measured {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().size(13.0));
                                ui.label(RichText::new("Measuring...").size(13.0).color(dim()));
                            });
                            return;
                        }
                        ui.label(RichText::new(format!("Size {}", fmt_size(a.total()))).size(13.0).color(dim()));
                        ui.separator();
                        let mut cats: Vec<(String, u64)> = vec![("Binaries".into(), a.size.unwrap_or(0))];
                        for (c, _, s) in &a.leftovers {
                            match cats.iter_mut().find(|x| &x.0 == c) {
                                Some(x) => x.1 += s,
                                None => cats.push((c.clone(), *s)),
                            }
                        }
                        for (c, s) in cats {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(c).size(13.0));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(b(fmt_size(s), 13.0)));
                            });
                        }
                        if let Some(p) = &a.path {
                            ui.add_space(6.0);
                            let show = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in folder" };
                            if ui.link(RichText::new(show).color(accent())).clicked() {
                                reveal(p);
                            }
                        }
                    });
                });
            },
        );
        self.app_tab = tab;
        self.app_sort = sort;
        self.app_sel = show;
    }

    // ---------- Startup manager ----------

    pub fn startup_page(&mut self, ui: &mut egui::Ui) {
        if self.startup.is_none() {
            let rows = [
                (Icon::Speed, "Quicker log-in", "Fewer things launching when you log in."),
                (Icon::Startup, "Background helpers", "Turn off background agents and daemons you don't need."),
                (Icon::Check, "Know what's running", "See every app that opens by itself."),
                (Icon::Shield, "Reversible", "Items you turn off can be turned back on."),
            ];
            if intro(ui, "Startup manager", "See what starts with your computer, and switch it off.", rows, "SHOW STARTUPS", true, Color32::from_rgb(0x36, 0x54, 0x9f)) {
                self.startup = Some(apps::scan_startup());
            }
            return;
        }
        let n = self.startup.as_ref().unwrap().len();
        let count = format!("{n} startup item(s)");
        results_heading(
            ui,
            &[("You have ", ink()), (&count, size_color())],
            false,
            "Apps, launch agents and launch daemons can start by themselves when you log in. Turning one off stops it starting; you can turn it back on any time. System-wide items (with a lock) ask for an administrator first.",
        );
        let list = self.startup.as_mut().unwrap();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.start_search).hint_text("Search").desired_width(220.0));
            egui::ComboBox::from_id_salt("start_sort").selected_text("Sort by").width(110.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.start_sort, StartSort::Name, "Name");
                ui.selectable_value(&mut self.start_sort, StartSort::Status, "Status");
                ui.selectable_value(&mut self.start_sort, StartSort::Type, "Type");
            });
            if kit::outline(ui, "REFRESH") {
                *list = apps::scan_startup();
            }
        });
        match self.start_sort {
            StartSort::Name => list.sort_by_key(|i| i.name.to_lowercase()),
            StartSort::Status => list.sort_by_key(|i| !i.on),
            StartSort::Type => list.sort_by_key(|i| i.kind),
        }
        if list.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(b("Nothing starts on its own", 24.0));
                ui.label(muted("We can't find any startup items right now."));
            });
            return;
        }
        let q = self.start_search.to_lowercase();
        let mut err = None;
        let mut sel = self.start_sel;
        let mut remove = None;
        let list = self.startup.as_mut().unwrap();
        ui.add_space(6.0);
        let right_w = 200.0;
        ui.horizontal_top(|ui| {
            let h = ui.available_height();
            ui.allocate_ui_with_layout(vec2(ui.available_width() - right_w - 14.0, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(44.0);
                    ui.label(muted("Name"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_sized([90.0, 18.0], egui::Label::new(muted("Status")));
                        ui.add_sized([110.0, 18.0], egui::Label::new(muted("Type")));
                    });
                });
                let r = ui.cursor();
                ui.painter().hline(r.x_range(), r.top() + 2.0, Stroke::new(1.0, line()));
                egui::ScrollArea::vertical().id_salt("startup_list").show(ui, |ui| {
                    for (i, it) in list.iter_mut().enumerate() {
                        if !q.is_empty() && !it.name.to_lowercase().contains(&q) {
                            continue;
                        }
                        let row = ui.horizontal(|ui| {
                            ui.set_min_height(48.0);
                            kit::badge(ui, &it.name, 30.0);
                            ui.vertical(|ui| {
                                ui.add_space(6.0);
                                ui.label(b(kit::trunc_end(&it.name, 30), 14.5));
                                ui.label(RichText::new(kit::trunc_end(&it.location, 44)).size(12.0).color(dim()));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_space(8.0);
                                ui.label(RichText::new(if it.on { "On" } else { "Off" }).size(13.5));
                                // System-wide items ask for an administrator (macOS's own prompt).
                                if kit::switch(ui, it.on, !it.system || cfg!(target_os = "macos")) {
                                    if let Err(e) = apps::set_on(it, !it.on) {
                                        err = Some(e);
                                    }
                                }
                                if it.system {
                                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                                    kit::icon(ui.painter(), r, Icon::Lock, dim());
                                    resp.on_hover_text(if cfg!(target_os = "macos") { "System-wide: macOS asks for an administrator first" } else { "System-wide: needs administrator rights" });
                                }
                                ui.add_space(10.0);
                                ui.add_sized([110.0, 18.0], egui::Label::new(RichText::new(it.kind.label()).size(13.5).color(dim())));
                            });
                        });
                        let rr = row.response.rect;
                        if sel == Some(i) {
                            ui.painter().rect_filled(rr.expand2(vec2(4.0, 1.0)), 6.0, accent().gamma_multiply(0.08));
                        }
                        if ui.interact(rr, ui.scope_id().with(("start_row", i)), Sense::click()).clicked() {
                            sel = Some(i);
                        }
                        ui.painter().hline(rr.x_range(), rr.bottom() + 3.0, Stroke::new(1.0, line()));
                    }
                });
            });
            ui.allocate_ui_with_layout(vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.label(muted("Details"));
                ui.add_space(6.0);
                let Some(it) = sel.and_then(|i| list.get(i)) else {
                    ui.label(RichText::new("Click an item to see where it comes from.").size(13.0).color(dim()));
                    return;
                };
                kit::badge(ui, &it.name, 56.0);
                ui.label(b(kit::trunc_end(&it.name, 24), 15.0));
                ui.separator();
                info_line(ui, "Type", it.kind.label());
                let created = std::fs::metadata(&it.location).ok().and_then(|m| m.created().ok());
                info_line(ui, "Date installed", created.map(kit::date_only).unwrap_or("—".into()));
                info_line(ui, "File location", kit::trunc_mid(&it.location, 60));
                if std::path::Path::new(&it.location).exists() {
                    let show = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in folder" };
                    if ui.link(RichText::new(show).color(accent())).clicked() {
                        reveal(std::path::Path::new(&it.location));
                    }
                }
                ui.add_space(10.0);
                if it.system && !cfg!(target_os = "macos") {
                    ui.label(RichText::new("Needs administrator rights to change.").size(12.5).color(dim()));
                } else if kit::outline(ui, "REMOVE") {
                    remove = sel;
                }
            });
        });
        self.start_sel = sel;
        if let Some(i) = remove {
            self.confirm = Some(Confirm::RemoveStartup(i));
        }
        if let Some(e) = err {
            self.message = Some(e);
        }
    }

    // ---------- System monitor ----------

    pub fn monitor_page(&mut self, ui: &mut egui::Ui) {
        if !self.monitor_on {
            let rows = [
                (Icon::Monitor, "Processor and memory", "How busy your computer is, in real time."),
                (Icon::Thermo, "Sensors", "Temperatures your computer reports."),
                (Icon::Folder, "Disk usage", "Used and free space on every drive."),
                (Icon::Battery, "Battery", "Charge, health, charge cycles and temperature."),
            ];
            if intro(ui, "System monitor", "Your system's vital stats at a glance.", rows, "SCAN SYSTEM", true, Color32::from_rgb(0x4a, 0x7b, 0xd9)) {
                self.monitor_on = true;
            }
            return;
        }
        let now = ui.input(|i| i.time);
        self.refresh_stats(now);
        ui.ctx().request_repaint_after(Duration::from_secs(1));
        kit::heading(ui, "System monitor", "Your system's vital stats at a glance.");
        self.monitor_body(ui);
    }

    /// Read CPU, memory, disks, sensors (every second) and the battery (every 10 seconds).
    pub fn refresh_stats(&mut self, now: f64) {
        if now - self.last_refresh > 1.0 {
            self.sys.refresh_cpu_usage();
            self.sys.refresh_memory();
            self.disks.refresh(true);
            if let Some(c) = self.components.as_mut() {
                c.refresh(true);
            }
            let mem = self.sys.used_memory() as f32 / self.sys.total_memory().max(1) as f32;
            for (h, v) in [(&mut self.cpu_hist, self.sys.global_cpu_usage() / 100.0), (&mut self.mem_hist, mem)] {
                h.push(v);
                if h.len() > 60 {
                    h.remove(0);
                }
            }
            if now - self.last_battery > 10.0 {
                self.battery = read_battery();
                self.last_battery = now;
            }
            if now - self.last_sensors > 2.0 {
                self.gpu = crate::sensors::gpu_usage();
                self.fans = crate::sensors::fans();
                self.last_sensors = now;
            }
            self.last_refresh = now;
        }
    }

    fn monitor_body(&mut self, ui: &mut egui::Ui) {
        let cpu = self.sys.global_cpu_usage();
        let (used, total) = (self.sys.used_memory(), self.sys.total_memory());
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(2, |c| {
                tile(&mut c[0], |ui| {
                    ui.set_min_height(190.0);
                    head(ui, Icon::Monitor, "CPU / Memory");
                    ui.columns(2, |c| {
                        c[0].label(b(format!("{cpu:.0}%"), 30.0));
                        c[0].label(muted(format!("CPU · {} cores", self.sys.cpus().len())));
                        kit::sparkline(&mut c[0], &self.cpu_hist, accent(), 60.0);
                        c[1].label(b(fmt_size(used), 30.0));
                        c[1].label(muted(format!("memory used of {}", fmt_size(total))));
                        kit::sparkline(&mut c[1], &self.mem_hist, Color32::from_rgb(0x8e, 0x5b, 0xe8), 60.0);
                    });
                    if let Some(g) = self.gpu {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Graphics (GPU)").size(13.5));
                            ui.add(egui::ProgressBar::new(g / 100.0).fill(accent()).desired_height(8.0).desired_width(120.0).corner_radius(4.0));
                            ui.label(b(format!("{g:.0}%"), 14.0));
                        });
                    }
                });
                tile(&mut c[1], |ui| {
                    ui.set_min_height(190.0);
                    head(ui, Icon::Thermo, "Sensors");
                    let mut temps: Vec<(String, f32)> = self.components.iter().flat_map(|c| c.iter()).filter_map(|c| Some((c.label().to_string(), c.temperature()?))).filter(|t| t.1 > 0.0).collect();
                    temps.sort_by(|a, b| b.1.total_cmp(&a.1));
                    for (i, f) in self.fans.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(if self.fans.len() > 1 { format!("Fan {}", i + 1) } else { "Fan".into() }).size(13.5));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(b(format!("{:.0} rpm", f.rpm), 14.0));
                                if let Some(p) = f.percent() {
                                    ui.label(muted(format!("{p:.0}% ·")));
                                }
                            });
                        });
                    }
                    if temps.is_empty() && self.fans.is_empty() {
                        ui.label(muted("This computer doesn't report its temperatures to apps."));
                    }
                    for (l, t) in temps.iter().take(if self.fans.is_empty() { 6 } else { 5 - self.fans.len().min(4) }) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(kit::trunc_end(l, 30)).size(13.5));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(b(format!("{t:.0} °C"), 14.0)));
                        });
                    }
                });
            });
            ui.add_space(10.0);
            ui.columns(2, |c| {
                tile(&mut c[0], |ui| {
                    ui.set_min_height(170.0);
                    head(ui, Icon::Folder, "Disk usage");
                    let mut disks: Vec<_> = self.disks.iter().filter(|d| d.total_space() > 0).collect();
                    disks.sort_by_key(|d| std::cmp::Reverse(d.total_space()));
                    // macOS mounts the same disk as "/" and "/System/Volumes/Data": show it once.
                    disks.dedup_by_key(|d| (d.total_space(), d.available_space()));
                    for d in disks.into_iter().take(3) {
                        let (t, f) = (d.total_space(), d.available_space());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(kit::trunc_end(&d.mount_point().display().to_string(), 26)).size(13.5));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(muted(format!("{} free of {}", fmt_size(f), fmt_size(t)))));
                        });
                        ui.add(egui::ProgressBar::new((t - f) as f32 / t as f32).fill(accent()).desired_height(9.0).corner_radius(4.5));
                        ui.add_space(4.0);
                    }
                });
                tile(&mut c[1], |ui| {
                    ui.set_min_height(170.0);
                    head(ui, Icon::Battery, "Battery");
                    match &self.battery {
                        None => {
                            ui.label(muted("No battery found (a desktop computer, or the system doesn't share it)."));
                        }
                        Some(bt) => {
                            ui.label(b(format!("{:.0}%", bt.percent), 30.0));
                            ui.label(muted(bt.state.clone()));
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("Health {:.0}%", bt.health)).size(13.5));
                                if let Some(c) = bt.cycles {
                                    ui.label(RichText::new(format!("· {c} cycles")).size(13.5));
                                }
                                if let Some(t) = bt.temp {
                                    ui.label(RichText::new(format!("· {t:.0} °C")).size(13.5));
                                }
                            });
                        }
                    }
                });
            });
        });
    }

    // ---------- Statistics ----------

    pub fn stats_page(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        kit::heading(ui, "Statistics", "Check it out! You've done all this with Claybroom.");
        let r = egui::Rect::from_center_size(egui::pos2(full.right() - 14.0, full.top() + 60.0), Vec2::splat(26.0));
        kit::help(
            &mut ui.new_child(egui::UiBuilder::new().max_rect(r)),
            "Everything Claybroom has removed on this computer. The chart shows space (blue) and items (red) per month.",
        );
        // The ALL TIME / LAST 30 DAYS pill.
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 300.0).max(0.0) / 2.0);
            egui::Frame::new().fill(rail_active()).corner_radius(CornerRadius::same(22)).inner_margin(Margin::same(4)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (v, t) in [(false, "ALL TIME"), (true, "PAST MONTH")] {
                        let on = self.stats_30_days == v;
                        let btn = egui::Button::new(RichText::new(t).family(kit::bold_family()).color(if on { Color32::WHITE } else { ink() }))
                            .fill(if on { accent() } else { Color32::TRANSPARENT })
                            .corner_radius(CornerRadius::same(18))
                            .min_size(vec2(140.0, 32.0));
                        if ui.add(btn).clicked() {
                            self.stats_30_days = v;
                            self.stats_back = 0;
                        }
                    }
                });
            });
        });
        ui.add_space(12.0);
        let since = if self.stats_30_days { now_secs().saturating_sub(30 * 86400) } else { 0 };
        let log: Vec<Entry> = self.saved.log.iter().filter(|e| e.at >= since).cloned().collect();
        let sum = |keys: &[&str]| -> (u64, u64) { log.iter().filter(|e| keys.contains(&e.key.as_str())).fold((0, 0), |a, e| (a.0 + e.bytes, a.1 + e.items)) };
        let clutter_keys = ["App caches", "Log files", "Trash", "Downloads", "Large files", "Other"];
        let browser_keys = ["Cookies", "Browser caches", "Browsing history"];
        let all: Vec<&str> = clutter_keys.iter().chain(&browser_keys).copied().chain(["Duplicates", "Photos"]).collect();
        let (bytes, items) = sum(&all);
        // Chart columns: six months (or six 5-day spans in the last 30 days), paged with ‹ ›.
        let (labels, spans) = chart_columns(self.stats_30_days, self.stats_back);
        let per = |f: &dyn Fn(&Entry) -> f64| -> Vec<f64> { spans.iter().map(|(a, z)| log.iter().filter(|e| all.contains(&e.key.as_str()) && e.at >= *a && e.at < *z).map(f).sum()).collect() };
        let space = per(&|e| e.bytes as f64);
        let count = per(&|e| e.items as f64);
        let mut back = self.stats_back;
        egui::ScrollArea::vertical().show(ui, |ui| {
            tile(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                    kit::icon(ui.painter(), r, Icon::Check, ink());
                    ui.label(b("Here's what you've cleaned so far:", 17.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(back > 0, egui::Button::new(RichText::new("›").size(20.0)).frame(false)).clicked() {
                            back -= 1;
                        }
                        if ui.add(egui::Button::new(RichText::new("‹").size(20.0)).frame(false)).clicked() {
                            back += 1;
                        }
                    });
                });
                ui.add_space(8.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(210.0);
                        for (color, value, what, size) in [(accent(), fmt_size(bytes), "of space", 34.0), (bad(), items.to_string(), "items removed", 24.0)] {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                ui.painter().circle_filled(r.center(), 5.0, color);
                                ui.label(b(value, size));
                            });
                            ui.horizontal(|ui| {
                                ui.add_space(20.0);
                                ui.label(RichText::new(what).size(14.0));
                            });
                            ui.add_space(10.0);
                        }
                    });
                    kit::area_chart(ui, &labels, [(&count, bad()), (&space, accent())], 180.0);
                });
                ui.separator();
                ui.add_space(6.0);
                let eq = |per: f64| (bytes as f64 / per).round() as u64;
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(210.0);
                        ui.add_space(18.0);
                        ui.label(b("That's enough space for about...", 20.0));
                    });
                    let parts = [(Icon::Picture, eq(clean::PICTURE_BYTES), "pictures"), (Icon::Video, eq(clean::VIDEO_MINUTE_BYTES), "minutes of video"), (Icon::Music, eq(clean::SONG_BYTES), "songs")];
                    let w = (ui.available_width() - 80.0) / 3.0;
                    for (i, (ic, n, what)) in parts.into_iter().enumerate() {
                        if i > 0 {
                            ui.label(RichText::new("OR").family(kit::bold_family()).color(accent()));
                        }
                        ui.allocate_ui_with_layout(vec2(w, 110.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                            let (r, _) = ui.allocate_exact_size(Vec2::splat(54.0), Sense::hover());
                            kit::icon(ui.painter(), r, ic, ink());
                            ui.label(RichText::new(format!("{n} {what}")).size(16.0));
                        });
                    }
                });
            });
            ui.add_space(12.0);
            ui.label(b("Here's a breakdown of some details:", 17.0));
            ui.add_space(4.0);
            ui.columns(2, |cols| {
                tile(&mut cols[0], |ui| {
                    head(ui, Icon::Clutter, "Clutter removed");
                    let max = clutter_keys.iter().map(|k| sum(&[k]).0).max().unwrap_or(0).max(1);
                    for k in clutter_keys {
                        let (bytes, _) = sum(&[k]);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(k).size(14.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(b(if bytes == 0 { "0".into() } else { fmt_size(bytes) }, 14.0)));
                        });
                        ui.add(egui::ProgressBar::new(bytes as f32 / max as f32).fill(kit::good()).desired_height(4.0));
                    }
                });
                tile(&mut cols[1], |ui| {
                    head(ui, Icon::Globe, "Browser data removed");
                    for k in browser_keys {
                        let (bytes, items) = sum(&[k]);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(k).size(14.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(b(format!("{items} items / {}", if bytes == 0 { "0".into() } else { fmt_size(bytes) }), 14.0))
                            });
                        });
                        ui.add_space(8.0);
                    }
                });
            });
            ui.add_space(10.0);
            ui.columns(3, |cols| {
                let tiles = [(Icon::Duplicates, "Duplicates", "copies removed"), (Icon::Picture, "Photos", "photos removed"), (Icon::Trash, "Apps", "apps removed")];
                for (col, (ic, key, label)) in cols.iter_mut().zip(tiles) {
                    tile(col, |ui| {
                        ui.horizontal(|ui| {
                            kit::icon_box(ui, ic, 48.0);
                            ui.vertical(|ui| {
                                ui.label(b(sum(&[key]).1.to_string(), 26.0));
                                ui.label(RichText::new(label).size(13.5));
                            });
                        });
                    });
                }
            });
        });
        self.stats_back = back;
    }

    // ---------- Settings window ----------

    pub fn settings_page(&mut self, ui: &mut egui::Ui) {
        const TABS: [(Icon, &str); 7] = SETTINGS_TABS;
        egui::Frame::new()
            .fill(kit::bg())
            .corner_radius(CornerRadius::same(12))
            .shadow(egui::epaint::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(40) })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(ui.available_height());
                // Tab bar: icon above label, blue underline on the active one.
                egui::Frame::new().fill(kit::paper()).corner_radius(CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        for (i, (ic, name)) in TABS.iter().enumerate() {
                            let w = ui.fonts_mut(|f| f.layout_no_wrap(name.to_string(), egui::FontId::proportional(15.0), ink()).size().x) + 18.0;
                            let (r, resp) = ui.allocate_exact_size(vec2(w, 62.0), Sense::click());
                            let ir = egui::Rect::from_center_size(egui::pos2(r.center().x, r.top() + 16.0), Vec2::splat(24.0));
                            kit::icon(ui.painter(), ir, *ic, ink());
                            ui.painter().text(egui::pos2(r.center().x, r.top() + 44.0), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(15.0), ink());
                            if self.settings_tab == i {
                                ui.painter().hline(r.x_range(), r.bottom() + 9.0, Stroke::new(3.0, accent()));
                            }
                            if resp.clicked() {
                                self.settings_tab = i;
                            }
                        }
                    });
                });
                // One scroll position per tab, starting at the top.
                egui::ScrollArea::vertical().id_salt(("settings", self.settings_tab)).show(ui, |ui| {
                    egui::Frame::new().inner_margin(Margin::symmetric(56, 24)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.settings_tab_ui(ui);
                    });
                });
            });
    }

    fn settings_tab_ui(&mut self, ui: &mut egui::Ui) {
        let combo = |ui: &mut egui::Ui, id: &str, idx: &mut usize, opts: Vec<String>| {
            egui::ComboBox::from_id_salt(id).selected_text(opts[(*idx).min(opts.len() - 1)].clone()).show_ui(ui, |ui| {
                for (i, o) in opts.iter().enumerate() {
                    ui.selectable_value(idx, i, o);
                }
            });
        };
        let section = |ui: &mut egui::Ui, t: &str| {
            ui.add_space(8.0);
            ui.label(b(t, 18.0));
            ui.add_space(4.0);
        };
        let s = &mut self.saved.settings;
        match self.settings_tab {
            0 => {
                ui.vertical_centered(|ui| {
                    ui.label(b("Claybroom", 30.0));
                    ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(16.0));
                    ui.label(muted("Free and open source (MIT licence)."));
                });
                ui.add_space(14.0);
                ui.separator();
                let where_ = if cfg!(target_os = "macos") { "menu bar" } else { "system tray" };
                ui.add_space(8.0);
                kit::tick(ui, &mut s.tray, format!("Show the Claybroom icon in your {where_}"));
                ui.label(muted(format!("Claybroom keeps running there when you close its window; click the icon for the System monitor panel, right-click for Quit.")));
                ui.add_space(6.0);
                let menu = if cfg!(target_os = "macos") { "Finder's right-click menu (Quick Actions)" } else if cfg!(windows) { "File Explorer's right-click menu" } else { "GNOME Files' right-click menu (Scripts)" };
                kit::tick(ui, &mut s.folder_action, format!("Add \"{}\" to {menu}", crate::shell::FOLDER_ACTION));
                // Appearance: the warm look, following the system's light/dark or fixed.
                section(ui, "Appearance");
                ui.horizontal(|ui| {
                    let light = (Color32::from_rgb(0xfa, 0xf9, 0xf5), Color32::from_rgb(0xf0, 0xee, 0xe6), Color32::from_rgb(0x14, 0x14, 0x13));
                    let darkc = (Color32::from_rgb(0x26, 0x26, 0x24), Color32::from_rgb(0x1f, 0x1e, 0x1d), Color32::from_rgb(0xf3, 0xf1, 0xea));
                    for (i, name) in kit::THEMES.iter().enumerate() {
                        let chosen = s.appearance as usize == i;
                        let (cell, resp) = ui.allocate_exact_size(vec2(200.0, 98.0), Sense::click());
                        let r = egui::Rect::from_min_size(cell.min, vec2(cell.width(), 72.0));
                        let (bg, side, text) = if i == 2 { darkc } else { light };
                        let p = ui.painter();
                        p.rect_filled(r, 10.0, bg);
                        p.rect_filled(egui::Rect::from_min_size(r.min, vec2(46.0, r.height())), CornerRadius { nw: 10, sw: 10, ne: 0, se: 0 }, side);
                        if i == 0 {
                            // Match system: half light, half dark.
                            p.rect_filled(egui::Rect::from_min_max(egui::pos2(r.center().x + 20.0, r.top()), r.max), CornerRadius { ne: 10, se: 10, nw: 0, sw: 0 }, darkc.0);
                        }
                        p.text(r.min + vec2(60.0, 8.0), egui::Align2::LEFT_TOP, "Aa", egui::FontId::new(22.0, egui::FontFamily::Name("serif".into())), text);
                        p.rect_filled(egui::Rect::from_min_size(r.min + vec2(60.0, 42.0), vec2(56.0, 16.0)), 6.0, Color32::from_rgb(0xd9, 0x77, 0x57));
                        p.rect_stroke(r, 10.0, Stroke::new(if chosen { 2.5 } else { 1.0 }, if chosen { kit::accent() } else { line() }), egui::StrokeKind::Inside);
                        p.text(egui::pos2(r.left() + 2.0, cell.bottom()), egui::Align2::LEFT_BOTTOM, *name, egui::FontId::proportional(14.0), ink());
                        if resp.clicked() {
                            s.appearance = i as u8;
                            ui.ctx().request_repaint();
                        }
                    }
                });
                ui.label(muted("Cream paper, a clay accent and serif headings. Match system follows your computer's light or dark mode."));
                ui.add_space(10.0);
                if cfg!(target_os = "macos") {
                    ui.horizontal(|ui| {
                        if kit::outline(ui, "FULL DISK ACCESS…") {
                            clean::open_full_disk_access_settings();
                        }
                        ui.label(muted("Lets Claybroom see the Trash, Mail and every browser."));
                    });
                    ui.add_space(8.0);
                }
                section(ui, "Updates");
                kit::tick(ui, &mut s.check_updates, "Check for a newer Claybroom when it starts");
                ui.horizontal(|ui| {
                    let checking = self.update_job.is_some();
                    if kit::outline(ui, if checking { "CHECKING…" } else { "CHECK NOW" }) && !checking {
                        self.update_job = Some(crate::spawn(ui.ctx(), crate::update::latest));
                    }
                    match (&self.update, &self.update_note) {
                        (Some(rel), Some(note)) => {
                            let url = rel.url.clone();
                            ui.label(b(note.clone(), 14.0));
                            if ui.link(RichText::new("Download").color(accent())).clicked() {
                                crate::update::open_page(&url);
                            }
                        }
                        (_, Some(note)) => {
                            ui.label(muted(note.clone()));
                        }
                        _ => {
                            ui.label(muted("Asks GitHub for the latest release; nothing else is sent."));
                        }
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if kit::outline(ui, "RESET STATISTICS") {
                        self.confirm = Some(Confirm::ResetStats);
                    }
                    ui.label(muted("Forget everything the Statistics page counts."));
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if kit::outline(ui, "CHECK THE RULES") {
                        self.rules_check = Some(crate::mechanics::compare());
                    }
                    ui.label(muted("Runs the cleaners on test files and compares what they do with the rules list."));
                });
                if let Some(rows) = &self.rules_check {
                    rules_table(ui, rows);
                }
            }
            1 => {
                section(ui, "Clutter");
                kit::tick(ui, &mut s.scan_downloads, "Include Downloads");
                if cfg!(target_os = "macos") {
                    kit::tick(ui, &mut s.scan_external, "Include external drives");
                }
                kit::tick(ui, &mut s.scan_system, "Include system caches and logs");
                kit::tick(ui, &mut s.scan_large_files, "Scan for large files to be deleted");
                ui.horizontal(|ui| {
                    ui.label("Look for files larger than:");
                    combo(ui, "lf", &mut s.large_file_idx, clean::LARGE_FILE_SIZES.iter().map(|b| fmt_size(*b)).collect());
                });
                ui.horizontal(|ui| {
                    ui.label("Look for large files in:");
                    let mut i = s.large_whole_disk as usize;
                    combo(ui, "lw", &mut i, vec!["My home folder".into(), "The whole computer (slower)".into()]);
                    s.large_whole_disk = i == 1;
                });
                section(ui, "Duplicates");
                kit::tick(ui, &mut s.dup_ignore_hidden, "Skip hidden files");
                kit::tick(ui, &mut s.dup_to_trash, "Send removed copies to the Trash");
                ui.horizontal(|ui| {
                    ui.label("Only files bigger than:");
                    combo(ui, "dm", &mut s.dup_min_idx, clean::DUP_SIZES.iter().map(|b| fmt_size(*b)).collect());
                });
                section(ui, "Photos");
                kit::tick(ui, &mut s.photos_to_trash, "Send removed photos to the Trash");
                if cfg!(target_os = "macos") {
                    kit::tick(ui, &mut s.photos_library, "Include my Photos library (photos to remove go to an album in Photos)");
                }
                section(ui, "Apps");
                ui.horizontal(|ui| {
                    ui.label("Count an app as unused after:");
                    combo(ui, "ua", &mut s.unused_idx, clean::UNUSED_DAYS.iter().map(|d| format!("{} or more", d.1)).collect());
                });
                ui.horizontal(|ui| {
                    ui.label("Count an app as large from:");
                    combo(ui, "la", &mut s.large_app_idx, clean::LARGE_APP_SIZES.iter().map(|b| fmt_size(*b)).collect());
                });
            }
            2 => {
                // only delete history older than…
                ui.horizontal(|ui| {
                    kit::tick(ui, &mut s.history_older, "Only delete Browser History older than:");
                    combo(ui, "ha", &mut s.history_age_idx, crate::schedule::HISTORY_AGES.iter().map(|a| a.1.to_string()).collect());
                });
                ui.label(muted("Recent history stays; Firefox never loses bookmarks."));
                // scheduled cleaning, one row per browser.
                section(ui, "On a schedule");
                ui.label("Clean your browsers' cache, history and cookies automatically. A small background job checks every hour, even when Claybroom is closed, and skips a browser while it's open.");
                ui.add_space(6.0);
                let last = crate::schedule::last_runs();
                for d in clean::browser_defs() {
                    let name = d.name.to_string();
                    let on = !s.browsers_off.contains(&name);
                    let mut sch = s.schedules.get(&name).copied().unwrap_or_default();
                    let open = self.sched_open.as_deref() == Some(d.name);
                    ui.horizontal(|ui| {
                        ui.set_min_height(46.0);
                        kit::badge(ui, d.name, 30.0);
                        ui.label(b(d.name, 15.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(if open { "⌃" } else { "⌄" }).frame(false)).clicked() {
                                self.sched_open = if open { None } else { Some(name.clone()) };
                            }
                            egui::ComboBox::from_id_salt(("sched", d.name))
                                .selected_text(RichText::new(crate::schedule::FREQ_NAMES[sch.freq].to_uppercase()).family(kit::bold_family()))
                                .width(130.0)
                                .show_ui(ui, |ui| {
                                    for (i, f) in crate::schedule::FREQ_NAMES.iter().enumerate() {
                                        ui.selectable_value(&mut sch.freq, i, *f);
                                    }
                                });
                            ui.add_space(10.0);
                            if kit::switch(ui, on, true) {
                                if on {
                                    s.browsers_off.push(name.clone());
                                } else {
                                    s.browsers_off.retain(|o| o != &name);
                                }
                            }
                            ui.label(muted(if on { "Scanned" } else { "Not scanned" }));
                        });
                    });
                    if open {
                        ui.horizontal(|ui| {
                            ui.add_space(44.0);
                            ui.label(muted("Clean on schedule:"));
                            kit::tick(ui, &mut sch.cache, "Cache");
                            kit::tick(ui, &mut sch.history, "History");
                            kit::tick(ui, &mut sch.cookies, "Cookies");
                            let when = last.get(&name).map(|t| kit::date_time(std::time::UNIX_EPOCH + std::time::Duration::from_secs(*t)));
                            ui.label(muted(when.map_or("Not cleaned on schedule yet".into(), |w| format!("Last cleaned {w}"))));
                        });
                    }
                    if sch != crate::schedule::Schedule::default() || s.schedules.contains_key(&name) {
                        s.schedules.insert(name.clone(), sch);
                    }
                    let r = ui.cursor();
                    ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, line()));
                }
                // the cookie allowlist.
                section(ui, "Cookie Allowlist");
                ui.label("When you clean cookies, you're signed out of websites. Add the sites you want to stay signed in to, and their cookies are kept (Chrome, Edge, Brave, Opera, Vivaldi and Firefox).");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if kit::outline(ui, "IMPORT BOOKMARKS") {
                        let found = crate::browser_pro::bookmark_hosts();
                        let before = s.allowlist.len();
                        for h in found {
                            if !s.allowlist.contains(&h) {
                                s.allowlist.push(h);
                            }
                        }
                        self.import_note = Some(format!("{} site(s) added from your bookmarks.", s.allowlist.len() - before));
                    }
                    ui.label(RichText::new("OR").family(kit::bold_family()).color(accent()));
                    let add = ui.add(egui::TextEdit::singleline(&mut self.allow_input).hint_text("example.com").desired_width(200.0));
                    let enter = add.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if kit::outline(ui, "ADD MANUALLY") || enter {
                        let typed = self.allow_input.trim().to_lowercase();
                        let host = crate::browser_pro::host_of(&typed).or_else(|| crate::browser_pro::host_of(&format!("https://{typed}")));
                        if let Some(h) = host.filter(|h| !s.allowlist.contains(h)) {
                            s.allowlist.push(h);
                            self.allow_input.clear();
                        }
                    }
                });
                if let Some(n) = &self.import_note {
                    ui.label(muted(n.clone()));
                }
                ui.add_space(6.0);
                if s.allowlist.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.label(b("No websites yet!", 16.0));
                    });
                }
                let mut gone = None;
                for (i, h) in s.allowlist.iter().enumerate() {
                    ui.horizontal(|ui| {
                        kit::badge(ui, h, 24.0);
                        ui.label(RichText::new(h).size(14.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("×").size(18.0).color(dim())).frame(false)).clicked() {
                                gone = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = gone {
                    s.allowlist.remove(i);
                }
            }
            3 => {
                ui.horizontal(|ui| {
                    kit::tick(ui, &mut s.trash_auto, "Empty Trash items older than:");
                    combo(ui, "td", &mut s.trash_days_idx, clean::TRASH_DAYS.iter().map(|d| if *d == 1 { "1 day".into() } else { format!("{d} days") }).collect());
                });
                ui.label(muted("Checked every hour in the background, even when Claybroom is closed."));
            }
            4 => {
                ui.horizontal(|ui| {
                    kit::tick(ui, &mut s.alert_clutter, "Tell me when clutter passes:");
                    combo(ui, "ac", &mut s.alert_clutter_idx, clean::CLUTTER_ALERT.iter().map(|b| fmt_size(*b)).collect());
                });
                ui.horizontal(|ui| {
                    kit::tick(ui, &mut s.alert_disk, "Tell me when free space drops below:");
                    combo(ui, "ad", &mut s.alert_disk_idx, clean::DISK_ALERT.iter().map(|b| fmt_size(*b)).collect());
                });
                ui.label(muted("Notices appear on the Overview page."));
            }
            5 => {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(b("Free, forever", 26.0));
                    ui.label(RichText::new("No subscription, no licence key, no account. Every feature is unlocked.").size(16.0));
                });
            }
            _ => {
                section(ui, "Privacy");
                ui.label("Claybroom works only on this computer. It sends no usage data, shows no offers and does no tracking. The only time it goes online is the update check in General, which is off unless you turn it on.");
                section(ui, "Improvements");
                ui.label(muted("Found a bug or have an idea? The project is open source: anyone can read the code and suggest changes."));
                ui.add_space(20.0);
                ui.vertical_centered(|ui| ui.label(muted("These settings apply only to Claybroom")));
            }
        }
    }

    /// Overview notices for Settings › Alerts.
    /// The alerts that apply now (Settings › Alerts).
    pub fn alert_lines(&self) -> Vec<String> {
        let s = &self.saved.settings;
        let mut lines = vec![];
        if s.alert_disk {
            if let Some(d) = self.disks.iter().max_by_key(|d| d.total_space()) {
                let limit = clean::DISK_ALERT[s.alert_disk_idx.min(3)];
                if d.available_space() < limit {
                    lines.push(format!("Your disk has less than {} free.", fmt_size(limit)));
                }
            }
        }
        if s.alert_clutter && self.saved.last_clutter_total > clean::CLUTTER_ALERT[s.alert_clutter_idx.min(3)] {
            lines.push(format!("Your last scan found {} of clutter.", fmt_size(self.saved.last_clutter_total)));
        }
        lines
    }

    /// While Claybroom's window is hidden or in the background, each alert becomes a system
    /// notification once (checked every 30 minutes).
    pub fn alert_notifications(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.last_alert_check < 1800.0 {
            return;
        }
        self.last_alert_check = now;
        self.disks.refresh(true);
        let lines = self.alert_lines();
        self.notified.retain(|l| lines.contains(l));
        let away = !self.visible || ctx.input(|i| i.viewport().focused) == Some(false);
        if away {
            for l in lines {
                if self.notified.insert(l.clone()) {
                    crate::shell::notify(&format!("{l} Open Claybroom to clean up."));
                }
            }
        }
    }

    pub fn alerts(&mut self, ui: &mut egui::Ui) {
        let lines: Vec<(String, Option<crate::Page>)> = self.alert_lines().into_iter().map(|l| (l, Some(crate::Page::Clutter))).collect();
        for (t, page) in lines {
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    let w = 420.0;
                    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                    ui.label(mixed(&[("⚠ ", size_color()), (&t, ink())], 15.0));
                    if let Some(p) = page {
                        if ui.link(RichText::new("Clean clutter").color(accent())).clicked() {
                            self.page = p;
                        }
                    }
                });
            });
        }
    }
}

/// The rules check: each value of the rules list, what this build measurably does, ✓ or ✗.
fn rules_table(ui: &mut egui::Ui, rows: &[crate::mechanics::Row]) {
    let measured: Vec<_> = rows.iter().filter(|r| r.measured.is_some()).collect();
    let ok = measured.iter().filter(|r| r.ok).count();
    ui.add_space(10.0);
    let (col, verdict) = if ok == measured.len() { (kit::good(), "all match") } else { (bad(), "some differ") };
    ui.label(mixed(&[(&format!("{ok} of {} measured rules match", measured.len()), ink()), (&format!(" — {verdict}"), col)], 17.0));
    let rest = rows.len() - measured.len();
    ui.label(muted(match rest {
        0 => "Every rule was measured.".to_string(),
        1 => "1 more is a description that can't be measured.".to_string(),
        n => format!("{n} more are descriptions that can't be measured."),
    }));
    ui.add_space(6.0);
    egui::Grid::new("rules").striped(true).num_columns(4).spacing([14.0, 6.0]).show(ui, |ui| {
        for t in ["", "Rule", "Rules list", "This build"] {
            ui.label(muted(t));
        }
        ui.end_row();
        for r in rows {
            let (mark, c) = match (&r.measured, r.ok) {
                (None, _) => ("–", dim()),
                (Some(_), true) => ("✓", kit::good()),
                (Some(_), false) => ("✗", bad()),
            };
            ui.label(RichText::new(mark).family(kit::bold_family()).size(16.0).color(c));
            ui.label(RichText::new(&r.name).size(13.5)).on_hover_text(&r.how);
            ui.label(RichText::new(kit::trunc_end(&r.sheet, 40)).size(13.0)).on_hover_text(&r.sheet);
            let m = r.measured.clone().unwrap_or_else(|| "not measurable".into());
            ui.label(RichText::new(kit::trunc_end(&m, 40)).size(13.0).color(if r.ok { ink() } else { dim() })).on_hover_text(m);
            ui.end_row();
        }
    });
}

fn head(ui: &mut egui::Ui, ic: Icon, title: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
        kit::icon(ui.painter(), r, ic, ink());
        ui.label(b(title, 17.0));
    });
    ui.add_space(6.0);
}

/// Labels and [start, end) times of the chart's six columns: months, or 5-day spans in the last
/// 30 days; `back` pages towards the past.
fn chart_columns(last_30: bool, back: i32) -> (Vec<String>, Vec<(u64, u64)>) {
    use chrono::{Datelike, Local, TimeZone};
    let now = Local::now();
    if last_30 {
        let end = now.timestamp() as u64 - back as u64 * 30 * 86400;
        let spans: Vec<(u64, u64)> = (0..6).rev().map(|i| (end - (i + 1) * 5 * 86400, end - i * 5 * 86400)).collect();
        let labels = spans.iter().map(|(a, _)| Local.timestamp_opt(*a as i64, 0).unwrap().format("%-d %b").to_string()).collect();
        return (labels, spans);
    }
    let mut cols = vec![];
    for i in (0..6).rev() {
        let k = now.year() * 12 + now.month0() as i32 - i - back * 6;
        let (y, m) = (k.div_euclid(12), k.rem_euclid(12) as u32 + 1);
        let start = Local.with_ymd_and_hms(y, m, 1, 0, 0, 0).single().map_or(0, |d| d.timestamp().max(0) as u64);
        let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        let end = Local.with_ymd_and_hms(ny, nm, 1, 0, 0, 0).single().map_or(u64::MAX, |d| d.timestamp().max(0) as u64);
        let label = Local.with_ymd_and_hms(y, m, 1, 0, 0, 0).single().map_or(String::new(), |d| d.format("%b").to_string());
        cols.push((label, (start, end)));
    }
    cols.into_iter().unzip()
}
