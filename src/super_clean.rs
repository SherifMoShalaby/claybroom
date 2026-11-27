//! Super Clean: Claybroom's own addition. One scan runs Quick
//! Clean, Clean clutter, Clean browser, Find duplicates and Analyze photos, and suggests unused
//! apps to uninstall and startup items to turn off. Results come in two lanes: "Safe to clean
//! now" (regenerable junk, ticked like Quick Clean's recommended selection) and "Your call"
//! (personal files, apps and startup items, never ticked for you). Each lane's full list is the
//! existing screen for it, filled with the same results, so "See all" shows the same selection.

use crate::apps::{self, App, StartItem};
use crate::clean::{self, fmt_size, Category, DupGroup, Preselect, Progress, Settings};
use crate::kit::{self, b, dim, ink, muted, size_color, accent, Icon};
use crate::photos::{self, PhotoResults};
use crate::screens::{mixed, results_heading, selected_parts};
use crate::{intro, Confirm, Page, Sweep};
use eframe::egui::{self, vec2, Color32, RichText, Sense, Vec2};
use std::path::PathBuf;
use std::time::SystemTime;

/// The steps of the scan, in order.
pub const STEPS: [(Icon, &str); 5] = [
    (Icon::Clutter, "Junk & browsers"),
    (Icon::Duplicates, "Duplicates"),
    (Icon::Picture, "Photos"),
    (Icon::Trash, "Apps"),
    (Icon::Startup, "Startup"),
];

/// The "Your call" suggestions, summed and ranked once when the scan finishes (the results page
/// redraws many times a second; matching thousands of files on every redraw made it crawl).
#[derive(Default)]
pub struct Picks {
    dup_n: usize,
    dup_total: u64,
    dup_top: Vec<(PathBuf, u64)>,
    photo_n: usize,
    photo_total: u64,
    photo_top: Vec<(PathBuf, u64)>,
}

/// How many, how big in all, and the three biggest.
fn rank(items: Vec<(PathBuf, u64)>) -> (usize, u64, Vec<(PathBuf, u64)>) {
    let (n, total) = (items.len(), items.iter().map(|x| x.1).sum());
    let mut top = items;
    top.sort_by_key(|x| std::cmp::Reverse(x.1));
    top.truncate(3);
    (n, total, top)
}

/// What one Super Clean scan found. Suggestions are the files the duplicate finder and photo
/// analyzer would have ticked; here they start unticked.
pub struct Found {
    junk: Vec<Category>,
    dups: Vec<DupGroup>,
    dup_picks: Vec<PathBuf>,
    photos: PhotoResults,
    photo_picks: Vec<PathBuf>,
    apps: Vec<App>,
    startup: Vec<StartItem>,
}

/// A step finished: its ring is full and it says what it found.
fn finish(p: &Progress, note: String) {
    let mut st = p.lock().unwrap();
    st.notes = vec![note];
    st.frac = 1.0;
    st.text.clear();
}

/// Run all five steps at once, each on its own thread with its own progress (`ps`, one per step,
/// in STEPS order); the slowest step sets how long the scan takes.
pub fn scan(s: &Settings, dup_folders: &[PathBuf], ctx: &egui::Context, ps: &[Progress]) -> Found {
    std::thread::scope(|sc| {
        let junk = sc.spawn(|| {
            let mut junk = clean::scan_junk(s, &ps[0], true);
            crate::screens::apply_preset(&mut junk, 0);
            junk.retain(|c| !c.items.is_empty());
            let size: u64 = junk.iter().flat_map(|c| &c.items).map(|i| i.size).sum();
            finish(&ps[0], format!("{} of junk", fmt_size(size)));
            junk
        });
        let dups = sc.spawn(|| {
            let mut dups = clean::scan_duplicates(dup_folders, s, &ps[1]);
            clean::preselect(&mut dups, Preselect::KeepNewest);
            let picks: Vec<PathBuf> = dups.iter().flat_map(|g| &g.files).filter(|f| f.selected).map(|f| f.path.clone()).collect();
            clean::preselect(&mut dups, Preselect::None);
            finish(&ps[1], format!("{} extra copies", picks.len()));
            (dups, picks)
        });
        let photos = sc.spawn(|| {
            let mut photos = photos::scan(&photos::roots(s), &ps[2]);
            let picks: Vec<PathBuf> = photos.similar.iter().flatten().filter(|x| x.selected).chain(photos.bad.iter()).map(|x| x.path.clone()).collect();
            photos.all_mut().for_each(|x| x.selected = false);
            finish(&ps[2], format!("{} photos to look at", picks.len()));
            (photos, picks)
        });
        let apps = sc.spawn(|| {
            let mut list = apps::scan_apps(&ps[3]);
            clean::say(&ps[3], "Measuring apps");
            apps::measure_now(&mut list, ctx.clone());
            let unused = list.iter().filter(|a| is_unused(a, s)).count();
            finish(&ps[3], format!("{unused} unused apps"));
            list
        });
        let startup = sc.spawn(|| {
            let startup = apps::scan_startup();
            let n = startup.iter().filter(|i| can_stop(i)).count();
            finish(&ps[4], format!("{n} could stop"));
            startup
        });
        let (dups, dup_picks) = dups.join().unwrap_or_default();
        let (photos, photo_picks) = photos.join().unwrap_or_else(|_| (PhotoResults { similar: vec![], bad: vec![] }, vec![]));
        Found {
            junk: junk.join().unwrap_or_default(),
            dups,
            dup_picks,
            photos,
            photo_picks,
            apps: apps.join().unwrap_or_default(),
            startup: startup.join().unwrap_or_default(),
        }
    })
}

fn is_unused(a: &App, s: &Settings) -> bool {
    let days = clean::UNUSED_DAYS[s.unused_idx.min(3)].0;
    a.can_uninstall() && a.last_used.is_some_and(|t| SystemTime::now().duration_since(t).is_ok_and(|d| d.as_secs() > days * 86400))
}

/// Startup items worth suggesting: on now, and changeable without an administrator.
fn can_stop(i: &StartItem) -> bool {
    i.on && !i.system
}

/// A "Your call" section header; returns true when "See all" is clicked.
fn section(ui: &mut egui::Ui, ic: Icon, title: &str, sub: &str, has_more: bool) -> bool {
    let mut all = false;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
        kit::icon(ui.painter(), r, ic, ink());
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(b(title, 16.0));
            ui.label(RichText::new(sub).size(13.0).color(dim()));
        });
        if has_more {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                all = ui.link(RichText::new("See all ›").color(accent())).clicked();
            });
        }
    });
    all
}

/// One suggestion: a tick box, a name, a note and a size on the right.
fn pick_row(ui: &mut egui::Ui, on: &mut bool, name: &str, note: &str, size: Option<u64>) {
    ui.horizontal(|ui| {
        ui.add_space(30.0);
        if kit::check(ui, *on, false, true) {
            *on = !*on;
        }
        if ui.add(egui::Label::new(RichText::new(kit::trunc_end(name, 30)).size(14.0)).sense(Sense::click())).clicked() {
            *on = !*on;
        }
        ui.label(RichText::new(note).size(12.5).color(dim()));
        if let Some(sz) = size {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(b(fmt_size(sz), 13.5).color(if *on { size_color() } else { ink() }));
            });
        }
    });
}

fn more(ui: &mut egui::Ui, n: usize) {
    if n > 3 {
        ui.horizontal(|ui| {
            ui.add_space(30.0);
            ui.label(RichText::new(format!("and {} more", n - 3)).size(12.5).color(dim()));
        });
    }
}

impl Sweep {
    pub fn start_super(&mut self, ctx: &egui::Context) {
        clean::reset(&self.progress);
        self.done = None;
        self.photo_tex.clear();
        self.super_stop.clear();
        let f: Vec<PathBuf> = self.saved.dup_folders.iter().filter(|f| !self.saved.dup_off.contains(f)).cloned().collect();
        self.super_steps = STEPS.iter().map(|_| Progress::default()).collect();
        let (s, ps, c) = (self.saved.settings.clone(), self.super_steps.clone(), ctx.clone());
        self.super_job = Some(crate::spawn(ctx, move || scan(&s, &f, &c, &ps)));
        self.page = Page::Super;
    }

    /// The scan finished: its results fill the screens they belong to.
    pub fn super_found(&mut self, f: Found) {
        self.quick = Some(f.junk);
        self.dups = Some(f.dups);
        self.photos = Some(f.photos);
        self.apps = Some(f.apps);
        self.apps_sizes = None;
        self.startup = Some(f.startup);
        let dup_size: std::collections::HashMap<&PathBuf, u64> = self.dups.iter().flatten().flat_map(|g| g.files.iter().map(move |x| (&x.path, g.size))).collect();
        let photo_size: std::collections::HashMap<&PathBuf, u64> = self.photos.iter().flat_map(|r| r.all()).map(|x| (&x.path, x.size)).collect();
        let (dup_n, dup_total, dup_top) = rank(f.dup_picks.iter().map(|p| (p.clone(), dup_size.get(p).copied().unwrap_or(0))).collect());
        let (photo_n, photo_total, photo_top) = rank(f.photo_picks.iter().map(|p| (p.clone(), photo_size.get(p).copied().unwrap_or(0))).collect());
        self.super_picks = Picks { dup_n, dup_total, dup_top, photo_n, photo_total, photo_top };
        self.super_ready = true;
    }

    /// What's ticked across both lanes: bytes and count.
    fn super_selected(&self) -> (u64, usize) {
        let junk = self.quick.iter().flatten().flat_map(|c| &c.items).filter(|i| i.selected);
        let dups = self.dups.iter().flatten().flat_map(|g| g.files.iter().map(move |f| (f, g.size))).filter(|(f, _)| f.selected);
        let photos = self.photos.iter().flat_map(|r| r.all()).filter(|x| x.selected);
        let apps = self.apps.iter().flatten().filter(|a| a.selected);
        let mut size = 0;
        let mut n = self.super_stop.len();
        for i in junk {
            size += i.size;
            n += 1;
        }
        for (_, s) in dups {
            size += s;
            n += 1;
        }
        for x in photos {
            size += x.size;
            n += 1;
        }
        for a in apps {
            size += a.total();
            n += 1;
        }
        (size, n)
    }

    pub fn super_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.clean_job.is_some() {
            self.scanning(ui, "Cleaning...", false);
            return;
        }
        if self.super_job.is_some() {
            if self.super_progress(ui) {
                self.super_job = None;
            }
            return;
        }
        if !self.super_ready || self.quick.is_none() {
            let rows = [
                (Icon::Clutter, "Everything in one scan", "Junk, browser data, duplicates, photos, apps and startup items."),
                (Icon::Check, "Safe things first", "Only files apps make again are ticked for you."),
                (Icon::Picture, "Your files stay yours", "Duplicates and photos are suggestions you tick yourself."),
                (Icon::Speed, "Lighter and faster", "See unused apps and startup items you could stop."),
            ];
            if intro(ui, "Super Clean", "Every check Claybroom does, in one go.", rows, "START SUPER CLEAN", true, accent()) {
                self.start_super(&ctx);
            }
            return;
        }

        let s = self.saved.settings.clone();
        let pk = &self.super_picks;
        let (dup_n, dup_total, dup_top) = (pk.dup_n, pk.dup_total, pk.dup_top.clone());
        let (photo_n, photo_total, photo_top) = (pk.photo_n, pk.photo_total, pk.photo_top.clone());
        let unused_label = clean::UNUSED_DAYS[s.unused_idx.min(3)].1;
        // Everything found, for the heading.
        let junk_total: u64 = self.quick.iter().flatten().flat_map(|c| &c.items).map(|i| i.size).sum();
        let unused: Vec<usize> = self.apps.iter().flatten().enumerate().filter(|(_, a)| is_unused(a, &s)).map(|(i, _)| i).collect();
        let app_total: u64 = unused.iter().filter_map(|&i| self.apps.as_ref()?.get(i)).map(|a| a.total()).sum();
        let stoppable: Vec<usize> = self.startup.iter().flatten().enumerate().filter(|(_, i)| can_stop(i)).map(|(i, _)| i).collect();
        let found = fmt_size(junk_total + dup_total + photo_total + app_total);
        results_heading(
            ui,
            &[("Super Clean found ", ink()), (&found, size_color())],
            false,
            "Left: files apps and browsers make again, ticked like Quick Clean. Right: suggestions about your own files, apps and startup items. Nothing on the right is ticked for you.",
        );

        let (size, n) = self.super_selected();
        let summary = if n == 0 { selected_parts(0, 0) } else { vec![(fmt_size(size), size_color()), (format!(" selected · {n} item(s)"), ink())] };
        if self.bottom_bar(ui, "super_bar", |_| {}, summary, "CLEAN SELECTED", n > 0) {
            self.confirm = Some(Confirm::Super);
        }

        let mut go: Option<Page> = None;
        let h = ui.available_height();
        let left_w = (ui.available_width() * 0.42).max(300.0);
        ui.horizontal_top(|ui| {
            // Left lane: safe to clean now.
            ui.allocate_ui_with_layout(vec2(left_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                kit::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                        kit::icon(ui.painter(), r, Icon::Check, kit::good());
                        ui.label(b("Safe to clean now", 17.0));
                    });
                    ui.label(muted("Apps and browsers make these again when they need them."));
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().id_salt("super_safe").max_height(h - 130.0).show(ui, |ui| {
                        for c in self.quick.iter_mut().flatten() {
                            let sel = c.items.iter().filter(|i| i.selected).count();
                            let (all, some) = (sel == c.items.len(), sel > 0);
                            ui.horizontal(|ui| {
                                ui.set_min_height(30.0);
                                if kit::check(ui, all, some && !all, true) {
                                    c.items.iter_mut().for_each(|i| i.selected = !all);
                                }
                                ui.label(RichText::new(&c.title).size(14.5)).on_hover_text(c.tip);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let bytes: u64 = c.items.iter().map(|i| i.size).sum();
                                    ui.label(b(fmt_size(bytes), 14.0).color(if some { size_color() } else { dim() }));
                                });
                            });
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Cookies, Downloads and large files wait for you to tick them.").size(12.5).color(dim()));
                    });
                    if ui.link(RichText::new("See every file ›").color(accent())).clicked() {
                        go = Some(Page::Quick);
                    }
                });
            });
            ui.add_space(12.0);
            // Right lane: your call.
            ui.allocate_ui_with_layout(vec2(ui.available_width(), h), egui::Layout::top_down(egui::Align::Min), |ui| {
                kit::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                        kit::icon(ui.painter(), r, Icon::Sparkle, accent());
                        ui.label(b("Your call", 17.0));
                    });
                    ui.label(muted("Suggestions about your own files, apps and startup. Tick what you want gone."));
                    egui::ScrollArea::vertical().id_salt("super_call").max_height(h - 70.0).show(ui, |ui| {
                        // Duplicates
                        let sub = if dup_n == 0 { "No copies".to_string() } else { format!("{} extra copies · {}", dup_n, fmt_size(dup_total)) };
                        if section(ui, Icon::Duplicates, "Duplicates", &sub, !dup_n == 0) {
                            go = Some(Page::Duplicates);
                        }
                        for (path, sz) in &dup_top {
                            let (path, sz) = (path.clone(), *sz);
                            if let Some(f) = self.dups.iter_mut().flatten().flat_map(|g| g.files.iter_mut()).find(|f| f.path == path) {
                                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                pick_row(ui, &mut f.selected, &name, "copy", Some(sz));
                            }
                        }
                        more(ui, dup_n);

                        // Photos
                        let sub = if photo_n == 0 { "No bad or look-alike photos".to_string() } else { format!("{} photos · {}", photo_n, fmt_size(photo_total)) };
                        if section(ui, Icon::Picture, "Bad and look-alike photos", &sub, !photo_n == 0) {
                            go = Some(Page::Photos);
                        }
                        for (path, _) in &photo_top {
                            let path = path.clone();
                            if let Some(x) = self.photos.as_mut().and_then(|r| r.all_mut().find(|x| x.path == path)) {
                                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                let note = if x.reason.is_empty() { "look-alike" } else { x.reason };
                                let sz = x.size;
                                pick_row(ui, &mut x.selected, &name, note, Some(sz));
                            }
                        }
                        more(ui, photo_n);

                        // Unused apps
                        let sub = if unused.is_empty() { format!("None unopened for {unused_label}") } else { format!("{} not opened for {unused_label} or more · {}", unused.len(), fmt_size(app_total)) };
                        if section(ui, Icon::Trash, "Apps you don't use", &sub, !unused.is_empty()) {
                            self.app_tab = crate::AppTab::Unused;
                            go = Some(Page::Uninstaller);
                        }
                        let mut top = unused.clone();
                        if let Some(list) = self.apps.as_ref() {
                            top.sort_by_key(|&i| std::cmp::Reverse(list[i].total()));
                        }
                        for i in top.into_iter().take(3) {
                            if let Some(a) = self.apps.as_mut().and_then(|l| l.get_mut(i)) {
                                let note = a.last_used.map(|t| format!("last used {}", kit::date_only(t))).unwrap_or_default();
                                let (name, sz) = (a.name.clone(), a.total());
                                pick_row(ui, &mut a.selected, &name, &note, Some(sz));
                            }
                        }
                        more(ui, unused.len());

                        // Startup
                        let sub = if stoppable.is_empty() { "Nothing you can turn off".to_string() } else { format!("{} open when you log in; turning one off doesn't delete it", stoppable.len()) };
                        if section(ui, Icon::Startup, "Startup items you could stop", &sub, !stoppable.is_empty()) {
                            go = Some(Page::Startup);
                        }
                        for i in stoppable.iter().take(3) {
                            if let Some(it) = self.startup.as_ref().and_then(|l| l.get(*i)) {
                                let (name, loc, kind) = (it.name.clone(), it.location.clone(), it.kind.label());
                                let mut on = self.super_stop.contains(&loc);
                                let was = on;
                                pick_row(ui, &mut on, &name, kind, None);
                                if on != was {
                                    if on {
                                        self.super_stop.insert(loc);
                                    } else {
                                        self.super_stop.remove(&loc);
                                    }
                                }
                            }
                        }
                        more(ui, stoppable.len());
                    });
                });
            });
        });
        if let Some(p) = go {
            self.page = p;
        }
    }

    /// The scan's progress: the ring for the whole scan and the five steps under it.
    fn super_progress(&mut self, ui: &mut egui::Ui) -> bool {
        let steps: Vec<clean::Status> = self.super_steps.iter().map(|p| p.lock().unwrap().clone()).collect();
        let time = ui.input(|i| i.time);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        let frac = steps.iter().map(|s| s.frac).sum::<f32>() / STEPS.len().max(1) as f32;
        let found: u64 = steps.first().map_or(0, |s| s.found);
        // The step still running that says the most, for the line under the ring.
        let text = steps.iter().find(|s| s.notes.is_empty() && !s.text.is_empty()).map(|s| s.text.clone()).unwrap_or_default();
        let mut cancel = false;
        ui.vertical_centered(|ui| {
            ui.add_space(16.0);
            ui.label(b("Super Clean is checking everything", 30.0));
            ui.label(RichText::new("All five checks run at the same time.").size(15.0).color(dim()));
            ui.add_space(18.0);
            kit::progress_ring(ui, frac, time);
            ui.add_space(10.0);
            ui.label(RichText::new(kit::trunc_mid(&text, 70)).size(15.0).color(dim()));
            ui.add_space(18.0);
            // The five steps, side by side: done ones say what they found.
            let w = 150.0;
            let (r, _) = ui.allocate_exact_size(vec2(w * STEPS.len() as f32, 62.0), Sense::hover());
            let p = ui.painter();
            for (k, (ic, name)) in STEPS.iter().enumerate() {
                let x = r.left() + w * (k as f32 + 0.5);
                let st = &steps[k];
                let col = ink();
                kit::icon(p, egui::Rect::from_center_size(egui::pos2(x, r.top() + 12.0), Vec2::splat(22.0)), *ic, col);
                p.text(egui::pos2(x, r.top() + 34.0), egui::Align2::CENTER_CENTER, *name, egui::FontId::new(14.0, kit::bold_family()), col);
                let (note, nc) = match st.notes.first() {
                    Some(n) => (format!("✓ {n}"), kit::good()),
                    None => (format!("checking… {:.0}%", st.frac * 100.0), accent()),
                };
                p.text(egui::pos2(x, r.top() + 54.0), egui::Align2::CENTER_CENTER, note, egui::FontId::proportional(12.5), nc);
            }
            ui.add_space(16.0);
            if found > 0 {
                ui.label(mixed(&[(&fmt_size(found), accent()), (" of junk found so far", ink())], 15.0));
            }
            ui.add_space(8.0);
            cancel = kit::outline(ui, "CANCEL");
        });
        cancel
    }
}

/// "Super Clean · NEW": the highlighted card on the Overview.
pub fn home_card(ui: &mut egui::Ui, enabled: bool) -> bool {
    let mut go = false;
    egui::Frame::new()
        .fill(kit::tile_fill())
        .stroke(egui::Stroke::new(1.5, accent().gamma_multiply(0.7)))
        .corner_radius(egui::CornerRadius::same(26))
        .inner_margin(egui::Margin::symmetric(26, 20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
                kit::icon(ui.painter(), r, Icon::Sparkle, accent());
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(b("Super Clean", 19.0));
                        egui::Frame::new().fill(accent()).corner_radius(egui::CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| {
                            ui.label(RichText::new("NEW").family(kit::bold_family()).size(11.0).color(Color32::WHITE));
                        });
                    });
                    ui.label(RichText::new("Everything in one scan").size(15.5).color(dim()));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    go = kit::primary(ui, "SCAN", enabled);
                });
            });
        });
    go
}

#[cfg(test)]
mod timing {
    /// Read-only: how long each Super Clean step takes on this computer, all at once.
    /// `cargo test super_steps -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn super_steps_timed() {
        let s = crate::clean::Settings::default();
        let ps: Vec<crate::clean::Progress> = super::STEPS.iter().map(|_| Default::default()).collect();
        let watch = ps.clone();
        let t0 = std::time::Instant::now();
        let w = std::thread::spawn(move || {
            let mut done = vec![false; watch.len()];
            while done.iter().any(|d| !d) {
                for (k, p) in watch.iter().enumerate() {
                    if !done[k] && !p.lock().unwrap().notes.is_empty() {
                        done[k] = true;
                        println!("{:>16}: {:>6.1} s  {}", super::STEPS[k].1, t0.elapsed().as_secs_f32(), p.lock().unwrap().notes[0]);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        });
        let f = super::scan(&s, &[crate::clean::home()], &eframe::egui::Context::default(), &ps);
        w.join().unwrap();
        println!("all done in {:.1} s ({} junk categories)", t0.elapsed().as_secs_f32(), f.junk.len());
    }
}
