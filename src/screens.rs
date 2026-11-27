//! Scan, results and done screens, laid out.

use crate::clean::{self, fmt_size, Category, Kind, Preselect};
use crate::kit::{self, b, muted, primary, Icon, line, dim, accent, size_color, ink};
use crate::{intro, recommended, Confirm, Page, Sweep};
use eframe::egui::{self, text::LayoutJob, vec2, Color32, CornerRadius, Margin, RichText, Sense, Stroke, TextFormat, Vec2};

#[derive(Clone, Copy, PartialEq)]
pub enum SortBy {
    Size,
    Name,
    Date,
}

pub struct DoneScreen {
    pub page: Page,
    pub title: String,
    pub lines: Vec<String>,
    /// System items an administrator could still remove (path, bytes).
    pub admin: Vec<(std::path::PathBuf, u64)>,
}

/// One label with differently coloured parts, e.g. "We've found " + orange "17.06 GB".
pub(crate) fn mixed(parts: &[(&str, Color32)], size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (t, c) in parts {
        job.append(t, 0.0, TextFormat { font_id: egui::FontId::new(size, kit::bold_family()), color: *c, ..Default::default() });
    }
    job
}

/// Centered results heading with an optional "!" badge, and a "?" at the far right.
pub(crate) fn results_heading(ui: &mut egui::Ui, parts: &[(&str, Color32)], badge: bool, help: &str) {
    let full = ui.max_rect();
    ui.add_space(4.0);
    ui.vertical_centered(|ui| {
        ui.horizontal(|ui| {
            let mut job = mixed(parts, 28.0);
            job.sections.iter_mut().for_each(|x| x.format.font_id.family = kit::head_family());
            let w = ui.fonts_mut(|f| f.layout_job(job.clone())).size().x + if badge { 44.0 } else { 0.0 };
            ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
            if badge {
                kit::alert_badge(ui);
            }
            ui.label(job);
        });
    });
    let r = egui::Rect::from_center_size(egui::pos2(full.right() - 14.0, full.top() + 22.0), Vec2::splat(26.0));
    // Placed on top of the layout, so it doesn't push the content around.
    kit::help(&mut ui.new_child(egui::UiBuilder::new().max_rect(r)), help);
    ui.add_space(10.0);
}

fn cat_icon(title: &str) -> Icon {
    match title {
        "App caches" => Icon::Folder,
        "Log Files" => Icon::Monitor,
        "Trash" => Icon::Trash,
        "Downloads" => Icon::Folder,
        "Developer leftovers" => Icon::Gear,
        "Junk on external drives" => Icon::Duplicates,
        "Cookies" => Icon::Shield,
        "Web cache" | "Caches" => Icon::Globe,
        "Browsing history" => Icon::Stats,
        clean::AUTOFILL => Icon::Folder,
        clean::PASSWORDS => Icon::Lock,
        _ => Icon::Clutter,
    }
}

pub(crate) fn round_icon(ui: &mut egui::Ui, ic: Icon, size: f32, on_blue: bool) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let fill = if on_blue { Color32::from_white_alpha(40) } else { kit::soft() };
    ui.painter().circle_filled(r.center(), size / 2.0, fill);
    kit::icon(ui.painter(), r.shrink(size * 0.26), ic, if on_blue { Color32::WHITE } else { ink() });
}

/// Tri-state checkbox over a set of items; returns the new value when clicked.
pub(crate) fn tri_checkbox(ui: &mut egui::Ui, selected: usize, total: usize) -> Option<bool> {
    let all = selected == total && total > 0;
    kit::check(ui, all, selected > 0 && !all, total > 0).then_some(!all)
}

pub(crate) fn checkbox(ui: &mut egui::Ui, v: &mut bool) {
    if kit::check(ui, *v, false, true) {
        *v = !*v;
    }
}

fn sort_items(items: &mut [clean::Item], by: SortBy) {
    match by {
        SortBy::Size => items.sort_by(|a, b| b.size.cmp(&a.size)),
        SortBy::Name => items.sort_by_key(|i| i.label.to_lowercase()),
        SortBy::Date => items.sort_by(|a, b| b.modified.cmp(&a.modified)),
    }
}

fn sort_combo(ui: &mut egui::Ui, id: &str, by: &mut SortBy) -> bool {
    let before = *by;
    egui::ComboBox::from_id_salt(id).selected_text("Sort by").width(110.0).show_ui(ui, |ui| {
        ui.selectable_value(by, SortBy::Size, "Size");
        ui.selectable_value(by, SortBy::Name, "Name");
        ui.selectable_value(by, SortBy::Date, "Date modified");
    });
    before != *by
}

/// Open the file manager with this file selected.
pub fn reveal(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
}

/// "13.43 GB selected" with the size in orange, or "Nothing selected".
pub(crate) fn selected_parts(size: u64, n: usize) -> Vec<(String, Color32)> {
    if n == 0 {
        vec![("Nothing selected".into(), ink())]
    } else {
        vec![(fmt_size(size), size_color()), (" selected".into(), ink())]
    }
}

impl Sweep {
    /// The bottom bar of results screens: a selection preset on the left, the selected total and
    /// the action on the right. Returns true when the action is pressed.
    pub(crate) fn bottom_bar(&mut self, ui: &mut egui::Ui, id: &str, left: impl FnOnce(&mut egui::Ui), summary: Vec<(String, Color32)>, verb: &str, enabled: bool) -> bool {
        let mut go = false;
        egui::Panel::bottom(id.to_string())
            .frame(egui::Frame::new().inner_margin(Margin { left: 8, right: 8, top: 14, bottom: 4 }))
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().hline(r.x_range(), r.top() - 6.0, Stroke::new(1.0, line()));
                ui.horizontal(|ui| {
                    left(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(RichText::new(verb).family(kit::bold_family()).size(16.0).color(if enabled { Color32::WHITE } else { dim() }))
                            .fill(if enabled { accent() } else { kit::disabled() })
                            .corner_radius(CornerRadius::same(4))
                            .min_size(vec2(140.0, 46.0));
                        go = ui.add_enabled(enabled, btn).clicked();
                        ui.add_space(14.0);
                        let parts: Vec<(&str, Color32)> = summary.iter().map(|(t, c)| (t.as_str(), *c)).collect();
                        ui.label(mixed(&parts, 21.0));
                    });
                });
            });
        go
    }

    /// The scanning (or cleaning) screen. Returns true when CANCEL is pressed.
    pub fn scanning(&self, ui: &mut egui::Ui, title: &str, can_cancel: bool) -> bool {
        let st = self.progress.lock().unwrap().clone();
        let time = ui.input(|i| i.time);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        let mut cancel = false;
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);
            ui.label(b(title, 32.0));
            ui.add_space(24.0);
            kit::progress_ring(ui, st.frac, time);
            ui.add_space(14.0);
            ui.label(RichText::new(kit::trunc_mid(&st.text, 70)).size(16.0).color(dim()));
            if st.found > 0 {
                ui.label(mixed(&[(&fmt_size(st.found), accent()), (" found so far", ink())], 16.0));
            }
            ui.add_space(14.0);
            if can_cancel {
                cancel = kit::outline(ui, "CANCEL");
            }
        });
        cancel
    }

    /// Shown after cleaning, or when there was nothing to clean.
    pub fn done_screen(&mut self, ui: &mut egui::Ui) {
        let Some(d) = &self.done else { return };
        let (title, lines, admin) = (d.title.clone(), d.lines.clone(), d.admin.clone());
        ui.vertical_centered(|ui| {
            ui.add_space(70.0);
            ui.label(b(title, 34.0));
            ui.add_space(14.0);
            kit::done_check(ui);
            ui.add_space(14.0);
            for l in &lines {
                ui.label(RichText::new(l).size(18.0));
            }
            if !admin.is_empty() {
                // The rest needs an administrator: the system asks, then only these items go.
                let bytes: u64 = admin.iter().map(|a| a.1).sum();
                ui.add_space(14.0);
                ui.label(mixed(&[(&format!("{} system item(s) ", admin.len()), ink()), (&fmt_size(bytes), size_color()), (" need an administrator.", ink())], 17.0));
                ui.add_space(6.0);
                let how = if cfg!(target_os = "macos") { "macOS asks for your administrator password (or Touch ID, where your Mac offers it)." } else if cfg!(windows) { "Windows asks for permission." } else { "Your system asks for your password." };
                ui.label(muted(how));
                ui.add_space(6.0);
                if kit::outline(ui, "CLEAN AS ADMINISTRATOR") {
                    self.clean_as_admin(ui.ctx(), admin.clone());
                }
            }
            ui.add_space(26.0);
            if ui.link(RichText::new("Open statistics").size(16.0).color(accent())).clicked() {
                self.page = Page::Stats;
                self.done = None;
            }
            ui.add_space(60.0);
            if kit::cta(ui, "DONE", true) {
                self.done = None;
            }
        });
    }

    pub(crate) fn nothing_to_clean(&mut self) {
        self.done = Some(DoneScreen {
            page: self.page,
            title: "All clear".into(),
            lines: vec!["Your computer is already spotless.".into(), "Come back in a while.".into()],
            admin: vec![],
        });
    }

    // ---------- Quick Clean ----------

    pub fn quick_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.clean_job.is_some() {
            self.scanning(ui, "Cleaning...", false);
            return;
        }
        if self.quick_job.is_some() {
            if self.scanning(ui, "Scanning...", true) {
                self.quick_job = None;
            }
            return;
        }
        if self.quick.is_none() {
            let rows = [
                (Icon::Broom, "Clean with ease", "Remove files you don't need in a few steps."),
                (Icon::Folder, "Free up space", "Caches, logs and the Trash in one pass."),
                (Icon::Speed, "Speed up", "Less clutter for your computer to wade through."),
                (Icon::Shield, "Help your privacy", "Clear browser caches, history and cookies."),
            ];
            let sub = "Run a quick scan and remove unwanted data to save space and protect your privacy.";
            if intro(ui, "Quick Clean", sub, rows, "START SCANNING", self.clean_job.is_none(), accent()) {
                self.start_quick(&ctx);
            }
            return;
        }
        let cats = self.quick.as_ref().unwrap();
        let total: u64 = cats.iter().map(|c| c.size()).sum();
        if total == 0 {
            self.quick = None;
            return self.nothing_to_clean();
        }
        let (n, sel): (usize, u64) = (cats.iter().map(|c| c.selected_count()).sum(), cats.iter().map(|c| c.selected_size()).sum());
        let size = fmt_size(total);
        results_heading(
            ui,
            &[("Here is what you can clean and free up to ", ink()), (&size, size_color())],
            false,
            "Caches and logs are rebuilt by apps when needed. Cookies are not ticked: removing them signs you out of websites.",
        );
        let mut preset = None;
        let clean = self.bottom_bar(
            ui,
            "quick_bar",
            |ui| preset = preset_combo(ui, "quick_pre"),
            selected_parts(sel, n),
            "CLEAN",
            n > 0,
        );
        if let Some(how) = preset {
            apply_preset(self.quick.as_mut().unwrap(), how);
        }
        if clean {
            self.confirm = Some(Confirm::Quick);
        }
        self.fda_banner(ui);
        self.running_warning(ui);
        let mut toggle_all = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (ci, c) in self.quick.as_mut().unwrap().iter_mut().enumerate() {
                let empty = c.items.is_empty();
                let key = format!("quick/{}", c.title);
                ui.add_enabled_ui(!empty, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_min_height(62.0);
                        ui.add_space(16.0);
                        if let Some(v) = tri_checkbox(ui, c.selected_count(), c.items.len()) {
                            c.items.iter_mut().for_each(|i| i.selected = v);
                        }
                        ui.add_space(12.0);
                        round_icon(ui, cat_icon(&c.title), 40.0, false);
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.add_space(10.0);
                            ui.label(b(&c.title, 16.0));
                            ui.label(RichText::new(c.tip).size(13.5).color(dim()));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(10.0);
                            if !empty && ui.add(egui::Button::new(if c.open { "⌃" } else { "⌄" }).frame(false)).clicked() {
                                c.open = !c.open;
                            }
                            ui.add_sized([90.0, 20.0], egui::Label::new(b(if empty { String::new() } else { fmt_size(c.size()) }, 16.0)));
                            ui.add_sized([90.0, 20.0], egui::Label::new(muted(format!("{} items", c.items.len()))));
                        });
                    });
                });
                if c.open && !empty {
                    let all = self.show_all.contains(&key);
                    egui::Frame::new().fill(Color32::from_rgba_premultiplied(0x0a, 0x0b, 0x10, 0x14)).inner_margin(Margin::symmetric(70, 6)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for it in c.items.iter_mut().take(if all { usize::MAX } else { 5 }) {
                            ui.horizontal(|ui| {
                                ui.set_min_height(34.0);
                                checkbox(ui, &mut it.selected);
                                ui.label(RichText::new(kit::trunc_mid(&it.label, 60)).size(15.0)).on_hover_text(it.path.display().to_string());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(40.0);
                                    ui.label(b(fmt_size(it.size), 15.0));
                                });
                            });
                        }
                        if c.items.len() > 5 {
                            ui.vertical_centered(|ui| {
                                if ui.link(muted(if all { "Show less" } else { "Show all" })).clicked() {
                                    toggle_all = Some(key.clone());
                                }
                            });
                        }
                    });
                }
                let _ = ci;
                let r = ui.cursor();
                ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, line()));
            }
        });
        if let Some(k) = toggle_all {
            if !self.show_all.remove(&k) {
                self.show_all.insert(k);
            }
        }
    }

    fn running_warning(&self, ui: &mut egui::Ui) {
        let cats = self.quick.iter().flatten().chain(self.browsers.iter().flatten().flat_map(|b| b.cats.iter()));
        let mut open: Vec<&str> = vec![];
        for i in cats.flat_map(|c| c.items.iter()) {
            if let Some(app) = i.app {
                if i.procs.iter().any(|p| self.running.iter().any(|r| r == p)) && !open.contains(&app) {
                    open.push(app);
                }
            }
        }
        if !open.is_empty() {
            ui.label(RichText::new(format!("⚠ Close {} first — open browsers are skipped when cleaning.", open.join(", "))).color(kit::warn()));
        }
    }

    // ---------- Clean clutter ----------

    pub fn clutter_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.clean_job.is_some() {
            self.scanning(ui, "Cleaning...", false);
            return;
        }
        if self.clutter_job.is_some() {
            if self.scanning(ui, "Looking for clutter…", true) {
                self.clutter_job = None;
            }
            return;
        }
        if self.clutter.is_none() {
            let rows = [
                (Icon::Folder, "Reclaim disk space", "Gigabytes of junk, gone in one pass."),
                (Icon::Speed, "Less to index", "Fewer stale files for search and backups to wade through."),
                (Icon::Check, "Fix odd errors", "Old caches and logs can confuse apps."),
                (Icon::Clutter, "Beyond the caches", "Downloads, Trash, developer junk and large files."),
            ];
            if intro(ui, "Clean clutter", "Caches, logs and other files your apps can rebuild.", rows, "SCAN CLUTTER", self.clean_job.is_none(), Color32::from_rgb(0x4a, 0x7b, 0xd9)) {
                self.start_clutter(&ctx);
            }
            return;
        }
        let cats = self.clutter.as_ref().unwrap();
        let total: u64 = cats.iter().map(|c| c.size()).sum();
        if total == 0 {
            self.clutter = None;
            return self.nothing_to_clean();
        }
        let (n, sel): (usize, u64) = (cats.iter().map(|c| c.selected_count()).sum(), cats.iter().map(|c| c.selected_size()).sum());
        let size = fmt_size(total);
        results_heading(
            ui,
            &[("We've found ", ink()), (&size, size_color()), (" of clutter", ink())],
            true,
            "Clutter is app caches, logs, the Trash, Downloads and other files you can safely remove. Personal files (Downloads, large files) go to the Trash; junk is deleted.",
        );
        let mut preset = None;
        let clean = self.bottom_bar(ui, "clutter_bar", |ui| preset = preset_combo(ui, "clutter_pre"), selected_parts(sel, n), "CLEAN", n > 0);
        if let Some(how) = preset {
            apply_preset(self.clutter.as_mut().unwrap(), how);
        }
        if clean {
            self.confirm = Some(Confirm::Clutter);
        }
        self.fda_banner(ui);

        let avail = ui.available_size();
        let left_w = 250.0;
        let mut pick = None;
        let mut row_ys = vec![];
        ui.horizontal_top(|ui| {
            // Left: the categories.
            ui.allocate_ui_with_layout(vec2(left_w, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::ScrollArea::vertical().id_salt("clutter_left").show(ui, |ui| {
                    for (i, c) in self.clutter.as_mut().unwrap().iter_mut().enumerate() {
                        let active = self.clutter_sel == Some(i);
                        let empty = c.items.is_empty();
                        let (rect, resp) = ui.allocate_exact_size(vec2(left_w, 66.0), if empty { Sense::hover() } else { Sense::click() });
                        row_ys.push(rect.center().y);
                        if active {
                            ui.painter().rect_filled(rect, 0.0, accent());
                            let tip = rect.right_center();
                            ui.painter().add(egui::Shape::convex_polygon(
                                vec![tip + vec2(-1.0, -8.0), tip + vec2(8.0, 0.0), tip + vec2(-1.0, 8.0)],
                                accent(),
                                Stroke::NONE,
                            ));
                        } else {
                            ui.painter().hline((rect.left() + 56.0)..=rect.right(), rect.bottom(), Stroke::new(1.0, line()));
                        }
                        let fg = if active { Color32::WHITE } else if empty { kit::faint() } else { ink() };
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                        child.add_enabled_ui(!empty, |ui| {
                            if let Some(v) = tri_checkbox(ui, c.selected_count(), c.items.len()) {
                                c.items.iter_mut().for_each(|it| it.selected = v);
                            }
                        });
                        let (ir, _) = child.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                        kit::icon(child.painter(), ir, cat_icon(&c.title), fg);
                        child.vertical(|ui| {
                            ui.add_space(12.0);
                            ui.label(RichText::new(kit::trunc_end(&c.title, 22)).family(kit::bold_family()).size(15.0).color(fg));
                            if !empty {
                                let col = if c.selected_count() > 0 { size_color() } else if active { Color32::from_white_alpha(210) } else { dim() };
                                ui.label(RichText::new(fmt_size(c.size())).size(13.5).color(col));
                            }
                        });
                        if !active && !empty {
                            child.painter().text(rect.right_center() - vec2(14.0, 0.0), egui::Align2::CENTER_CENTER, "›", egui::FontId::proportional(20.0), dim());
                        }
                        if resp.clicked() {
                            pick = Some(if active { None } else { Some(i) });
                        }
                    }
                });
            });
            let r = ui.cursor();
            ui.painter().vline(r.left(), r.top()..=r.top() + avail.y, Stroke::new(1.0, line()));
            ui.add_space(16.0);
            // Right: an overview of every category, or the chosen one's files.
            ui.vertical(|ui| self.clutter_detail(ui, &row_ys));
        });
        if let Some(p) = pick {
            self.clutter_sel = p;
        }
    }

    fn clutter_detail(&mut self, ui: &mut egui::Ui, row_ys: &[f32]) {
        let sort = &mut self.sort;
        let cats = self.clutter.as_mut().unwrap();
        let max = cats.iter().map(|c| c.size()).max().unwrap_or(1).max(1);
        let bar = |ui: &mut egui::Ui, c: &Category| {
            let w = ui.available_width() - 10.0;
            let (r, _) = ui.allocate_exact_size(vec2(w, 6.0), Sense::hover());
            let len = (c.size() as f32 / max as f32 * w).max(6.0);
            let col = if c.selected_count() > 0 { size_color() } else { kit::faint() };
            ui.painter().rect_filled(egui::Rect::from_min_size(r.min, vec2(len, 5.0)), 2.5, col);
        };
        match self.clutter_sel.filter(|&i| i < cats.len()) {
            None => {
                // One bar per category, level with its row on the left.
                let x = ui.cursor().left();
                let w = ui.available_width() - 10.0;
                for (c, &y) in cats.iter().zip(row_ys) {
                    if c.items.is_empty() {
                        ui.painter().text(egui::pos2(x, y), egui::Align2::LEFT_CENTER, "Already clean", egui::FontId::proportional(15.0), dim());
                    } else {
                        let len = (c.size() as f32 / max as f32 * w).max(6.0);
                        let col = if c.selected_count() > 0 { size_color() } else { kit::faint() };
                        ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(x, y - 2.5), vec2(len, 5.0)), 2.5, col);
                    }
                }
            }
            Some(i) => {
                let c = &mut cats[i];
                ui.add_space(8.0);
                bar(ui, c);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if sort_combo(ui, "clutter_sort", sort) {
                        sort_items(&mut c.items, *sort);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.add_sized([120.0, 18.0], egui::Label::new(muted("Date Modified")));
                        ui.add_sized([90.0, 18.0], egui::Label::new(muted("Size")));
                    });
                });
                let r = ui.cursor();
                ui.painter().hline(r.x_range(), r.top() + 2.0, Stroke::new(1.0, line()));
                ui.add_space(4.0);
                egui::ScrollArea::vertical().id_salt(("clutter_items", i)).show(ui, |ui| {
                    for it in c.items.iter_mut().take(1000) {
                        ui.horizontal(|ui| {
                            ui.set_min_height(40.0);
                            checkbox(ui, &mut it.selected);
                            ui.label(RichText::new(kit::trunc_mid(&it.label, 34)).size(15.0)).on_hover_text(it.path.display().to_string());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_space(10.0);
                                let d = it.modified.map(kit::date_time).unwrap_or_default();
                                let d = d.replacen(" at ", " at\n", 1);
                                ui.add_sized([120.0, 34.0], egui::Label::new(RichText::new(d).size(12.5).color(dim())));
                                ui.add_sized([90.0, 20.0], egui::Label::new(b(fmt_size(it.size), 15.0)));
                            });
                        });
                    }
                });
            }
        }
    }

    // ---------- Clean browser ----------

    pub fn browsers_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.clean_job.is_some() {
            self.scanning(ui, "Cleaning...", false);
            return;
        }
        if self.browsers_job.is_some() {
            if self.scanning(ui, "Reading your browsers…", true) {
                self.browsers_job = None;
            }
            return;
        }
        if self.browsers.is_none() {
            let rows = [
                (Icon::Globe, "History", "Sites you visited, searches and downloads lists."),
                (Icon::Folder, "Caches", "Temporary files sites leave to load faster."),
                (Icon::Shield, "Cookies", "Small files sites use to remember you. Keep the ones you want."),
                (Icon::Check, "Seven browsers", "Safari, Chrome, Firefox, Opera, Edge, Brave and Vivaldi."),
            ];
            if intro(ui, "Browser data", "What your browsers remember about you, cleared in one go.", rows, "SCAN BROWSERS", self.clean_job.is_none(), Color32::from_rgb(0x8e, 0x5b, 0xe8)) {
                self.start_browsers(&ctx);
            }
            return;
        }
        if self.browsers.as_ref().unwrap().is_empty() {
            self.browsers = None;
            return self.nothing_to_clean();
        }
        results_heading(ui, &[("Here's what you can clean", ink())], false, "Cookies keep you signed in to websites, so they're not ticked. Open browsers are skipped when cleaning.");
        let list = self.browsers.as_ref().unwrap();
        let n: usize = list.iter().flat_map(|b| &b.cats).map(|c| c.selected_count()).sum();
        let sel: u64 = list.iter().flat_map(|b| &b.cats).map(|c| c.selected_size()).sum();
        let summary = if n == 0 { selected_parts(0, 0) } else { vec![(format!("{n} items selected ({})", fmt_size(sel)), ink())] };
        let mut preset = None;
        let clean = self.bottom_bar(ui, "browser_bar", |ui| preset = preset_combo(ui, "browser_pre"), summary, "CLEAN", n > 0);
        if let Some(how) = preset {
            for br in self.browsers.as_mut().unwrap() {
                apply_preset(&mut br.cats, how);
            }
        }
        if clean {
            self.confirm = Some(Confirm::Browsers);
        }
        self.fda_banner(ui);
        self.running_warning(ui);
        let mut toggle = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for br in self.browsers.as_mut().unwrap() {
                let total: usize = br.cats.iter().map(|c| c.items.len()).sum();
                let selected: usize = br.cats.iter().map(|c| c.selected_count()).sum();
                let size: u64 = br.cats.iter().map(|c| c.size()).sum();
                let open = self.browser_open.contains(br.name);
                ui.horizontal(|ui| {
                    ui.set_min_height(64.0);
                    ui.add_space(36.0);
                    if let Some(v) = tri_checkbox(ui, selected, total) {
                        br.cats.iter_mut().flat_map(|c| c.items.iter_mut()).for_each(|i| i.selected = v);
                    }
                    ui.add_space(12.0);
                    // A neutral badge instead of the browser's logo.
                    kit::badge(ui, br.name, 34.0);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.add_space(12.0);
                        ui.label(b(br.name, 16.0));
                        ui.label(muted(format!("{total} items ({})", fmt_size(size))));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(40.0);
                        if ui.add(egui::Button::new(if open { "⌃" } else { "⌄" }).frame(false)).clicked() || ui.link(RichText::new("Details").color(accent())).clicked() {
                            toggle = Some(br.name);
                        }
                    });
                });
                if open {
                    egui::Frame::new().fill(Color32::from_rgba_premultiplied(0x0a, 0x0b, 0x10, 0x14)).inner_margin(Margin::symmetric(80, 6)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for c in br.cats.iter_mut() {
                            ui.horizontal(|ui| {
                                ui.set_min_height(34.0);
                                if let Some(v) = tri_checkbox(ui, c.selected_count(), c.items.len()) {
                                    c.items.iter_mut().for_each(|i| i.selected = v);
                                }
                                ui.label(RichText::new(&c.title).size(15.0)).on_hover_text(c.tip);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(20.0);
                                    ui.add_sized([90.0, 20.0], egui::Label::new(b(fmt_size(c.size()), 15.0)));
                                    ui.add_sized([90.0, 20.0], egui::Label::new(muted(format!("{} items", c.items.len()))));
                                });
                            });
                        }
                    });
                }
                let r = ui.cursor();
                ui.painter().hline((r.left() + 30.0)..=r.right(), r.top(), Stroke::new(1.0, line()));
            }
        });
        if let Some(name) = toggle {
            if !self.browser_open.remove(name) {
                self.browser_open.insert(name);
            }
        }
    }

    // ---------- Find duplicates ----------

    pub fn dups_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.clean_job.is_some() {
            self.scanning(ui, "Cleaning...", false);
            return;
        }
        if self.dups_job.is_some() {
            if self.scanning(ui, "Comparing files…", true) {
                self.dups_job = None;
            }
            return;
        }
        if self.dups.is_none() && !self.dup_pick {
            let rows = [
                (Icon::Duplicates, "Byte-for-byte copies", "Files with 100% the same content, even with different names."),
                (Icon::Folder, "Keep one, free the rest", "Copies add up quietly."),
                (Icon::Check, "Your pick", "You choose which copies to keep."),
                (Icon::Speed, "Pick where to look", "Your home folder, other folders or external drives."),
            ];
            if intro(ui, "Duplicate files", "The same file in two places is one too many.", rows, "FIND DUPLICATES", true, Color32::from_rgb(0x2e, 0xa6, 0x6a)) {
                self.dup_pick = true;
            }
            return;
        }
        if self.dups.is_none() {
            return self.dup_locations(ui, &ctx);
        }
        if self.dups.as_ref().unwrap().is_empty() {
            self.dups = None;
            return self.nothing_to_clean();
        }
        let groups = self.dups.as_ref().unwrap();
        let wasted: u64 = groups.iter().map(|g| g.size * (g.files.len() as u64 - 1)).sum();
        let n = groups.iter().flat_map(|g| &g.files).filter(|f| f.selected).count();
        let sel: u64 = groups.iter().map(|g| g.size * g.files.iter().filter(|f| f.selected).count() as u64).sum();

        let back = egui::Rect::from_min_size(ui.cursor().min + vec2(0.0, 12.0), vec2(80.0, 24.0));
        if ui.new_child(egui::UiBuilder::new().max_rect(back)).link(RichText::new("← Back").size(15.0).color(accent())).clicked() {
            self.dups = None;
            return;
        }
        let size = fmt_size(wasted);
        results_heading(
            ui,
            &[("We've found ", ink()), (&size, size_color()), (" of duplicate files", ink())],
            true,
            "Duplicates have exactly the same content, even when their names differ. Files with the same name but different content are not duplicates.",
        );
        let mut pre = self.dup_preselect;
        let clean = self.bottom_bar(
            ui,
            "dups_bar",
            |ui| {
                egui::ComboBox::from_id_salt("dup_pre")
                    .selected_text(match pre {
                        Preselect::KeepNewest => "Keep newest",
                        Preselect::KeepOldest => "Keep oldest",
                        Preselect::None => "Select none",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut pre, Preselect::KeepNewest, "Keep newest");
                        ui.selectable_value(&mut pre, Preselect::KeepOldest, "Keep oldest");
                        ui.selectable_value(&mut pre, Preselect::None, "Select none");
                    });
            },
            selected_parts(sel, n),
            "CLEAN",
            n > 0,
        );
        if pre != self.dup_preselect {
            self.dup_preselect = pre;
            clean::preselect(self.dups.as_mut().unwrap(), pre);
        }
        if clean {
            self.confirm = Some(Confirm::Duplicates);
        }

        let avail = ui.available_size();
        let (left_w, right_w) = (190.0, 165.0);
        ui.horizontal_top(|ui| {
            // Left: All files and each kind.
            ui.allocate_ui_with_layout(vec2(left_w, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                let groups = self.dups.as_ref().unwrap();
                let kinds = [None, Some(Kind::Documents), Some(Kind::Pictures), Some(Kind::Songs), Some(Kind::Videos), Some(Kind::Others), Some(Kind::Folders)];
                for k in kinds {
                    let size: u64 = groups.iter().filter(|g| k.map_or(true, |k| g.kind == k)).map(|g| g.size * g.files.len() as u64).sum();
                    let name = match k {
                        None => "All files",
                        Some(Kind::Documents) => "Documents",
                        Some(Kind::Pictures) => "Pictures",
                        Some(Kind::Songs) => "Songs",
                        Some(Kind::Videos) => "Videos",
                        Some(Kind::Others) => "Others",
                        Some(Kind::Folders) => "Folders",
                    };
                    let active = self.dup_filter == k;
                    let (rect, resp) = ui.allocate_exact_size(vec2(left_w, if k.is_none() { 60.0 } else { 52.0 }), Sense::click());
                    if active {
                        ui.painter().rect_filled(rect, 0.0, accent());
                    }
                    let fg = if active { Color32::WHITE } else { ink() };
                    let indent = if k.is_none() { 14.0 } else { 30.0 };
                    if k.is_some() {
                        ui.painter().hline((rect.left() + 14.0)..=(rect.left() + 24.0), rect.center().y, Stroke::new(1.0, line()));
                    }
                    ui.painter().text(rect.left_center() + vec2(indent, -8.0), egui::Align2::LEFT_CENTER, name, egui::FontId::new(15.0, kit::bold_family()), fg);
                    let col = if active { Color32::from_white_alpha(210) } else { dim() };
                    ui.painter().text(rect.left_center() + vec2(indent, 10.0), egui::Align2::LEFT_CENTER, fmt_size(size), egui::FontId::proportional(13.0), col);
                    if !active {
                        ui.painter().text(rect.right_center() - vec2(12.0, 0.0), egui::Align2::CENTER_CENTER, "›", egui::FontId::proportional(20.0), dim());
                    }
                    if resp.clicked() {
                        self.dup_filter = k;
                    }
                }
            });
            let r = ui.cursor();
            ui.painter().vline(r.left(), r.top()..=r.top() + avail.y, Stroke::new(1.0, line()));
            ui.add_space(12.0);
            // Middle: copies grouped by name.
            let mid_w = ui.available_width() - right_w - 16.0;
            ui.allocate_ui_with_layout(vec2(mid_w, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.horizontal(|ui| {
                    if sort_combo(ui, "dup_sort", &mut self.sort) {
                        let groups = self.dups.as_mut().unwrap();
                        match self.sort {
                            SortBy::Size => groups.sort_by(|a, b| (b.size * b.files.len() as u64).cmp(&(a.size * a.files.len() as u64))),
                            SortBy::Name => groups.sort_by_key(|g| g.files[0].path.file_name().map(|n| n.to_string_lossy().to_lowercase())),
                            SortBy::Date => groups.sort_by(|a, b| b.files.iter().map(|f| f.modified).max().cmp(&a.files.iter().map(|f| f.modified).max())),
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_sized([90.0, 18.0], egui::Label::new(muted("Date Modified")));
                        ui.add_sized([70.0, 18.0], egui::Label::new(muted("Size")));
                    });
                });
                let filter = self.dup_filter;
                let mut preview = None;
                // One row per group name and per copy, all the same height, so only the rows on
                // screen are laid out (thousands of copies stay smooth).
                let groups = self.dups.as_mut().unwrap();
                let rows: Vec<(usize, Option<usize>)> = groups
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| filter.map_or(true, |k| g.kind == k))
                    .flat_map(|(gi, g)| std::iter::once((gi, None)).chain((0..g.files.len()).map(move |fi| (gi, Some(fi)))))
                    .collect();
                let row_h = 30.0;
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::ScrollArea::vertical().id_salt("dup_groups").show_rows(ui, row_h, rows.len(), |ui, range| {
                    for &(gi, fi) in &rows[range] {
                        let g = &mut groups[gi];
                        let size = g.size;
                        ui.allocate_ui_with_layout(vec2(ui.available_width(), row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_min_height(row_h);
                            match fi {
                                None => {
                                    let name = g.files[0].path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                    let r = ui.max_rect();
                                    ui.painter().hline(r.x_range(), r.top() + 1.0, Stroke::new(1.0, line()));
                                    ui.label(muted("⋮"));
                                    ui.label(b(kit::trunc_mid(&name, 44), 15.0));
                                }
                                Some(fi) => {
                                    let f = &mut g.files[fi];
                                    checkbox(ui, &mut f.selected);
                                    let path = f.path.display().to_string();
                                    if ui.add(egui::Label::new(RichText::new(kit::trunc_end(&path, 40)).size(13.5)).sense(Sense::click())).on_hover_text(&path).clicked() {
                                        preview = Some(f.path.clone());
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.add_sized([90.0, 18.0], egui::Label::new(RichText::new(kit::date_only(f.modified)).size(13.0)));
                                        ui.add_sized([70.0, 18.0], egui::Label::new(b(fmt_size(size), 13.5)));
                                    });
                                }
                            }
                        });
                    }
                });
                if preview.is_some() {
                    self.dup_preview = preview;
                }
            });
            // Right: preview of the clicked copy.
            ui.allocate_ui_with_layout(vec2(right_w, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| self.dup_preview_ui(ui));
        });
    }

    fn dup_preview_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(muted("Preview"));
        ui.add_space(6.0);
        let Some(p) = self.dup_preview.clone() else {
            ui.label(RichText::new("Click a file to see its details.").size(13.0).color(dim()));
            return;
        };
        let kind = clean::kind_of(&p);
        let ic = match kind {
            Kind::Pictures => Icon::Duplicates,
            Kind::Songs | Kind::Videos => Icon::Monitor,
            _ => Icon::Folder,
        };
        kit::icon_box(ui, ic, 88.0);
        ui.add_space(6.0);
        ui.label(b(kit::trunc_mid(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), 24), 14.0));
        ui.separator();
        let meta = std::fs::metadata(&p).ok();
        for (label, t) in [("Date Created", meta.as_ref().and_then(|m| m.created().ok())), ("Date Modified", meta.as_ref().and_then(|m| m.modified().ok()))] {
            ui.label(RichText::new(label).size(12.5).color(dim()));
            ui.label(RichText::new(t.map(kit::date_time).unwrap_or("—".into())).size(13.0));
            ui.add_space(4.0);
        }
        let show = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in folder" };
        if ui.link(RichText::new(show).color(accent())).clicked() {
            reveal(&p);
        }
    }

    /// Picking the folders to compare..
    fn dup_locations(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        kit::heading(ui, "Pick the folders to compare", "More folders means a longer scan.");
        let checked: Vec<std::path::PathBuf> = self.saved.dup_folders.iter().filter(|f| !self.saved.dup_off.contains(f)).cloned().collect();
        let n = checked.len();
        egui::Panel::bottom("dup_loc_bar").frame(egui::Frame::new().inner_margin(Margin::symmetric(0, 8))).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(b(if n == 0 { "Nothing selected".to_string() } else { format!("{n} location(s) selected") }, 16.0));
                ui.add_space(8.0);
                if primary(ui, "SCAN", n > 0) {
                    self.start_dups(ctx);
                }
            });
        });
        ui.horizontal(|ui| {
            ui.add_space(60.0);
            ui.add_sized([150.0, 16.0], egui::Label::new(RichText::new("NAME").family(kit::bold_family()).size(11.5)));
            ui.label(RichText::new("LOCATION").family(kit::bold_family()).size(11.5));
        });
        let mut remove = None;
        egui::Frame::new()
            .stroke(Stroke::new(1.0, kit::dim()))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(200.0);
                for (i, f) in self.saved.dup_folders.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let mut on = !self.saved.dup_off.contains(f);
                        if kit::check(ui, on, false, true) {
                            on = !on;
                            if on {
                                self.saved.dup_off.retain(|x| x != f);
                            } else {
                                self.saved.dup_off.push(f.clone());
                            }
                        }
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                        kit::icon(ui.painter(), r, Icon::Folder, accent());
                        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| f.display().to_string());
                        ui.add_sized([130.0, 18.0], egui::Label::new(b(kit::trunc_end(&name, 18), 14.5)));
                        ui.label(RichText::new(f.display().to_string()).size(14.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("×").size(20.0).color(dim())).frame(false)).on_hover_text("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                }
            });
        if let Some(i) = remove {
            let f = self.saved.dup_folders.remove(i);
            self.saved.dup_off.retain(|x| x != &f);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if kit::outline(ui, "ADD") {
                if let Some(f) = rfd::FileDialog::new().pick_folder() {
                    if !self.saved.dup_folders.contains(&f) {
                        self.saved.dup_folders.push(f);
                    }
                }
            }
            ui.label(RichText::new("Add a folder or a drive.").size(14.0));
        });
        let mins = clean::fmt_size(clean::DUP_SIZES[self.saved.settings.dup_min_idx.min(4)]);
        ui.label(RichText::new(format!("Files smaller than {mins} and app data folders are skipped (change it in Settings).")).size(12.5).color(dim()));
    }
}

/// The bottom-left selection preset: 0 recommended, 1 all, 2 none.
fn preset_combo(ui: &mut egui::Ui, id: &str) -> Option<u8> {
    let mut pick = None;
    egui::ComboBox::from_id_salt(id).selected_text("Suggested").width(190.0).show_ui(ui, |ui| {
        for (i, t) in ["Suggested", "Select all", "Select none"].iter().enumerate() {
            if ui.selectable_label(false, *t).clicked() {
                pick = Some(i as u8);
            }
        }
    });
    pick
}

pub(crate) fn apply_preset(cats: &mut [Category], how: u8) {
    for c in cats {
        let on = match how {
            0 => recommended(c),
            1 => true,
            _ => false,
        };
        c.items.iter_mut().for_each(|i| i.selected = on);
    }
}
