// No console window behind the app on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

//! Claybroom: a free, open-source disk cleaner for macOS, Windows and Linux.

mod apps;
mod browser_pro;
mod schedule;
mod clean;
mod kit;
mod illus;
mod mechanics;
mod panel;
mod tray;
mod update;
mod super_clean;
mod shell;
mod tour;
mod sensors;

/// While a scan or clean runs, tell macOS it's work the person started, so a hidden or covered
/// window isn't slowed down by App Nap (measured: a hidden window's Super Clean dropped to
/// priority 4 and hadn't finished after 30 minutes; the same scan takes 2 minutes unthrottled).
#[cfg(target_os = "macos")]
mod awake {
    use objc2::rc::Retained;
    use objc2::runtime::{NSObjectProtocol, ProtocolObject};
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

    pub struct Token(Retained<ProtocolObject<dyn NSObjectProtocol>>);

    pub fn begin() -> Token {
        Token(NSProcessInfo::processInfo().beginActivityWithOptions_reason(NSActivityOptions::UserInitiated, &NSString::from_str("Scanning or cleaning")))
    }

    impl Drop for Token {
        fn drop(&mut self) {
            // SAFETY: the token is the object beginActivity returned.
            unsafe { NSProcessInfo::processInfo().endActivity(&self.0) }
        }
    }
}
#[cfg(not(target_os = "macos"))]
mod awake {
    pub struct Token;
    pub fn begin() -> Token {
        Token
    }
}
mod photos;
mod tabs;
mod screens;

/// Window size in points.
const WINDOW: [f32; 2] = [1006.0, 660.0];

use clean::{fmt_size, Browser, Category, CleanReport, DupGroup, Kind, Preselect, Progress, Settings};
use eframe::egui::{self, vec2, Color32, CornerRadius, Margin, RichText, Vec2};
use kit::{b, card, heading, muted, primary, tile, Icon, dim, accent, rail, rail_active, ink};
use screens::{DoneScreen, SortBy};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    // `--mechanics <file.json>`: measure what this build actually does and write it down, no window.
    if let Some(out) = args.iter().position(|a| a == "--mechanics").and_then(|i| args.get(i + 1)) {
        let json = serde_json::to_string_pretty(&mechanics::measure()).unwrap();
        std::fs::write(out, json + "\n").expect("write the measurement");
        return Ok(());
    }
    // `--scheduled-clean`: what the hourly background job runs (Settings › Browser Cleaning, Trash).
    if args.iter().any(|a| a == "--scheduled-clean") {
        schedule::run_scheduled();
        return Ok(());
    }
    // `--write-icon <file.png> <size>`: save the app icon (used to make the macOS .icns).
    if let Some(i) = args.iter().position(|a| a == "--write-icon") {
        let out = args.get(i + 1).expect("--write-icon <file.png> <size>");
        let size: u32 = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(1024);
        image::RgbaImage::from_raw(size, size, kit::app_icon(size)).unwrap().save(out).expect("write the icon");
        return Ok(());
    }
    let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    // `--page clutter` opens straight on a screen (handy for screenshots of each one).
    let start = args.iter().position(|a| a == "--page").and_then(|i| args.get(i + 1)).cloned().unwrap_or_default();
    // `--scan` also starts that screen's scan; with --screenshot the picture waits for the results
    // unless `--during` asks for the scanning screen itself.
    let auto = (args.iter().any(|a| a == "--scan"), args.iter().any(|a| a == "--during"));

    // `--page tray` opens as the menu-bar panel (for pictures of it).
    let as_panel = start == "tray";
    let mut vp = egui::ViewportBuilder::default()
        .with_title("Claybroom")
        .with_inner_size(if as_panel { panel::PANEL } else { WINDOW })
        .with_decorations(!as_panel)
        .with_icon(egui::IconData { rgba: kit::app_icon(256), width: 256, height: 256 })
        .with_min_inner_size(if as_panel { [300.0, 400.0] } else { [900.0, 600.0] });
    let mut opts = eframe::NativeOptions::default();
    // The window turns into the menu-bar panel and back, so its size isn't remembered.
    opts.persist_window = false;
    if shot.is_some() {
        // A check is taking a picture: don't steal focus from the person.
        vp = vp.with_active(false);
        #[cfg(windows)]
        {
            vp = vp.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
        #[cfg(target_os = "macos")]
        {
            opts.event_loop_builder = Some(Box::new(|b| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                b.with_activate_ignoring_other_apps(false);
            }));
        }
        opts.persist_window = false;
    }
    opts.viewport = vp;
    eframe::run_native("Claybroom", opts, Box::new(move |cc| Ok(Box::new(Sweep::new(cc, shot, &start, auto)))))
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Quick,
    Clutter,
    Browsers,
    Duplicates,
    Photos,
    Uninstaller,
    Startup,
    Monitor,
    Stats,
    Settings,
    /// Claybroom's own: every scan at once.
    Super,
}

const PAGES: [(Page, Icon, &str); 11] = [
    (Page::Home, Icon::Home, "OVERVIEW"),
    (Page::Quick, Icon::Quick, "QUICK CLEAN"),
    // Claybroom's own; its results stay here while other pages are open.
    (Page::Super, Icon::Sparkle, "SUPER CLEAN"),
    (Page::Clutter, Icon::Clutter, "CLEAN CLUTTER"),
    (Page::Browsers, Icon::Globe, "CLEAN BROWSER"),
    (Page::Duplicates, Icon::Duplicates, "FIND DUPLICATES"),
    (Page::Photos, Icon::Picture, "ANALYZE PHOTOS"),
    (Page::Uninstaller, Icon::Trash, "UNINSTALL APPS"),
    (Page::Startup, Icon::Startup, "MANAGE STARTUPS"),
    (Page::Monitor, Icon::Monitor, "SYSTEM MONITOR"),
    (Page::Stats, Icon::Stats, "STATISTICS"),
];

/// One cleaning, for the Statistics page.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Entry {
    at: u64,
    key: String,
    bytes: u64,
    items: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct Saved {
    settings: Settings,
    dup_folders: Vec<PathBuf>,
    /// Locations in the list that are unticked.
    dup_off: Vec<PathBuf>,
    log: Vec<Entry>,
    /// Size of the last clutter scan, for Settings › Alerts.
    last_clutter_total: u64,
    /// First-run tour stage (tour::FDA … tour::DONE).
    tour: u8,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Which Statistics row a cleaned category counts towards.
fn stat_key(title: &str, browser: bool) -> &'static str {
    let t = title.to_lowercase();
    match () {
        _ if browser && t.contains("cookies") => "Cookies",
        _ if browser && t.contains("cache") => "Browser caches",
        _ if browser => "Browsing history",
        _ if t == "app caches" => "App caches",
        _ if t == "log files" => "Log files",
        _ if t == "trash" => "Trash",
        _ if t == "downloads" => "Downloads",
        _ if t.starts_with("large files") => "Large files",
        _ => "Other",
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AppTab {
    All,
    Large,
    Unused,
}

#[derive(Clone, Copy, PartialEq)]
enum AppSort {
    Name,
    Size,
    LastUsed,
}

#[derive(Clone, Copy, PartialEq)]
enum StartSort {
    Name,
    Status,
    Type,
}

enum Confirm {
    Quick,
    Clutter,
    Browsers,
    Duplicates,
    Apps,
    Photos,
    ResetStats,
    RemoveStartup(usize),
    Super,
    SkipFda,
}

enum Done {
    Quick(CleanReport),
    Clutter(CleanReport),
    Browsers(CleanReport),
    Duplicates(CleanReport),
    Apps(usize, Vec<String>),
    Photos(CleanReport),
    Super { junk: CleanReport, dups: CleanReport, photos: CleanReport, apps: usize, stopped: usize, errs: Vec<String> },
    /// System items removed by an administrator, or why not.
    Admin(CleanReport),
}

struct Sweep {
    page: Page,
    saved: Saved,
    progress: Progress,
    quick: Option<Vec<Category>>,
    quick_job: Option<Receiver<Vec<Category>>>,
    clutter: Option<Vec<Category>>,
    clutter_job: Option<Receiver<Vec<Category>>>,
    browsers: Option<Vec<Browser>>,
    browsers_job: Option<Receiver<Vec<Browser>>>,
    running: Vec<String>,
    dups: Option<Vec<DupGroup>>,
    dups_job: Option<Receiver<Vec<DupGroup>>>,
    dup_filter: Option<Kind>,
    dup_preselect: Preselect,
    apps: Option<Vec<apps::App>>,
    apps_job: Option<Receiver<Vec<apps::App>>>,
    /// App sizes arriving one by one after the list shows.
    apps_sizes: Option<Receiver<(PathBuf, apps::Measured)>>,
    app_tab: AppTab,
    app_sort: AppSort,
    startup: Option<Vec<apps::StartItem>>,
    start_search: String,
    start_sort: StartSort,
    monitor_on: bool,
    stats_30_days: bool,
    clean_job: Option<Receiver<Done>>,
    confirm: Option<Confirm>,
    message: Option<String>,
    sys: sysinfo::System,
    disks: sysinfo::Disks,
    last_refresh: f64,
    nav_open: bool,
    needs_fda: bool,
    done: Option<DoneScreen>,
    clutter_sel: Option<usize>,
    sort: SortBy,
    show_all: HashSet<String>,
    browser_open: HashSet<&'static str>,
    dup_pick: bool,
    dup_preview: Option<PathBuf>,
    photos: Option<photos::PhotoResults>,
    photos_job: Option<Receiver<photos::PhotoResults>>,
    photo_tex: std::collections::HashMap<PathBuf, egui::TextureHandle>,
    photo_view: u8,
    app_sel: Option<String>,
    start_sel: Option<usize>,
    cpu_hist: Vec<f32>,
    mem_hist: Vec<f32>,
    components: Option<sysinfo::Components>,
    battery: Option<tabs::BatteryInfo>,
    last_battery: f64,
    settings_tab: usize,
    stats_back: i32,
    rules_check: Option<Vec<mechanics::Row>>,
    allow_input: String,
    sched_open: Option<String>,
    import_note: Option<String>,
    /// Whether the background job is set up (None until checked).
    agent_on: Option<bool>,
    /// Menu-bar icon, and whether the window is showing the panel, visible, or quitting.
    tray: Option<tray::Tray>,
    tray_tried: bool,
    /// The Full Disk Access guide opened from SEE HOW, and when access was last checked.
    fda_guide: bool,
    fda_checked: f64,
    /// Graphics-chip use and fans (read every 2 seconds while a monitor shows).
    gpu: Option<f32>,
    fans: Vec<sensors::Fan>,
    last_sensors: f64,
    /// The tour view on screen and when it appeared, for its entrance motion.
    tour_anim: (u8, f64),
    /// The folder right-click item as last set up (None until the first frame).
    folder_action_on: Option<bool>,
    /// `--scan-folder`: look for duplicates in this folder as soon as the window opens.
    scan_folder: Option<PathBuf>,
    /// Alerts already sent as notifications, and when the alerts were last checked.
    notified: HashSet<String>,
    last_alert_check: f64,
    /// Super Clean: the scan, whether its results are showing, the duplicate and photo files it
    /// suggests, and the startup items ticked to turn off (by location).
    super_job: Option<Receiver<super_clean::Found>>,
    super_ready: bool,
    super_picks: super_clean::Picks,
    super_stop: HashSet<String>,
    /// One progress per Super Clean step (they run at the same time).
    super_steps: Vec<Progress>,
    /// Held while busy: keeps macOS from throttling a hidden window's work.
    awake: Option<awake::Token>,
    /// Update check: running, its result, and a newer release if there is one.
    update_job: Option<Receiver<Result<Option<update::Release>, String>>>,
    update_note: Option<String>,
    update: Option<update::Release>,
    /// `--theme` for this run, over the Appearance setting.
    theme_force: Option<u8>,
    /// eframe restores a window size older versions saved; the panel resets it once.
    sized: bool,
    panel: bool,
    panel_seen_focus: bool,
    visible: bool,
    quitting: bool,
    rec_idx: usize,
    host: String,
    shot: Option<PathBuf>,
    frames: u32,
    /// (start the page's scan on launch, take the picture while it's scanning)
    auto: (bool, bool),
}

fn spawn<T: Send + 'static>(ctx: &egui::Context, f: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(f());
        ctx.request_repaint();
    });
    rx
}

fn poll<T>(job: &mut Option<Receiver<T>>) -> Option<T> {
    let v = job.as_ref()?.try_recv().ok();
    if v.is_some() {
        *job = None;
    }
    v
}

impl Sweep {
    fn new(cc: &eframe::CreationContext, shot: Option<PathBuf>, start: &str, auto: (bool, bool)) -> Self {
        kit::setup(&cc.egui_ctx);
        let mut saved: Saved = cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default();
        if saved.dup_folders.is_empty() {
            saved.dup_folders.push(clean::home());
        }
        // For pictures of a result screen: `--folder <dir>` looks for duplicates there only,
        // `--pick <n>` opens the n-th clutter category.
        let args: Vec<String> = std::env::args().collect();
        let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
        if let Some(f) = arg("--folder") {
            saved.dup_folders = vec![PathBuf::from(f)];
        }
        let pick = arg("--pick").and_then(|n| n.parse().ok());
        // `--tour <0-4>` shows a tour stage for this run (for pictures).
        if let Some(t) = arg("--tour").and_then(|n| n.parse().ok()) {
            saved.tour = t;
        }
        // Only macOS hides parts of the disk: elsewhere the tour starts at the clutter step.
        if !cfg!(target_os = "macos") && saved.tour == tour::FDA {
            saved.tour = tour::CLUTTER;
        }
        // `--theme light|dark` shows that look for this run only (for pictures).
        let theme_force = match arg("--theme").as_deref() {
            Some("dark") => Some(1),
            Some("light") => Some(0),
            _ => None,
        };
        kit::set_theme(theme_force.unwrap_or((saved.settings.appearance == 2) as u8));
        kit::apply_style(&cc.egui_ctx);
        if shot.is_none() {
            for (at, key, bytes, items) in schedule::take_pending_log() {
                saved.log.push(Entry { at, key, bytes, items });
            }
        }
        // Settings › Trash: empty old Trash items in the background (never during a picture).
        if saved.settings.trash_auto && shot.is_none() {
            let days = clean::TRASH_DAYS[saved.settings.trash_days_idx.min(4)];
            std::thread::spawn(move || clean::empty_old_trash(days));
        }
        let page = match start {
            "quick" => Page::Quick,
            "clutter" => Page::Clutter,
            "browser" => Page::Browsers,
            "duplicates" | "locations" => Page::Duplicates,
            "photos" => Page::Photos,
            "apps" => Page::Uninstaller,
            "super" => Page::Super,
            "startup" => Page::Startup,
            "monitor" => Page::Monitor,
            "stats" => Page::Stats,
            "settings" | "rules" | "browser-settings" => Page::Settings,
            _ => Page::Home,
        };
        Self {
            page,
            saved,
            progress: Progress::default(),
            quick: None,
            quick_job: None,
            clutter: None,
            clutter_job: None,
            browsers: None,
            browsers_job: None,
            running: vec![],
            dups: None,
            dups_job: None,
            dup_filter: None,
            dup_preselect: Preselect::KeepNewest,
            apps: None,
            apps_job: None,
            apps_sizes: None,
            app_tab: AppTab::All,
            app_sort: AppSort::Name,
            startup: None,
            start_search: String::new(),
            start_sort: StartSort::Name,
            monitor_on: false,
            stats_30_days: false,
            clean_job: None,
            confirm: None,
            message: None,
            sys: sysinfo::System::new().expect("system information"),
            disks: sysinfo::Disks::new_with_refreshed_list().expect("disk list"),
            last_refresh: -10.0,
            nav_open: false,
            needs_fda: args.iter().any(|a| a == "--needs-fda") || clean::needs_full_disk_access(),
            // `--demo-admin`: a sample finished cleaning with system items left (for pictures).
            done: args.iter().any(|a| a == "--demo-admin").then(|| DoneScreen {
                page,
                title: "All clean!".into(),
                lines: vec!["You freed up 12.40 GB.".into()],
                admin: vec![(PathBuf::from("/Library/Caches/com.example.updater"), 734_003_200), (PathBuf::from("/private/var/log/example.log"), 12_582_912)],
            }),
            clutter_sel: pick,
            sort: SortBy::Size,
            show_all: HashSet::new(),
            browser_open: HashSet::new(),
            dup_pick: start == "locations",
            dup_preview: None,
            photos: None,
            photos_job: None,
            photo_tex: Default::default(),
            photo_view: 0,
            app_sel: None,
            start_sel: None,
            cpu_hist: vec![],
            mem_hist: vec![],
            components: sysinfo::Components::new_with_refreshed_list().ok(),
            battery: None,
            last_battery: -100.0,
            settings_tab: if start == "browser-settings" { 2 } else { arg("--tab").and_then(|t| t.parse().ok()).unwrap_or(0) },
            stats_back: 0,
            rules_check: (start == "rules").then(mechanics::compare),
            allow_input: String::new(),
            sched_open: None,
            import_note: None,
            agent_on: None,
            tray: None,
            tray_tried: false,
            fda_guide: false,
            gpu: None,
            fans: vec![],
            last_sensors: -10.0,
            tour_anim: (255, 0.0),
            // `--needs-fda`: show the Full Disk Access steps as if access were missing (for pictures).
            fda_checked: if args.iter().any(|a| a == "--needs-fda") { f64::INFINITY } else { 0.0 },
            folder_action_on: None,
            scan_folder: arg("--scan-folder").map(PathBuf::from),
            notified: HashSet::new(),
            last_alert_check: -1e9,
            super_job: None,
            super_ready: false,
            super_picks: Default::default(),
            super_stop: HashSet::new(),
            super_steps: vec![],
            awake: None,
            update_job: None,
            update_note: None,
            update: None,
            theme_force,
            sized: false,
            panel: start == "tray",
            panel_seen_focus: false,
            visible: true,
            quitting: false,
            rec_idx: 0,
            host: sysinfo::System::host_name().unwrap_or_else(|_| "This computer".into()),
            shot,
            frames: 0,
            auto,
        }
    }

    fn busy(&self) -> bool {
        self.quick_job.is_some()
            || self.clutter_job.is_some()
            || self.browsers_job.is_some()
            || self.dups_job.is_some()
            || self.apps_job.is_some()
            || self.photos_job.is_some()
            || self.clean_job.is_some()
            || self.super_job.is_some()
    }

    fn log_report(&mut self, r: &CleanReport) {
        let at = now_secs();
        for (title, browser, bytes, items) in &r.by_cat {
            if *items > 0 {
                let key = stat_key(title, *browser).to_string();
                self.saved.log.push(Entry { at, key, bytes: *bytes, items: *items as u64 });
            }
        }
    }

    fn poll_jobs(&mut self) {
        if let Some(v) = poll(&mut self.quick_job) {
            self.running = clean::running_processes();
            self.quick = Some(v);
        }
        if let Some(v) = poll(&mut self.clutter_job) {
            self.saved.last_clutter_total = v.iter().map(|c| c.size()).sum();
            self.clutter = Some(v);
        }
        if let Some(v) = poll(&mut self.browsers_job) {
            self.running = clean::running_processes();
            self.browsers = Some(v);
        }
        if let Some(v) = poll(&mut self.dups_job) {
            self.dups = Some(v);
        }
        if let Some(v) = poll(&mut self.photos_job) {
            self.photos = Some(v);
        }
        if let Some(r) = poll(&mut self.update_job) {
            self.update_note = Some(match &r {
                Ok(Some(rel)) => format!("Claybroom {} is available.", rel.version),
                Ok(None) => "You have the latest Claybroom.".into(),
                Err(e) => format!("Couldn't check: {e}."),
            });
            self.update = r.ok().flatten();
        }
        if let Some(v) = poll(&mut self.apps_job) {
            self.apps = Some(v);
        }
        if let Some(f) = poll(&mut self.super_job) {
            self.super_found(f);
        }
        if let (Some(rx), Some(list)) = (&self.apps_sizes, self.apps.as_mut()) {
            for (path, m) in rx.try_iter() {
                if let Some(a) = list.iter_mut().find(|a| a.path.as_ref() == Some(&path)) {
                    (a.size, a.last_used, a.leftovers, a.measured) = (m.size, m.last_used, m.leftovers, true);
                }
            }
        }
        if let Some(done) = poll(&mut self.clean_job) {
            let finished = |r: &CleanReport| {
                let mut lines = vec![format!("You freed up {}.", fmt_size(r.freed))];
                let in_use = r.failed.saturating_sub(r.needs_admin.len());
                if in_use > 0 {
                    lines.push(format!("{in_use} item(s) were in use."));
                }
                if !r.skipped.is_empty() {
                    lines.push(format!("Close {} to clean it too.", r.skipped.join(", ")));
                }
                lines.extend(r.notes.iter().cloned());
                lines
            };
            let page = self.page;
            match &done {
                Done::Quick(r) | Done::Clutter(r) | Done::Browsers(r) | Done::Duplicates(r) | Done::Photos(r) => {
                    self.done = Some(DoneScreen { page, title: "All clean!".into(), lines: finished(r), admin: r.needs_admin.clone() });
                }
                Done::Admin(r) => {
                    self.done = Some(DoneScreen { page, title: "All clean!".into(), lines: finished(r), admin: vec![] });
                }
                Done::Apps(ok, errs) => {
                    let mut lines = vec![format!("{ok} app(s) uninstalled.")];
                    lines.extend(errs.iter().take(3).cloned());
                    self.done = Some(DoneScreen { page, title: "Uninstall finished".into(), lines, admin: vec![] });
                }
                Done::Super { junk, dups, photos, apps, stopped, errs } => {
                    let all = CleanReport {
                        freed: junk.freed + dups.freed + photos.freed,
                        failed: junk.failed + dups.failed + photos.failed,
                        skipped: junk.skipped.clone(),
                        notes: junk.notes.clone(),
                        needs_admin: junk.needs_admin.clone(),
                        ..Default::default()
                    };
                    let mut lines = finished(&all);
                    if *apps > 0 {
                        lines.insert(1, format!("{apps} app(s) uninstalled."));
                    }
                    if *stopped > 0 {
                        lines.insert(1, format!("{stopped} startup item(s) turned off."));
                    }
                    lines.extend(errs.iter().take(3).cloned());
                    self.done = Some(DoneScreen { page, title: "All clean!".into(), lines, admin: all.needs_admin.clone() });
                }
            }
            let cleaned = match &done {
                Done::Clutter(_) => Some(Page::Clutter),
                Done::Browsers(_) => Some(Page::Browsers),
                Done::Duplicates(_) => Some(Page::Duplicates),
                Done::Photos(_) => Some(Page::Photos),
                Done::Super { .. } => Some(Page::Super),
                _ => None,
            };
            if let Some(pg) = cleaned {
                self.saved.tour = tour::after(self.saved.tour, pg);
            }
            match done {
                Done::Quick(r) => {
                    self.log_report(&r);
                    self.quick = None;
                }
                Done::Clutter(r) => {
                    self.log_report(&r);
                    self.clutter = None;
                }
                Done::Browsers(r) => {
                    self.log_report(&r);
                    self.browsers = None;
                }
                Done::Duplicates(r) => {
                    self.saved.log.push(Entry { at: now_secs(), key: "Duplicates".into(), bytes: r.freed, items: r.removed as u64 });
                    self.dups = None;
                }
                Done::Apps(ok, _) => {
                    self.saved.log.push(Entry { at: now_secs(), key: "Apps".into(), bytes: 0, items: ok as u64 });
                    self.apps = None;
                    self.apps_sizes = None;
                }
                Done::Photos(r) => {
                    self.saved.log.push(Entry { at: now_secs(), key: "Photos".into(), bytes: r.freed, items: r.removed as u64 });
                    self.photos = None;
                }
                Done::Admin(r) => {
                    if r.removed > 0 {
                        self.saved.log.push(Entry { at: now_secs(), key: "Other".into(), bytes: r.freed, items: r.removed as u64 });
                    }
                }
                Done::Super { junk, dups, photos, apps, .. } => {
                    self.log_report(&junk);
                    for (key, r) in [("Duplicates", &dups), ("Photos", &photos)] {
                        if r.removed > 0 {
                            self.saved.log.push(Entry { at: now_secs(), key: key.into(), bytes: r.freed, items: r.removed as u64 });
                        }
                    }
                    if apps > 0 {
                        self.saved.log.push(Entry { at: now_secs(), key: "Apps".into(), bytes: 0, items: apps as u64 });
                    }
                    // Every screen scans again next time.
                    (self.quick, self.dups, self.photos, self.apps, self.startup) = (None, None, None, None, None);
                    self.super_ready = false;
                    self.super_stop.clear();
                }
            }
        }
    }

    /// Remove the system items an administrator approves (the system's own prompt asks).
    pub(crate) fn clean_as_admin(&mut self, ctx: &egui::Context, items: Vec<(PathBuf, u64)>) {
        clean::reset(&self.progress);
        clean::say(&self.progress, "Waiting for an administrator…");
        self.clean_job = Some(spawn(ctx, move || {
            let paths: Vec<PathBuf> = items.iter().map(|i| i.0.clone()).collect();
            let res = shell::admin_delete(&paths);
            let mut r = CleanReport::default();
            for (p, bytes) in &items {
                if p.exists() {
                    r.failed += 1;
                } else {
                    r.removed += 1;
                    r.freed += bytes;
                }
            }
            match res {
                Err(e) if e == "cancelled" => r.notes.push("Nothing more was removed: the administrator prompt was cancelled.".into()),
                Err(e) => r.notes.push(format!("The system refused: {e}")),
                Ok(()) => {}
            }
            Done::Admin(r)
        }));
    }

    fn start_quick(&mut self, ctx: &egui::Context) {
        clean::reset(&self.progress);
        self.done = None;
        let (s, p) = (self.saved.settings.clone(), self.progress.clone());
        self.quick_job = Some(spawn(ctx, move || clean::scan_quick(&s, &p)));
        self.page = Page::Quick;
    }
    fn start_clutter(&mut self, ctx: &egui::Context) {
        clean::reset(&self.progress);
        self.done = None;
        let (s, p) = (self.saved.settings.clone(), self.progress.clone());
        self.clutter_job = Some(spawn(ctx, move || clean::scan_clutter(&s, &p)));
        self.page = Page::Clutter;
    }
    fn start_browsers(&mut self, ctx: &egui::Context) {
        clean::reset(&self.progress);
        self.done = None;
        let p = self.progress.clone();
        let off = self.saved.settings.browsers_off.clone();
        self.browsers_job = Some(spawn(ctx, move || clean::scan_browsers(&p, &off)));
        self.page = Page::Browsers;
    }
    fn start_dups(&mut self, ctx: &egui::Context) {
        let f: Vec<PathBuf> = self.saved.dup_folders.iter().filter(|f| !self.saved.dup_off.contains(f)).cloned().collect();
        self.start_dups_in(ctx, f);
    }
    /// Look for duplicates in these folders (the saved list, or one folder from the right-click menu).
    fn start_dups_in(&mut self, ctx: &egui::Context, f: Vec<PathBuf>) {
        clean::reset(&self.progress);
        self.done = None;
        let (s, p) = (self.saved.settings.clone(), self.progress.clone());
        self.dup_preview = None;
        self.dups_job = Some(spawn(ctx, move || clean::scan_duplicates(&f, &s, &p)));
        self.page = Page::Duplicates;
    }
    fn start_apps(&mut self, ctx: &egui::Context) {
        clean::reset(&self.progress);
        self.done = None;
        let p = self.progress.clone();
        // List first, then measure in the background while the list is on screen.
        let (tx, rx) = std::sync::mpsc::channel();
        self.apps_sizes = Some(rx);
        let c = ctx.clone();
        self.apps_job = Some(spawn(ctx, move || {
            let list = apps::scan_apps(&p);
            apps::measure_in_background(&list, tx, c);
            list
        }));
        self.page = Page::Uninstaller;
    }
}

/// Intro page of a module: heading, four feature rows, one call to action, a soft picture.
/// Returns true when the call to action is pressed.
fn intro(ui: &mut egui::Ui, title: &str, sub: &str, rows: [(Icon, &str, &str); 4], cta: &str, enabled: bool, accent: Color32) -> bool {
    let full = ui.max_rect();
    // The page's illustration, bottom right (about 360 × 270 points).
    match illus::scene_for(title) {
        Some(scene) => {
            let art = egui::Rect::from_min_max(full.right_bottom() - vec2(370.0, 280.0), full.right_bottom() + vec2(14.0, 18.0));
            // Pictures move gently; ~30 redraws a second only while an intro page is showing.
            illus::draw(ui.painter(), art, scene, ui.input(|i| i.time) as f32);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        }
        None => kit::placeholder_art(ui.painter(), egui::Rect::from_center_size(full.right_bottom() - vec2(200.0, 170.0), Vec2::splat(260.0)), accent),
    }
    heading(ui, title, sub);
    ui.add_space(14.0);
    let mut go = false;
    ui.horizontal(|ui| {
        ui.add_space(40.0);
        ui.vertical(|ui| {
            for (ic, t, d) in rows {
                ui.horizontal(|ui| {
                    kit::icon_box(ui, ic, 48.0);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(b(t, 18.0));
                        ui.label(RichText::new(d).size(15.5).color(dim()));
                    });
                });
                ui.add_space(10.0);
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(58.0);
                go = kit::cta(ui, &format!("{cta}  →"), enabled);
            });
        });
    });
    go
}




impl Sweep {
    /// The menu-bar icon: start or stop it with the setting, answer its clicks, and keep the app
    /// running there when the window is closed.
    fn tray_logic(&mut self, ctx: &egui::Context) {
        if self.shot.is_some() {
            return;
        }
        let want = self.saved.settings.tray;
        if want && self.tray.is_none() && !self.tray_tried {
            self.tray_tried = true;
            self.tray = tray::start(ctx);
        } else if !want && self.tray.is_some() {
            self.tray = None;
            self.tray_tried = false;
        }
        let msgs: Vec<tray::TrayMsg> = self.tray.as_ref().map(|t| t.rx.try_iter().collect()).unwrap_or_default();
        for m in msgs {
            match m {
                tray::TrayMsg::Toggle(icon) => {
                    if self.panel && self.visible {
                        self.hide(ctx);
                    } else {
                        self.show_panel(ctx, icon);
                    }
                }
                tray::TrayMsg::Open => self.open_main(ctx, self.page),
                tray::TrayMsg::QuickClean => {
                    self.open_main(ctx, Page::Quick);
                    self.start_quick(ctx);
                }
                tray::TrayMsg::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        // Closing the window keeps Claybroom running in the menu bar.
        if self.tray.is_some() && !self.quitting && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide(ctx);
        }
        // The panel goes away when you click elsewhere, like a menu.
        if self.panel && self.visible {
            match ctx.input(|i| i.viewport().focused) {
                Some(true) => self.panel_seen_focus = true,
                Some(false) if self.panel_seen_focus => self.hide(ctx),
                _ => {}
            }
        }
    }

    fn hide(&mut self, ctx: &egui::Context) {
        self.visible = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    /// Turn the window into the small panel under the menu-bar icon.
    fn show_panel(&mut self, ctx: &egui::Context, icon: Option<[f64; 4]>) {
        let size = egui::Vec2::from(panel::PANEL);
        let monitor = ctx.input(|i| i.viewport().monitor_size);
        let pos = tray::panel_pos(icon, ctx.pixels_per_point(), size, monitor);
        self.panel = true;
        self.panel_seen_focus = false;
        self.visible = true;
        self.last_refresh = -10.0;
        for c in [
            egui::ViewportCommand::Decorations(false),
            egui::ViewportCommand::MinInnerSize(egui::vec2(300.0, 400.0)),
            egui::ViewportCommand::InnerSize(size),
            egui::ViewportCommand::OuterPosition(pos),
            egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop),
            egui::ViewportCommand::Visible(true),
            egui::ViewportCommand::Focus,
        ] {
            ctx.send_viewport_cmd(c);
        }
    }

    /// Show the full window on a page.
    pub fn open_main(&mut self, ctx: &egui::Context, page: Page) {
        let was_panel = self.panel;
        self.panel = false;
        self.visible = true;
        self.page = page;
        for c in [
            egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal),
            egui::ViewportCommand::Decorations(true),
            egui::ViewportCommand::MinInnerSize(egui::vec2(900.0, 600.0)),
            egui::ViewportCommand::InnerSize(egui::Vec2::from(WINDOW)),
            egui::ViewportCommand::Visible(true),
            egui::ViewportCommand::Focus,
        ] {
            ctx.send_viewport_cmd(c);
        }
        if was_panel {
            if let Some(c) = egui::ViewportCommand::center_on_screen(ctx) {
                ctx.send_viewport_cmd(c);
            }
        }
    }
}

/// Recommended: everything except personal files and cookies (cookies sign you out of sites).
fn recommended(c: &Category) -> bool {
    let t = c.title.to_lowercase();
    !c.to_trash && !t.contains("cookies") && c.title != clean::AUTOFILL && c.title != clean::PASSWORDS
}

impl eframe::App for Sweep {
    /// Runs even while the window is covered: keep painting so a picture can still be taken.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs();
        match (self.busy(), self.awake.is_some()) {
            (true, false) => self.awake = Some(awake::begin()),
            (false, true) => self.awake = None,
            _ => {}
        }
        // Keep the hourly background job in step with the schedules (never during a picture).
        let want = schedule::wanted(&self.saved.settings);
        if self.shot.is_none() && self.agent_on != Some(want) {
            self.agent_on = Some(want);
            std::thread::spawn(move || schedule::sync_agent(want));
        }
        if self.panel && !self.sized {
            self.sized = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::from(panel::PANEL)));
        }
        // Appearance: follow the system's light/dark (and its changes) unless one is picked.
        let want = self.theme_force.unwrap_or_else(|| match self.saved.settings.appearance {
            1 => 0,
            2 => 1,
            _ => (ctx.system_theme() == Some(egui::Theme::Dark)) as u8,
        });
        if want != kit::dark() as u8 {
            kit::set_theme(want);
            kit::apply_style(ctx);
            ctx.request_repaint();
        }
        if self.shot.is_none() && self.saved.settings.check_updates && self.update_note.is_none() && self.update_job.is_none() {
            self.update_job = Some(spawn(ctx, update::latest));
        }
        if self.shot.is_none() {
            // The folder right-click item follows its setting.
            let want = self.saved.settings.folder_action;
            if self.folder_action_on != Some(want) {
                self.folder_action_on = Some(want);
                std::thread::spawn(move || {
                    let _ = shell::set_folder_action(want);
                });
            }
            self.alert_notifications(ctx);
        }
        if let Some(f) = self.scan_folder.take() {
            self.start_dups_in(ctx, vec![f]);
        }
        self.tray_logic(ctx);
        if self.shot.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }

        if let Some(path) = self.shot.clone() {
            ctx.request_repaint_after(Duration::from_millis(16)); // keep drawing while unfocused
            if self.auto.0 && self.frames == 0 {
                match self.page {
                    Page::Quick => self.start_quick(&ctx),
                    Page::Clutter => self.start_clutter(&ctx),
                    Page::Browsers => self.start_browsers(&ctx),
                    Page::Duplicates => self.start_dups(&ctx),
                    Page::Uninstaller => self.start_apps(&ctx),
                    Page::Photos => self.start_photos(&ctx),
                    Page::Startup => self.startup = Some(apps::scan_startup()),
                    Page::Super => self.start_super(&ctx),
                    Page::Monitor => self.monitor_on = true,
                    _ => {}
                }
            }
            if !self.busy() || self.auto.1 || self.frames == 0 {
                self.frames += 1;
            }
            if self.frames == 120 {
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let code = match image::RgbaImage::from_raw(w as u32, h as u32, rgba).map(|i| i.save(&path)) {
                        Some(Ok(())) => 0,
                        _ => 1,
                    };
                    std::process::exit(code);
                });
            }
        }

        if self.panel {
            return self.tray_panel(ui);
        }
        let screen = ui.max_rect();
        let rail = egui::Panel::left("nav")
            .exact_size(52.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(rail()).inner_margin(Margin::symmetric(4, 14)))
            .show(ui, |ui| self.nav(ui, false))
            .response
            .rect;

        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(Margin::symmetric(34, 22)))
            .show(ui, |ui| {
                kit::gradient(ui.painter(), ui.max_rect().expand2(vec2(34.0, 22.0)));
                if let Some(m) = self.message.clone() {
                    egui::Frame::new()
                        .fill(kit::good().gamma_multiply(0.18))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(m);
                                if ui.small_button("✖").clicked() {
                                    self.message = None;
                                }
                            });
                        });
                    ui.add_space(6.0);
                }
                if self.done.as_ref().is_some_and(|d| d.page == self.page) {
                    return self.done_screen(ui);
                }
                match self.page {
                    Page::Home => self.home(ui),
                    Page::Quick => self.quick_page(ui),
                    Page::Clutter => self.clutter_page(ui),
                    Page::Browsers => self.browsers_page(ui),
                    Page::Duplicates => self.dups_page(ui),
                    Page::Uninstaller => self.apps_page(ui),
                    Page::Photos => self.photos_page(ui),
                    Page::Startup => self.startup_page(ui),
                    Page::Monitor => self.monitor_page(ui),
                    Page::Stats => self.stats_page(ui),
                    Page::Settings => self.settings_page(ui),
                    Page::Super => self.super_page(ui),
                }
            });

        // The icon rail opens over the page with labels while hovered.
        let open_rect = egui::Rect::from_min_size(rail.min, vec2(240.0, rail.height()));
        self.nav_open = ctx
            .pointer_hover_pos()
            .is_some_and(|p| if self.nav_open { open_rect.contains(p) } else { rail.contains(p) });
        if self.nav_open && self.confirm.is_none() {
            ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("fade")))
                .rect_filled(screen, 0.0, kit::bg().gamma_multiply(0.67));
            egui::Area::new(egui::Id::unique("nav_open")).fixed_pos(rail.min).order(egui::Order::Foreground).show(&ctx, |ui| {
                egui::Frame::new().fill(kit::rail()).inner_margin(Margin::symmetric(4, 14)).show(ui, |ui| {
                    ui.set_width(232.0);
                    ui.set_height(rail.height() - 28.0);
                    self.nav(ui, true);
                });
            });
        }

        self.confirm_modal(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        schedule::write_settings(&self.saved.settings);
        eframe::set_value(storage, eframe::APP_KEY, &self.saved);
    }
}

impl Sweep {
    fn nav_item(&mut self, ui: &mut egui::Ui, page: Page, icon: Icon, label: &str, labels: bool, step: f32) {
        let w = if labels { ui.available_width() } else { 44.0 };
        let h = (step - 4.0).min(44.0);
        let (r, resp) = ui.allocate_exact_size(vec2(w, h), egui::Sense::click());
        let active = self.page == page;
        if active || resp.hovered() {
            let fill = if active { rail_active() } else { rail_active().gamma_multiply(0.45) };
            ui.painter().rect_filled(r, 9.0, fill);
        }
        let ir = egui::Rect::from_min_size(r.left_center() + vec2(9.0, -13.0), Vec2::splat(26.0));
        kit::icon(ui.painter(), ir, icon, ink());
        if labels {
            ui.painter().text(r.left_center() + vec2(48.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::new(15.0, kit::bold_family()), ink());
        }
        if resp.clicked() {
            self.page = page;
            self.nav_open = false;
        }
        ui.add_space(step - h);
    }

    fn nav(&mut self, ui: &mut egui::Ui, labels: bool) {
        ui.add_space(4.0);
        // Twelve items: closer together when the window is short, so Settings never overlaps.
        ui.spacing_mut().item_spacing.y = 0.0;
        let step = ((ui.available_height() - 4.0) / (PAGES.len() + 1) as f32).clamp(36.0, 54.0);
        for (page, icon, name) in PAGES {
            self.nav_item(ui, page, icon, name, labels, step);
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            self.nav_item(ui, Page::Settings, Icon::Gear, "SETTINGS", labels, step);
        });
    }

    /// Shown when macOS keeps parts of the disk hidden from us.
    fn fda_banner(&mut self, ui: &mut egui::Ui) {
        if !self.needs_fda {
            return;
        }
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(b("Give disk access", 16.0));
                    ui.label(muted("Reveal the whole disk to find more clutter: macOS hides the Trash, Mail and some browsers until you allow it."));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary(ui, "SEE HOW", true) {
                        self.fda_guide = true;
                        self.page = Page::Home;
                    }
                });
            });
        });
        ui.add_space(6.0);
    }

    fn home(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.fda_guide_showing() {
            return self.fda_guide_ui(ui);
        }
        let touring = (tour::CLUTTER..tour::DONE).contains(&self.saved.tour);
        ui.add_space(24.0);
        if touring {
            heading(ui, "Welcome to Claybroom", "A tidy disk, without the upsell");
        } else {
            heading(ui, "You're using Claybroom", "Free and open source. Every feature is yours to use.");
        }
        // Main disk usage.
        if let Some(d) = self.disks.iter().max_by_key(|d| d.total_space()) {
            let (total, free) = (d.total_space(), d.available_space());
            ui.vertical_centered(|ui| {
                ui.set_max_width(420.0);
                ui.add(
                    egui::ProgressBar::new(total.saturating_sub(free) as f32 / total.max(1) as f32)
                        .fill(accent())
                        .desired_height(10.0)
                        .corner_radius(5.0),
                );
                ui.label(muted(format!("{} free of {} on your main disk", fmt_size(free), fmt_size(total))));
            });
        }
        if let Some(rel) = &self.update {
            let (v, url) = (rel.version.clone(), rel.url.clone());
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    ui.label(b(format!("Claybroom {v} is available."), 15.0));
                    if ui.link(RichText::new("Download").color(accent())).clicked() {
                        update::open_page(&url);
                    }
                });
            });
        }
        self.alerts(ui);
        ui.add_space(34.0);

        let busy = self.busy();
        let mut go = None;
        let row = |ui: &mut egui::Ui, ic: Icon, t: &str, sub: &str, btn: &str, go: &mut Option<Page>, page: Page| {
            tile(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(30.0), egui::Sense::hover());
                    kit::icon(ui.painter(), r, ic, ink());
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(b(t, 19.0));
                        ui.label(RichText::new(sub).size(15.5).color(dim()));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(ui, btn, !busy) {
                            *go = Some(page);
                        }
                    });
                });
            });
        };
        ui.horizontal(|ui| {
            ui.add_space(40.0);
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width() - 40.0);
                // Claybroom's own Super Clean sits beside Quick Clean.
                let mut sup = false;
                if touring {
                    // The first-run tour's next step takes the top row.
                    if let Some(p) = self.tour_card(ui, busy) {
                        go = Some(p);
                    }
                } else {
                    ui.columns(2, |cols| {
                        sup = super_clean::home_card(&mut cols[0], !busy);
                        row(&mut cols[1], Icon::Quick, "Quick Clean", "Caches, logs and trash", "SCAN", &mut go, Page::Quick);
                    });
                }
                if sup {
                    go = Some(Page::Super);
                }
                ui.add_space(14.0);
                // The deeper tools: one row of upright cards, each opened by clicking it.
                let tiles = [
                    (Icon::Duplicates, "Duplicates", "Same file, stored twice", Page::Duplicates),
                    (Icon::Picture, "Photos", "Blurry, dark and look-alike shots", Page::Photos),
                    (Icon::Trash, "Apps", "Remove them with their leftovers", Page::Uninstaller),
                    (Icon::Startup, "Startup", "What opens when you log in", Page::Startup),
                ];
                ui.columns(4, |cols| {
                    for (col, (ic, t, sub, page)) in cols.iter_mut().zip(tiles) {
                        let r = egui::Frame::new().fill(kit::tile_fill()).corner_radius(CornerRadius::same(22)).inner_margin(Margin::same(18)).show(col, |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_min_height(150.0);
                            // Left-aligned, not justified: columns stretch wrapped text otherwise.
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
                                kit::icon(ui.painter(), r, ic, accent());
                                ui.add_space(10.0);
                                ui.label(b(t, 19.0));
                                ui.label(RichText::new(sub).size(14.5).color(dim()));
                            });
                        });
                        let r = r.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        if r.clicked() && !busy {
                            go = Some(page);
                        }
                    }
                });
            });
        });
        match go {
            Some(Page::Super) => self.start_super(&ctx),
            Some(Page::Clutter) => self.start_clutter(&ctx),
            Some(Page::Quick) => self.start_quick(&ctx),
            Some(Page::Browsers) => self.start_browsers(&ctx),
            Some(Page::Uninstaller) => self.start_apps(&ctx),
            Some(Page::Startup) => {
                self.startup = Some(apps::scan_startup());
                self.page = Page::Startup;
            }
            Some(p) => self.page = p,
            None => {}
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.label(RichText::new("Claybroom asks before it removes anything.").size(15.5).color(dim()));
        });
    }

    fn confirm_modal(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let (head, body, verb) = match c {
            Confirm::Quick | Confirm::Clutter => (
                "Clean the selected items?",
                "Caches, logs, browser data and the Trash are deleted for good. Downloads and large files go to the Trash. Open browsers are skipped.".to_string(),
                "CLEAN",
            ),
            Confirm::Browsers => ("Clean the selected browser data?", "Open browsers are skipped; close them first to clean them.".to_string(), "CLEAN"),
            Confirm::Duplicates => {
                let all = self.dups.as_ref().map_or(false, |g| g.iter().any(|g| g.files.iter().all(|f| f.selected)));
                let dest = if self.saved.settings.dup_to_trash { "moved to the Trash" } else { "deleted for good" };
                let mut b = format!("The selected copies will be {dest}.");
                if all {
                    b += "\n\n⚠ In some groups every copy is selected, including the original.";
                }
                ("Remove the selected duplicates?", b, "CLEAN")
            }
            Confirm::Photos => {
                let dest = if self.saved.settings.photos_to_trash { "moved to the Trash" } else { "deleted for good" };
                let lib = self.photos.iter().flat_map(|r| r.all()).filter(|x| x.selected && x.library_id.is_some()).count();
                let mut body = format!("They will be {dest}.");
                if lib > 0 {
                    body = format!("{body}\n\n{lib} of them are in your Photos library: they go to the album \"{}\" in Photos instead, for you to delete there.", shell::REVIEW_ALBUM);
                }
                ("Remove the selected photos?", body, "CLEAN")
            }
            Confirm::SkipFda => (
                "Skip disk access for now?",
                "Without full disk access Claybroom can't see the Trash, Mail downloads or some browsers, so it will find less to clean.".to_string(),
                "SKIP FOR NOW",
            ),
            Confirm::ResetStats => ("Reset statistics?", "Everything the Statistics page counts will be forgotten. Nothing on your disk changes.".to_string(), "RESET"),
            Confirm::Apps => ("Uninstall the selected apps?", "They are removed together with the files they left behind.".to_string(), "UNINSTALL"),
            Confirm::Super => {
                let s = &self.saved.settings;
                let dest = |t: bool| if t { "go to the Trash" } else { "are deleted for good" };
                (
                    "Clean everything you selected?",
                    format!(
                        "Junk is deleted for good; Downloads and large files go to the Trash. Duplicates {} and photos {}. Ticked apps are uninstalled with their leftovers, and ticked startup items are turned off (you can turn them on again). Open browsers are skipped.",
                        dest(s.dup_to_trash),
                        dest(s.photos_to_trash)
                    ),
                    "CLEAN",
                )
            }
            Confirm::RemoveStartup(i) => {
                let name = self.startup.as_ref().and_then(|l| l.get(*i)).map(|x| x.name.clone()).unwrap_or_default();
                ("Remove this startup item?", format!("Once removed, {name} will never open automatically at startup. If it misbehaves afterwards, reinstall it."), "REMOVE")
            }
        };
        let mut go = false;
        let mut close = false;
        let r = egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(b(head, 19.0));
            ui.add_space(6.0);
            ui.label(body);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                go = primary(ui, verb, true);
                if kit::outline(ui, "CANCEL") {
                    close = true;
                }
            });
        });
        if r.should_close() {
            close = true;
        }
        if go {
            self.run_confirmed(ctx);
        }
        if go || close {
            self.confirm = None;
        }
    }

    fn run_confirmed(&mut self, ctx: &egui::Context) {
        clean::reset(&self.progress);
        let p = self.progress.clone();
        match self.confirm.take() {
            Some(Confirm::Quick) => {
                let opts = schedule::browser_opts(&self.saved.settings);
                let (cats, skipped) = clean::without_running(self.quick.as_deref().unwrap_or_default());
                self.clean_job = Some(spawn(ctx, move || {
                    let mut r = clean::clean_categories_with(&cats, &p, &opts);
                    r.skipped = skipped;
                    Done::Quick(r)
                }));
            }
            Some(Confirm::Clutter) => {
                let cats = self.clutter.clone().unwrap_or_default();
                self.clean_job = Some(spawn(ctx, move || Done::Clutter(clean::clean_categories(&cats, &p))));
            }
            Some(Confirm::Browsers) => {
                let all: Vec<Category> = self
                    .browsers
                    .iter()
                    .flatten()
                    .flat_map(|b| b.cats.iter().map(move |c| Category { title: format!("{} {}", b.name, c.title.to_lowercase()), ..c.clone() }))
                    .collect();
                let opts = schedule::browser_opts(&self.saved.settings);
                let (cats, skipped) = clean::without_running(&all);
                self.clean_job = Some(spawn(ctx, move || {
                    let mut r = clean::clean_categories_with(&cats, &p, &opts);
                    r.skipped = skipped;
                    Done::Browsers(r)
                }));
            }
            Some(Confirm::Duplicates) => {
                let g = self.dups.clone().unwrap_or_default();
                let t = self.saved.settings.dup_to_trash;
                self.clean_job = Some(spawn(ctx, move || Done::Duplicates(clean::clean_duplicates(&g, t, &p))));
            }
            Some(Confirm::Photos) => {
                if let Some(res) = self.photos.clone() {
                    let t = self.saved.settings.photos_to_trash;
                    self.clean_job = Some(spawn(ctx, move || Done::Photos(photos::clean(&res, t, &p))));
                }
            }
            Some(Confirm::ResetStats) => self.saved.log.clear(),
            Some(Confirm::SkipFda) => self.skip_fda_guide(),
            Some(Confirm::Super) => {
                let s = self.saved.settings.clone();
                let opts = schedule::browser_opts(&s);
                let (cats, skipped) = clean::without_running(self.quick.as_deref().unwrap_or_default());
                let groups = self.dups.clone().unwrap_or_default();
                let photo_res = self.photos.clone();
                let sel: Vec<apps::App> = self.apps.iter().flatten().filter(|a| a.selected).cloned().collect();
                let stop: Vec<apps::StartItem> = self.startup.iter().flatten().filter(|i| self.super_stop.contains(&i.location)).cloned().collect();
                self.clean_job = Some(spawn(ctx, move || {
                    let mut junk = clean::clean_categories_with(&cats, &p, &opts);
                    junk.skipped = skipped;
                    let dups = clean::clean_duplicates(&groups, s.dup_to_trash, &p);
                    let photos = photo_res.map(|r| photos::clean(&r, s.photos_to_trash, &p)).unwrap_or_default();
                    let mut errs = Vec::new();
                    let mut ok = 0;
                    for a in &sel {
                        clean::say(&p, format!("Uninstalling {}", a.name));
                        match apps::uninstall(a) {
                            Ok(()) => ok += 1,
                            Err(e) => errs.push(format!("{}: {e}", a.name)),
                        }
                    }
                    let mut stopped = 0;
                    for mut it in stop {
                        match apps::set_on(&mut it, false) {
                            Ok(()) => stopped += 1,
                            Err(e) => errs.push(format!("{}: {e}", it.name)),
                        }
                    }
                    Done::Super { junk, dups, photos, apps: ok, stopped, errs }
                }));
            }
            Some(Confirm::Apps) => {
                let sel: Vec<apps::App> = self.apps.iter().flatten().filter(|a| a.selected).cloned().collect();
                self.clean_job = Some(spawn(ctx, move || {
                    let errs: Vec<String> = sel
                        .iter()
                        .filter_map(|a| {
                            clean::say(&p, format!("Uninstalling {}", a.name));
                            apps::uninstall(a).err().map(|e| format!("{}: {e}", a.name))
                        })
                        .collect();
                    Done::Apps(sel.len() - errs.len(), errs)
                }));
            }
            Some(Confirm::RemoveStartup(i)) => {
                if let Some(list) = self.startup.as_mut() {
                    if let Some(it) = list.get(i) {
                        match apps::remove(it) {
                            Ok(()) => {
                                list.remove(i);
                            }
                            Err(e) => self.message = Some(e),
                        }
                    }
                }
            }
            None => {}
        }
    }
}
