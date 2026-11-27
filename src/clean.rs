//! Finding and removing clutter, browser data, large files and duplicates.
//! The rules come from MECHANICS.md.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use walkdir::WalkDir;

/// What a running scan or clean is doing, for the progress ring.
#[derive(Default, Clone)]
pub struct Status {
    pub text: String,
    /// 0..=1
    pub frac: f32,
    pub found: u64,
    /// The part of the ring the current step fills: it moves from `base` over `span`.
    pub base: f32,
    pub span: f32,
    /// Super Clean: what this step found, once it's finished.
    pub notes: Vec<String>,
}

pub type Progress = Arc<Mutex<Status>>;

pub fn say(p: &Progress, s: impl Into<String>) {
    p.lock().unwrap().text = s.into();
}

pub fn set_frac(p: &Progress, f: f32) {
    p.lock().unwrap().frac = f.clamp(0.0, 1.0);
}

fn add_found(p: &Progress, bytes: u64) {
    p.lock().unwrap().found += bytes;
}

pub fn reset(p: &Progress) {
    *p.lock().unwrap() = Status::default();
}

// ---------- settings ----------

pub const LARGE_FILE_SIZES: [u64; 6] = [
    250_000_000, 500_000_000, 1_000_000_000, 2_000_000_000, 5_000_000_000, 10_000_000_000,
];
pub const DUP_SIZES: [u64; 5] = [16_000, 500_000, 1_000_000, 10_000_000, 50_000_000];
pub const LARGE_APP_SIZES: [u64; 6] = [
    500_000_000, 1_000_000_000, 2_000_000_000, 5_000_000_000, 10_000_000_000, 15_000_000_000,
];
/// Folders inside ~/Library/Caches and ~/Library/Logs that are never cleaned.
pub const USER_CACHE_EXCLUDES: [&str; 3] = ["com.apple.Safari/Extensions", "fontconfig", "Java"];
pub const USER_LOG_EXCLUDES: [&str; 0] = [];
/// Helper files macOS leaves on external drives (config.json SystemMeta.ExternalDisk).
pub const DRIVE_JUNK_DIRS: [&str; 3] = [".fseventsd", ".Spotlight-V100", ".TemporaryItems"];
pub const DRIVE_JUNK_FILES: [&str; 2] = [".DS_Store", ".VolumeIcon.icns"];
pub const DRIVE_JUNK_PREFIX: &str = "._";
/// Statistics "enough space for about…" (bytes per picture, minute of video, song).
pub const PICTURE_BYTES: f64 = 3.0e6;
pub const VIDEO_MINUTE_BYTES: f64 = 16.0e6;
pub const SONG_BYTES: f64 = 4.0e6;

pub fn is_drive_junk(name: &str, is_dir: bool) -> bool {
    if is_dir {
        DRIVE_JUNK_DIRS.contains(&name)
    } else {
        DRIVE_JUNK_FILES.contains(&name) || name.starts_with(DRIVE_JUNK_PREFIX)
    }
}

/// Unused-app limits in days: 1, 3, 6 months and 1 year.
pub const UNUSED_DAYS: [(u64, &str); 4] =
    [(30, "1 month"), (91, "3 months"), (182, "6 months"), (365, "1 year")];

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub scan_system: bool,
    pub scan_downloads: bool,
    pub scan_external: bool,
    pub scan_large_files: bool,
    pub large_file_idx: usize,
    /// Look for large files on the whole startup disk, not only the home folder.
    /// Off by default: it takes much longer.
    pub large_whole_disk: bool,
    pub dup_min_idx: usize,
    pub dup_ignore_hidden: bool,
    pub dup_to_trash: bool,
    pub large_app_idx: usize,
    pub unused_idx: usize,
    pub photos_to_trash: bool,
    /// Analyze photos also looks in the Photos app library (macOS); photos to remove from it are
    /// put in an album in Photos, never deleted as files.
    pub photos_library: bool,
    /// Settings › Trash: empty Trash items older than TRASH_DAYS[trash_days_idx] on start.
    pub trash_auto: bool,
    pub trash_days_idx: usize,
    /// Settings › Alerts.
    pub alert_clutter: bool,
    pub alert_clutter_idx: usize,
    pub alert_disk: bool,
    pub alert_disk_idx: usize,
    /// Settings › Browser Cleaning: browsers left out of scans.
    pub browsers_off: Vec<String>,
    /// Settings › Browser Cleaning: delete only history older than
    /// HISTORY_AGES[history_age_idx]; per-browser schedules; sites whose cookies are kept.
    pub history_older: bool,
    pub history_age_idx: usize,
    pub schedules: std::collections::BTreeMap<String, crate::schedule::Schedule>,
    pub allowlist: Vec<String>,
    /// Settings › General › Appearance: 0 Original, 1 Warm (Claude-inspired).
    /// Settings › General › Appearance: 0 match the system's light/dark (default), 1 light, 2 dark.
    pub appearance: u8,
    /// Ask GitHub for a newer release when Claybroom starts. Off by default (nothing is sent
    /// anywhere unless the person turns it on).
    pub check_updates: bool,
    /// Settings › General: keep an icon in the menu bar / system tray, and keep running there
    /// when the window is closed.
    pub tray: bool,
    /// "Find duplicates with Claybroom" on folders' right-click menu.
    pub folder_action: bool,
}

pub const TRASH_DAYS: [u64; 5] = [1, 3, 7, 14, 30];
pub const CLUTTER_ALERT: [u64; 4] = [1_000_000_000, 2_000_000_000, 5_000_000_000, 10_000_000_000];
pub const DISK_ALERT: [u64; 4] = [5_000_000_000, 10_000_000_000, 20_000_000_000, 50_000_000_000];

// Default settings.
impl Default for Settings {
    fn default() -> Self {
        Self {
            scan_system: true,
            scan_downloads: true,
            scan_external: true,
            scan_large_files: true,
            large_file_idx: 2, // 1 GB
            large_whole_disk: false,
            dup_min_idx: 1,    // 500 KB
            dup_ignore_hidden: true,
            dup_to_trash: true, // Trash is safer for personal files
            large_app_idx: 1, // 1 GB
            unused_idx: 2,    // 6 months
            photos_to_trash: true,
            photos_library: true,
            trash_auto: false,     // off, 7 days
            trash_days_idx: 2,
            alert_clutter: true, // on, 2 GB
            alert_clutter_idx: 1,
            alert_disk: true, // on, 10 GB
            alert_disk_idx: 1,
            browsers_off: vec![],
            history_older: false, // off, 1 day
            history_age_idx: 0,
            schedules: Default::default(), // NEVER
            allowlist: vec![],
            appearance: 0,
            check_updates: false,
            tray: true,
            folder_action: true,
        }
    }
}

// ---------- results ----------

#[derive(Clone, Default)]
pub struct Item {
    pub path: PathBuf,
    pub label: String,
    pub size: u64,
    pub selected: bool,
    /// Clean the folder's contents but keep the folder itself (app caches expect it).
    pub keep_dir: bool,
    pub excludes: Vec<PathBuf>,
    pub modified: Option<SystemTime>,
    /// Browser this belongs to, and its process names: it must be closed to clean.
    pub app: Option<&'static str>,
    pub procs: &'static [&'static str],
    /// A browser database that can be edited instead of deleted.
    pub db: crate::browser_pro::Db,
}

#[derive(Clone)]
pub struct Category {
    pub title: String,
    pub tip: &'static str,
    pub items: Vec<Item>,
    pub to_trash: bool,
    pub open: bool,
    /// Browser data (for Statistics).
    pub browser: bool,
}

impl Category {
    pub fn size(&self) -> u64 {
        self.items.iter().map(|i| i.size).sum()
    }
    pub fn selected_size(&self) -> u64 {
        self.items.iter().filter(|i| i.selected).map(|i| i.size).sum()
    }
    pub fn selected_count(&self) -> usize {
        self.items.iter().filter(|i| i.selected).count()
    }
}

#[derive(Default, Clone)]
pub struct CleanReport {
    pub freed: u64,
    pub removed: usize,
    pub failed: usize,
    pub skipped: Vec<String>,
    /// Per category: (title, is browser data, bytes freed, items removed).
    pub by_cat: Vec<(String, bool, u64, usize)>,
    /// Things to tell the person afterwards.
    pub notes: Vec<String>,
    /// Items refused for lack of permission (path, bytes still there): offered to an administrator.
    pub needs_admin: Vec<(PathBuf, u64)>,
}

// ---------- helpers ----------

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn protected(p: &Path) -> bool {
    #[cfg(target_os = "macos")]
    let list: &[&str] = &["/System/"];
    #[cfg(windows)]
    let list: &[&str] = &["C:\\Windows\\System32"];
    #[cfg(all(unix, not(target_os = "macos")))]
    let list: &[&str] = &["/usr/", "/bin/", "/sbin/", "/etc/", "/boot/", "/lib/"];
    let s = p.to_string_lossy();
    list.iter().any(|x| s.to_lowercase().starts_with(&x.to_lowercase())) || p.parent().is_none()
}

fn is_hidden(p: &Path) -> bool {
    p.file_name().map_or(false, |n| n.to_string_lossy().starts_with('.'))
}

/// Bytes a file really takes: its length, or less when it's sparse (a virtual disk that says
/// 500 GB but uses 20 GB).
fn used_bytes(m: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        m.len().min(m.blocks() * 512)
    }
    #[cfg(not(unix))]
    {
        m.len()
    }
}

/// Total size of a file or folder; never follows links, skips `excludes`.
pub fn size_of(p: &Path, excludes: &[PathBuf]) -> u64 {
    WalkDir::new(p)
        .follow_root_links(false)
        .into_iter()
        .filter_entry(|e| !excludes.iter().any(|x| e.path().starts_with(x)))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| used_bytes(&m))
        .sum()
}

pub fn fmt_size(b: u64) -> String {
    let b = b as f64;
    match b {
        b if b >= 1e9 => format!("{:.2} GB", b / 1e9),
        b if b >= 1e6 => format!("{:.1} MB", b / 1e6),
        b if b >= 1e3 => format!("{:.0} KB", b / 1e3),
        _ => format!("{} bytes", b as u64),
    }
}

fn children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// One scanning rule: every child of each root becomes an item.
pub(crate) struct Rule {
    roots: Vec<PathBuf>,
    excludes: Vec<&'static str>,
    keep_dir: bool,
    selected: bool,
}

pub(crate) fn rule(roots: Vec<PathBuf>, excludes: Vec<&'static str>, keep_dir: bool, selected: bool) -> Rule {
    Rule { roots, excludes, keep_dir, selected }
}

pub(crate) fn run_rules(rules: Vec<Rule>, p: &Progress) -> Vec<Item> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for r in rules {
        for root in r.roots {
            let Ok(root) = root.canonicalize() else { continue };
            if !seen.insert(root.clone()) || protected(&root) {
                continue;
            }
            say(p, format!("Scanning {}", root.display()));
            let excludes: Vec<PathBuf> = r.excludes.iter().map(|x| root.join(x)).collect();
            let kids = children(&root);
            let n = kids.len().max(1) as f32;
            for (k, c) in kids.into_iter().enumerate() {
                {
                    // Move the ring within this step as folders are measured.
                    let mut st = p.lock().unwrap();
                    st.frac = (st.base + st.span * k as f32 / n).max(st.frac).min(1.0);
                }
                // Never follow a link out of the folder being cleaned.
                if fs::symlink_metadata(&c).map_or(true, |m| m.file_type().is_symlink()) {
                    continue;
                }
                if excludes.contains(&c) || c.file_name().map_or(false, |n| n == ".DS_Store" || n == "desktop.ini") {
                    continue;
                }
                let ex: Vec<PathBuf> = excludes.iter().filter(|x| x.starts_with(&c)).cloned().collect();
                let size = size_of(&c, &ex);
                if size == 0 && r.keep_dir {
                    continue;
                }
                let label = c.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                add_found(p, size);
                let modified = fs::symlink_metadata(&c).and_then(|m| m.modified()).ok();
                items.push(Item { path: c, label, size, selected: r.selected, keep_dir: r.keep_dir, excludes: ex, modified, ..Default::default() });
            }
        }
    }
    items.sort_by(|a, b| b.size.cmp(&a.size));
    items
}

#[cfg(target_os = "macos")]
fn glob_children(dir: PathBuf, sub: &str) -> Vec<PathBuf> {
    children(&dir).into_iter().map(|c| c.join(sub)).collect()
}

#[cfg_attr(not(windows), allow(dead_code))]
fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

// ---------- clutter ----------

pub fn scan_clutter(s: &Settings, p: &Progress) -> Vec<Category> {
    let h = home();
    let mut cats = Vec::new();
    let stages = 5.0 + s.scan_external as u8 as f32 + s.scan_large_files as u8 as f32;
    let mut push = |title: &str, tip: &'static str, to_trash: bool, rules: Vec<Rule>, p: &Progress| {
        say(p, format!("Scanning {title}"));
        {
            let mut st = p.lock().unwrap();
            st.base = st.frac;
            st.span = 1.0 / stages;
        }
        let items = run_rules(rules, p);
        let next = (p.lock().unwrap().base * stages + 1.0) / stages;
        set_frac(p, next);
        cats.push(Category { title: title.into(), tip, items, to_trash, open: false, browser: false });
    };

    // Application cache
    let mut caches = Vec::new();
    #[cfg(target_os = "macos")]
    {
        caches.push(rule(vec![h.join("Library/Caches")], USER_CACHE_EXCLUDES.to_vec(), true, true));
        caches.push(rule(glob_children(h.join("Library/Containers"), "Data/Library/Caches"), vec![], true, true));
        if s.scan_system {
            caches.push(rule(vec!["/Library/Caches".into()], vec![], true, true));
            let darwin = std::process::Command::new("getconf").arg("DARWIN_USER_CACHE_DIR").output();
            if let Ok(o) = darwin {
                let d = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !d.is_empty() {
                    caches.push(rule(vec![d.into()], vec!["com.apple.FontRegistry", "com.apple.metadata.mdworker"], true, true));
                }
            }
        }
    }
    #[cfg(windows)]
    {
        let mut roots: Vec<PathBuf> = env_path("TEMP").into_iter().collect();
        roots.extend(env_path("LOCALAPPDATA").map(|l| l.join("Temp")));
        caches.push(rule(roots, vec![], true, true));
        if s.scan_system {
            caches.push(rule(vec![env_path("SystemRoot").unwrap_or("C:\\Windows".into()).join("Temp")], vec![], true, true));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        caches.push(rule(vec![h.join(".cache")], vec!["fontconfig"], true, true));
        if s.scan_system {
            caches.push(rule(vec!["/var/cache".into()], vec![], true, true));
        }
    }
    push("App caches", "Files apps keep to start faster. They are rebuilt when needed, so they're safe to remove.", false, caches, p);

    // Log files
    let mut logs = Vec::new();
    #[cfg(target_os = "macos")]
    {
        logs.push(rule(vec![h.join("Library/Logs")], USER_LOG_EXCLUDES.to_vec(), true, true));
        logs.push(rule(glob_children(h.join("Library/Containers"), "Data/Library/Logs"), vec![], true, true));
        if s.scan_system {
            logs.push(rule(vec!["/Library/Logs".into()], vec![], true, true));
        }
    }
    #[cfg(windows)]
    {
        logs.push(rule(env_path("LOCALAPPDATA").map(|l| l.join("CrashDumps")).into_iter().collect(), vec![], true, true));
        if s.scan_system {
            logs.push(rule(vec![env_path("SystemRoot").unwrap_or("C:\\Windows".into()).join("Logs")], vec![], true, true));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    if s.scan_system {
        logs.push(rule(vec!["/var/log".into()], vec![], true, true));
    }
    push("Log Files", "Records of activity, errors and crashes. Useful for fixing problems, otherwise just taking space.", false, logs, p);

    // Trash
    #[cfg(target_os = "macos")]
    let trash = vec![rule(vec![h.join(".Trash")], vec![], false, true)];
    #[cfg(windows)]
    let trash = vec![rule(
        ('A'..='Z')
            .flat_map(|d| children(Path::new(&format!("{d}:\\$Recycle.Bin"))))
            .collect(),
        vec![],
        false,
        true,
    )];
    #[cfg(all(unix, not(target_os = "macos")))]
    let trash = vec![rule(vec![h.join(".local/share/Trash/files"), h.join(".local/share/Trash/info")], vec![], false, true)];
    push("Trash", "Files you already moved to the Trash but haven't deleted yet.", false, trash, p);

    // Downloads (personal files: never preselected, go to the Trash)
    let mut dl = Vec::new();
    if s.scan_downloads {
        dl.push(rule(dirs::download_dir().into_iter().collect(), vec![], false, false));
    }
    #[cfg(target_os = "macos")]
    dl.push(rule(
        [
            "MailDownloads",
            "Mail Downloads",
            "Library/MailDownloads",
            "Library/Mail Downloads",
            "Library/Containers/com.apple.mail/Data/MailDownloads",
            "Library/Containers/com.apple.mail/Data/Mail Downloads",
            "Library/Containers/com.apple.mail/Data/Library/Mail Downloads",
        ]
        .iter()
        .map(|x| h.join(x))
        .collect(),
        vec![],
        false,
        false,
    ));
    push("Downloads", "Everything in your Downloads folder. Check before removing: these are your files.", true, dl, p);

    // Development junk
    #[cfg_attr(all(unix, not(target_os = "macos")), allow(unused_mut))]
    let mut dev = vec![rule(vec![h.join(".npm/_cacache")], vec![], false, true)];
    #[cfg(windows)]
    dev.push(rule(env_path("LOCALAPPDATA").map(|l| l.join("npm-cache/_cacache")).into_iter().collect(), vec![], false, true));
    #[cfg(target_os = "macos")]
    dev.push(rule(
        vec![h.join("Library/Developer/Xcode/DerivedData"), h.join("Library/Developer/Xcode/Simulators")],
        vec![],
        false,
        true,
    ));
    push("Developer leftovers", "Temporary build files left by developer tools. They are rebuilt when needed.", false, dev, p);

    #[cfg(target_os = "macos")]
    if s.scan_external {
        let items = scan_external(p);
        cats.push(Category {
            title: "Junk on external drives".into(),
            tip: "Helper files macOS leaves on external drives (.DS_Store, ._ files, drive trash).",
            items,
            to_trash: false,
            open: false,
            browser: false,
        });
    }

    if s.scan_large_files {
        let min = LARGE_FILE_SIZES[s.large_file_idx.min(5)];
        let items = if s.large_whole_disk { scan_large_in(&computer_root(), &h, min, p) } else { scan_large(&h, min, p) };
        cats.push(Category {
            title: "Large Files".into(),
            tip: if s.large_whole_disk { "Big files anywhere on your computer outside the system, over the size set in Settings." } else { "Big files in your home folder, over the size set in Settings." },
            items,
            to_trash: true,
            open: false,
            browser: false,
        });
    }
    // Empty categories stay, shown greyed out as "Already clean".
    say(p, "");
    cats
}

#[cfg(target_os = "macos")]
fn scan_external(p: &Progress) -> Vec<Item> {
    let mut out = Vec::new();
    // The original leaves Mac OS Extended (HFS) drives alone (config.json UnsupportedFS ["hfs"]).
    let hfs_vols: Vec<PathBuf> = sysinfo::Disks::new_with_refreshed_list()
        .map(|d| d.iter().filter(|d| d.file_system().to_string_lossy().eq_ignore_ascii_case("hfs")).map(|d| d.mount_point().to_path_buf()).collect())
        .unwrap_or_default();
    let hfs = |vol: &Path| hfs_vols.iter().any(|v| v == vol);
    for vol in children(Path::new("/Volumes")) {
        if fs::symlink_metadata(&vol).map_or(true, |m| m.file_type().is_symlink()) {
            continue; // the startup disk is a link to "/"
        }
        if hfs(&vol) {
            continue;
        }
        say(p, format!("Scanning {}", vol.display()));
        let trashes = vol.join(".Trashes");
        for c in children(&trashes) {
            if c.file_name().map_or(false, |n| n == "MobileBackups") {
                continue;
            }
            out.push(Item { size: size_of(&c, &[]), label: c.display().to_string(), path: c, selected: true, keep_dir: false, excludes: vec![], ..Default::default() });
        }
        let mut it = WalkDir::new(&vol).into_iter();
        while let Some(Ok(e)) = it.next() {
            if e.path() == trashes {
                it.skip_current_dir();
                continue;
            }
            let name = e.file_name().to_string_lossy();
            let dir_junk = e.file_type().is_dir() && is_drive_junk(&name, true);
            let file_junk = e.file_type().is_file() && is_drive_junk(&name, false);
            if dir_junk || file_junk {
                let path = e.path().to_path_buf();
                out.push(Item { size: size_of(&path, &[]), label: path.display().to_string(), path, selected: true, keep_dir: false, excludes: vec![], ..Default::default() });
                if dir_junk {
                    it.skip_current_dir();
                }
            }
        }
    }
    out
}

pub(crate) fn scan_large(home: &Path, min: u64, p: &Progress) -> Vec<Item> {
    scan_large_in(home, home, min, p)
}

/// The startup disk: `/`, or the Windows system drive.
pub fn computer_root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("{}\\", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())))
    } else {
        PathBuf::from("/")
    }
}

/// Operating-system folders a whole-disk search leaves alone (only those that exist matter). On
/// macOS `/System` also holds the data volume, which `/Users` already reaches.
const SYSTEM_DIRS: [&str; 30] = [
    "System", "Library", "private", "dev", "Volumes", "usr", "bin", "sbin", "opt", "cores", "Applications", "Network",
    "proc", "sys", "run", "lib", "lib64", "boot", "etc", "var", "snap", "tmp", "media", "mnt", "lost+found",
    "Windows", "Program Files", "Program Files (x86)", "ProgramData", "$Recycle.Bin",
];

pub(crate) fn scan_large_in(root: &Path, home: &Path, min: u64, p: &Progress) -> Vec<Item> {
    let mut skip: Vec<PathBuf> = ["Applications", "Library", "AppData", ".Trash", ".local/share/Trash"]
        .iter()
        .map(|x| home.join(x))
        .collect();
    if root != home {
        skip.extend(SYSTEM_DIRS.iter().map(|d| root.join(d)));
        skip.extend(["System Volume Information", "Recovery"].iter().map(|d| root.join(d)));
    }
    // ponytail: package and git stores are huge to walk and never hold the person's large files.
    let skip_names = ["node_modules", ".git", ".npm", ".cargo", ".rustup", "uv", "pip", ".gradle", ".m2"];
    let base = p.lock().unwrap().frac;
    let tops = children(root);
    let n = tops.len().max(1) as f32;
    let mut out = Vec::new();
    for (k, top) in tops.into_iter().enumerate() {
        set_frac(p, base + (1.0 - base) * k as f32 / n);
        if skip.contains(&top) {
            continue;
        }
        // Stay on this disk: other drives and network shares aren't "this computer".
        let mut it = WalkDir::new(&top).same_file_system(true).into_iter();
        while let Some(e) = it.next() {
            let Ok(e) = e else { continue };
            if e.file_type().is_dir() {
                if skip_names.iter().any(|x| e.file_name() == *x) || skip.iter().any(|s| s == e.path()) {
                    it.skip_current_dir();
                } else if e.depth() <= 2 {
                    say(p, format!("Looking for large files in {}", e.path().display()));
                }
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            if e.file_type().is_file() && m.len() >= min {
                add_found(p, m.len());
                out.push(Item {
                    label: e.path().strip_prefix(home).unwrap_or(e.path()).display().to_string(),
                    path: e.path().to_path_buf(),
                    size: m.len(),
                    modified: m.modified().ok(),
                    ..Default::default()
                });
            }
        }
    }
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

// ---------- automatic Trash emptying ----------

/// When an item went into the Trash. Moving a file changes its inode time on macOS and Linux;
/// on Windows the Recycle Bin writes a small "$I" file next to each "$R" item at deletion.
fn trashed_at(p: &Path) -> Option<SystemTime> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(p).ok()?;
        Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(m.ctime().max(0) as u64))
    }
    #[cfg(windows)]
    {
        let name = p.file_name()?.to_string_lossy().into_owned();
        let info = p.with_file_name(name.replacen("$R", "$I", 1));
        fs::metadata(info).and_then(|m| m.modified()).ok()
    }
}

/// Settings › Trash: delete what has been in the Trash longer than `days`.
pub fn empty_old_trash(days: u64) -> CleanReport {
    #[cfg_attr(windows, allow(unused_variables))]
    let h = home();
    #[cfg(target_os = "macos")]
    let roots = vec![h.join(".Trash")];
    #[cfg(windows)]
    let roots: Vec<PathBuf> = ('A'..='Z').flat_map(|d| children(Path::new(&format!("{d}:\\$Recycle.Bin")))).collect();
    #[cfg(all(unix, not(target_os = "macos")))]
    let roots = vec![h.join(".local/share/Trash/files")];
    let limit = SystemTime::now() - std::time::Duration::from_secs(days * 86_400);
    let mut r = CleanReport::default();
    for root in roots {
        for c in children(&root) {
            let name = c.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if name == ".DS_Store" || name == "desktop.ini" || name.starts_with("$I") {
                continue;
            }
            if trashed_at(&c).is_some_and(|t| t < limit) {
                remove_path(&c, false, &mut r);
                #[cfg(all(unix, not(target_os = "macos")))]
                let _ = fs::remove_file(h.join(".local/share/Trash/info").join(format!("{name}.trashinfo")));
                #[cfg(windows)]
                let _ = fs::remove_file(c.with_file_name(name.replacen("$R", "$I", 1)));
            }
        }
    }
    r
}

// ---------- browsers ----------

pub struct Browser {
    pub name: &'static str,
    pub cats: Vec<Category>,
}

pub(crate) struct BrowserDef {
    pub name: &'static str,
    procs: &'static [&'static str],
    pub(crate) chromium: bool,
    /// Profile root and cache root, per OS (relative to home or to an env var on Windows).
    pub(crate) data: Option<PathBuf>,
    cache: Option<PathBuf>,
}

pub(crate) fn browser_defs() -> Vec<BrowserDef> {
    let h = home();
    let mut v = Vec::new();
    let mut add = |name, procs, chromium, data: Option<PathBuf>, cache: Option<PathBuf>| {
        v.push(BrowserDef { name, procs, chromium, data, cache })
    };
    #[cfg(target_os = "macos")]
    {
        let sup = h.join("Library/Application Support");
        let cac = h.join("Library/Caches");
        add("Safari", &["safari"][..], false, Some(h.join("Library/Safari")), Some(cac.join("com.apple.Safari")));
        add("Chrome", &["google chrome"], true, Some(sup.join("Google/Chrome")), Some(cac.join("Google/Chrome")));
        add("Firefox", &["firefox"], false, Some(sup.join("Firefox/Profiles")), Some(cac.join("Firefox/Profiles")));
        add("Opera", &["opera"], true, Some(sup.join("com.operasoftware.Opera")), Some(cac.join("com.operasoftware.Opera")));
        add("Edge", &["microsoft edge"], true, Some(sup.join("Microsoft Edge")), Some(cac.join("Microsoft Edge")));
        add("Brave", &["brave browser"], true, Some(sup.join("BraveSoftware/Brave-Browser")), Some(cac.join("BraveSoftware/Brave-Browser")));
        add("Vivaldi", &["vivaldi"], true, Some(sup.join("Vivaldi")), Some(cac.join("Vivaldi")));
    }
    #[cfg(windows)]
    {
        let local = env_path("LOCALAPPDATA").unwrap_or(h.join("AppData/Local"));
        let roam = env_path("APPDATA").unwrap_or(h.join("AppData/Roaming"));
        add("Chrome", &["chrome"][..], true, Some(local.join("Google/Chrome/User Data")), None);
        add("Firefox", &["firefox"], false, Some(roam.join("Mozilla/Firefox/Profiles")), Some(local.join("Mozilla/Firefox/Profiles")));
        add("Opera", &["opera"], true, Some(roam.join("Opera Software/Opera Stable")), Some(local.join("Opera Software/Opera Stable")));
        add("Edge", &["msedge"], true, Some(local.join("Microsoft/Edge/User Data")), None);
        add("Brave", &["brave"], true, Some(local.join("BraveSoftware/Brave-Browser/User Data")), None);
        add("Vivaldi", &["vivaldi"], true, Some(local.join("Vivaldi/User Data")), None);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let cfg = h.join(".config");
        let cac = h.join(".cache");
        add("Chrome", &["chrome", "google-chrome"][..], true, Some(cfg.join("google-chrome")), Some(cac.join("google-chrome")));
        add("Firefox", &["firefox", "firefox-bin", "firefox-esr"], false, Some(h.join(".mozilla/firefox")), Some(cac.join("mozilla/firefox")));
        add("Opera", &["opera"], true, Some(cfg.join("opera")), Some(cac.join("opera")));
        add("Edge", &["msedge", "microsoft-edge"], true, Some(cfg.join("microsoft-edge")), Some(cac.join("microsoft-edge")));
        add("Brave", &["brave", "brave-browser"], true, Some(cfg.join("BraveSoftware/Brave-Browser")), Some(cac.join("BraveSoftware/Brave-Browser")));
        add("Vivaldi", &["vivaldi", "vivaldi-bin"], true, Some(cfg.join("vivaldi")), Some(cac.join("vivaldi")));
    }
    v
}

fn file_items(paths: impl IntoIterator<Item = PathBuf>, selected: bool, keep_dir: bool) -> Vec<Item> {
    paths
        .into_iter()
        .filter(|p| p.exists())
        .map(|p| {
            let label = p.iter().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect::<PathBuf>().display().to_string();
            Item { size: size_of(&p, &[]), label, path: p, selected, keep_dir, excludes: vec![], ..Default::default() }
        })
        .filter(|i| i.size > 0)
        .collect()
}

/// "Saved form data" and "Autofill passwords":
/// never ticked for you, never part of scheduled cleaning.
pub(crate) fn browser_private_cats(autofill: Vec<Item>, passwords: Vec<Item>) -> Vec<Category> {
    let mk = |title: &str, tip, items| Category { title: title.into(), tip, items, to_trash: false, open: false, browser: false };
    vec![
        mk(AUTOFILL, "Addresses, cards and form entries the browser fills in for you.", autofill),
        mk(PASSWORDS, "Passwords the browser saved. Removing them means typing them again (your password manager is not touched).", passwords),
    ]
    .into_iter()
    .filter(|c| !c.items.is_empty())
    .collect()
}

pub const AUTOFILL: &str = "Saved form data";
pub const PASSWORDS: &str = "Autofill passwords";

pub(crate) fn browser_cats(cookies: Vec<Item>, caches: Vec<Item>, history: Vec<Item>) -> Vec<Category> {
    let mk = |title: &str, tip, items| Category { title: title.into(), tip, items, to_trash: false, open: false, browser: false };
    vec![
        mk("Caches", "Temporary files from sites you've visited, kept to load them faster.", caches),
        mk("Browsing history", "Your visited sites, searches and downloads list.", history),
        mk("Cookies", "Small files sites use to remember you. Removing them signs you out of websites.", cookies),
    ]
    .into_iter()
    .filter(|c| !c.items.is_empty())
    .collect()
}

pub fn scan_browsers(p: &Progress, off: &[String]) -> Vec<Browser> {
    scan_browsers_in(p, 0.0, 1.0, off)
}

/// Scan browsers, moving the progress ring from `base` over `span`.
fn scan_browsers_in(p: &Progress, base: f32, span: f32, off: &[String]) -> Vec<Browser> {
    let mut out = Vec::new();
    let defs = browser_defs();
    let n = defs.len() as f32;
    for (i, d) in defs.into_iter().enumerate().filter(|(_, d)| !off.iter().any(|o| o == d.name)) {
        set_frac(p, base + span * i as f32 / n);
        let Some(data) = d.data.clone().filter(|x| x.exists()) else { continue };
        say(p, format!("Scanning {}", d.name));
        let (mut cookies, mut caches, mut history, mut autofill, mut passwords) = (vec![], vec![], vec![], vec![], vec![]);
        if d.chromium {
            let mut profiles: Vec<PathBuf> = children(&data).into_iter().filter(|c| c.join("Preferences").is_file()).collect();
            if data.join("Preferences").is_file() {
                profiles.push(data.clone());
            }
            for pr in profiles {
                let j = |xs: &[&str]| xs.iter().map(|x| pr.join(x)).collect::<Vec<_>>();
                cookies.extend(file_items(j(&["Cookies", "Cookies-journal", "Network/Cookies", "Network/Cookies-journal", "Extension Cookies"]), false, false));
                history.extend(file_items(j(&["History", "History-journal", "Visited Links", "Top Sites", "Top Sites-journal", "Shortcuts", "Shortcuts-journal"]), true, false));
                caches.extend(file_items(j(&["Cache", "Code Cache", "GPUCache", "Service Worker/CacheStorage"]), true, true));
                // Edited table by table: search engines and settings in the same files stay.
                autofill.extend(file_items(j(&["Web Data"]), false, false));
                passwords.extend(file_items(j(&["Login Data", "Login Data For Account"]), false, false));
                if let (Some(c), Ok(rel)) = (&d.cache, pr.strip_prefix(&data)) {
                    caches.extend(file_items([c.join(rel)], true, true));
                }
            }
        } else if d.name == "Firefox" {
            for pr in children(&data).into_iter().filter(|c| c.join("prefs.js").is_file()) {
                let j = |xs: &[&str]| xs.iter().map(|x| pr.join(x)).collect::<Vec<_>>();
                cookies.extend(file_items(j(&["cookies.sqlite", "cookies.sqlite-wal", "cookies.sqlite-shm"]), false, false));
                // places.sqlite also holds bookmarks: it is edited (visits removed), never deleted.
                history.extend(file_items(j(&["places.sqlite", "sessionstore-backups"]), true, false));
                autofill.extend(file_items(j(&["formhistory.sqlite"]), false, false));
                // key4.db (the key) stays, so a password saved later still works.
                passwords.extend(file_items(j(&["logins.json", "logins-backup.json"]), false, false));
                caches.extend(file_items(j(&["startupCache", "shader-cache"]), true, true));
                if let Some(c) = &d.cache {
                    caches.extend(file_items([c.join(pr.file_name().unwrap()).join("cache2")], true, true));
                }
            }
        } else {
            // Safari
            let h = home();
            let sc = h.join("Library/Containers/com.apple.Safari/Data/Library");
            caches.extend(file_items([d.cache.clone().unwrap(), sc.join("Caches")], true, true));
            history.extend(file_items(["History.db", "History.db-wal", "History.db-shm"].iter().map(|x| data.join(x)), true, false));
            cookies.extend(file_items([sc.join("Cookies/Cookies.binarycookies"), h.join("Library/Cookies/Cookies.binarycookies")], false, false));
        }
        let mut cats = browser_cats(cookies, caches, history);
        cats.extend(browser_private_cats(autofill, passwords));
        for c in cats.iter_mut() {
            c.browser = true;
            for i in c.items.iter_mut() {
                i.app = Some(d.name);
                i.procs = d.procs;
                i.db = db_kind(&i.path, d.chromium);
                add_found(p, i.size);
            }
        }
        if !cats.is_empty() {
            out.push(Browser { name: d.name, cats });
        }
    }
    say(p, "");
    out
}

/// Quick Clean: caches, logs and the Trash, plus every browser's caches, history and cookies.
pub fn scan_quick(s: &Settings, p: &Progress) -> Vec<Category> {
    scan_junk(s, p, false)
}

/// Quick Clean's list, or with `everything` every Clean clutter category too (Super Clean),
/// with browser data merged in and nothing counted twice.
pub fn scan_junk(s: &Settings, p: &Progress, everything: bool) -> Vec<Category> {
    let q = if everything { s.clone() } else { Settings { scan_downloads: false, scan_large_files: false, scan_external: false, ..s.clone() } };
    let mut cats: Vec<Category> = scan_clutter(&q, p)
        .into_iter()
        .filter(|c| everything || ["App caches", "Log Files", "Trash"].contains(&c.title.as_str()))
        .collect();
    let scale = 0.6; // clutter is the first 60% of the ring
    set_frac(p, scale);
    // Browser caches under ~/Library/Caches are already in "App caches": don't count them twice.
    let taken: Vec<PathBuf> = cats.iter().flat_map(|c| c.items.iter().map(|i| i.path.clone())).collect();
    let mut merged = [
        ("Cookies", "Small files sites use to remember you. Removing them signs you out.", vec![]),
        ("Web cache", "Temporary files from sites you've visited, kept to load them faster.", vec![]),
        ("Browsing history", "Your visited sites, searches and downloads list.", vec![]),
        (AUTOFILL, "Addresses, cards and form entries the browser fills in for you.", vec![]),
        (PASSWORDS, "Passwords the browser saved. Removing them means typing them again.", vec![]),
    ];
    for b in scan_browsers_in(p, scale, 1.0 - scale, &s.browsers_off) {
        for c in b.cats {
            let slot = match c.title.as_str() {
                "Cookies" => 0,
                "Caches" => 1,
                AUTOFILL => 3,
                PASSWORDS => 4,
                _ => 2,
            };
            for mut i in c.items.into_iter().filter(|i| !taken.iter().any(|t| i.path.starts_with(t))) {
                i.label = format!("{} · {}", b.name, i.label);
                merged[slot].2.push(i);
            }
        }
    }
    for (title, tip, mut items) in merged {
        items.sort_by(|a, b| b.size.cmp(&a.size));
        cats.push(Category { title: title.into(), tip, items, to_trash: false, open: false, browser: true });
    }
    set_frac(p, 1.0);
    cats
}

/// macOS hides the Trash, Mail and some browsers' data until the app has Full Disk Access.
pub fn needs_full_disk_access() -> bool {
    cfg!(target_os = "macos") && fs::read_dir(home().join(".Trash")).is_err()
}

pub fn open_full_disk_access_settings() {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .status();
}

/// Leave out items whose browser is running; returns what can be cleaned and who to close.
pub fn without_running(cats: &[Category]) -> (Vec<Category>, Vec<String>) {
    let running = running_processes();
    let open = |i: &Item| i.procs.iter().any(|n| running.iter().any(|r| r == n));
    let mut skipped: Vec<String> = vec![];
    let mut ok = vec![];
    for c in cats {
        let mut c = c.clone();
        for i in c.items.iter().filter(|i| i.selected && open(i)) {
            let name = i.app.unwrap_or("a browser").to_string();
            if !skipped.contains(&name) {
                skipped.push(name);
            }
        }
        c.items.retain(|i| !open(i));
        ok.push(c);
    }
    (ok, skipped)
}

/// Names of running processes, lowercased and without ".exe".
pub fn running_processes() -> Vec<String> {
    let sys = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::nothing()),
    );
    let Ok(sys) = sys else { return vec![] };
    sys.processes()
        .values()
        .map(|p| p.name().to_string_lossy().to_lowercase().trim_end_matches(".exe").to_string())
        .collect()
}

// ---------- duplicates ----------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Pictures,
    Songs,
    Videos,
    Documents,
    Others,
    /// Whole folders with the same contents.
    Folders,
}

pub fn kind_of(p: &Path) -> Kind {
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "heic" | "heif" | "bmp" | "tif" | "tiff" | "webp" | "raw" | "cr2" | "nef" | "dng" | "svg" => Kind::Pictures,
        "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "aiff" | "wma" | "opus" => Kind::Songs,
        "mp4" | "mov" | "m4v" | "avi" | "mkv" | "wmv" | "webm" | "mpg" | "mpeg" => Kind::Videos,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt" | "ods" | "odp" | "pages" | "numbers" | "key" | "md" | "csv" | "epub" => Kind::Documents,
        _ => Kind::Others,
    }
}

#[derive(Clone)]
pub struct DupFile {
    pub path: PathBuf,
    pub modified: SystemTime,
    pub selected: bool,
}

#[derive(Clone)]
pub struct DupGroup {
    pub size: u64,
    pub kind: Kind,
    pub files: Vec<DupFile>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Preselect {
    KeepNewest,
    KeepOldest,
    None,
}

pub fn preselect(groups: &mut [DupGroup], how: Preselect) {
    for g in groups {
        let keep = match how {
            Preselect::KeepNewest => g.files.iter().enumerate().max_by_key(|(_, f)| f.modified).map(|x| x.0),
            Preselect::KeepOldest => g.files.iter().enumerate().min_by_key(|(_, f)| f.modified).map(|x| x.0),
            Preselect::None => None,
        };
        for (i, f) in g.files.iter_mut().enumerate() {
            f.selected = how != Preselect::None && Some(i) != keep;
        }
    }
}

fn hash_file(p: &Path) -> Option<[u8; 32]> {
    let mut h = blake3::Hasher::new();
    h.update_reader(fs::File::open(p).ok()?).ok()?;
    Some(*h.finalize().as_bytes())
}

/// The first 64 KB's hash: files that differ there can't be copies, so most same-size files are
/// told apart without reading them whole.
fn hash_head(p: &Path) -> Option<[u8; 32]> {
    use std::io::Read;
    let mut buf = Vec::with_capacity(65536);
    fs::File::open(p).ok()?.take(65536).read_to_end(&mut buf).ok()?;
    Some(*blake3::hash(&buf).as_bytes())
}

/// Files every folder has that differ by themselves and say nothing about its contents.
const FOLDER_NOISE: [&str; 3] = [".DS_Store", "Thumbs.db", "desktop.ini"];

/// Folders with the same contents: every file inside, small
/// ones too, has an identical twin at the same place in the other folder. Only the top-most
/// matches are kept (two copied projects show once, not once per subfolder).
fn match_folders(groups: &[DupGroup], roots: &[PathBuf], min: u64, ignore_hidden: bool, p: &Progress) -> Vec<DupGroup> {
    say(p, "Matching folders…");
    // Each duplicate file's content: the group it is in.
    let id: HashMap<&Path, usize> = groups.iter().enumerate().flat_map(|(gi, g)| g.files.iter().map(move |f| (f.path.as_path(), gi))).collect();
    // Folders holding a duplicate, below the folders the person chose.
    let mut cands = std::collections::BTreeSet::new();
    for f in id.keys() {
        for a in f.ancestors().skip(1) {
            if roots.iter().any(|r| a == r.as_path() || !a.starts_with(r)) {
                break;
            }
            cands.insert(a.to_path_buf());
        }
    }
    let mut by_sig: HashMap<[u8; 32], Vec<(PathBuf, u64)>> = HashMap::new();
    for dir in cands {
        if let Some((sig, size)) = folder_sig(&dir, &id, min, ignore_hidden) {
            by_sig.entry(sig).or_default().push((dir, size));
        }
    }
    let mut found: Vec<Vec<(PathBuf, u64)>> = by_sig.into_values().filter(|v| v.len() > 1).collect();
    let all: Vec<PathBuf> = found.iter().flatten().map(|x| x.0.clone()).collect();
    found.retain(|g| !g.iter().all(|(d, _)| all.iter().any(|t| t != d && d.starts_with(t))));
    found
        .into_iter()
        .map(|v| DupGroup {
            size: v[0].1,
            kind: Kind::Folders,
            files: v
                .into_iter()
                .map(|(path, _)| DupFile { modified: fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH), path, selected: false })
                .collect(),
        })
        .collect()
}

/// A folder's contents as one fingerprint (each file's place and content), and its size. None
/// when it holds a file with no twin anywhere, a link, or something unreadable.
fn folder_sig(dir: &Path, id: &HashMap<&Path, usize>, min: u64, ignore_hidden: bool) -> Option<([u8; 32], u64)> {
    let (mut entries, mut small, mut size) = (Vec::new(), Vec::new(), 0u64);
    let walk = WalkDir::new(dir).min_depth(1).into_iter().filter_entry(|e| !(ignore_hidden && is_hidden(e.path())));
    for e in walk {
        let e = e.ok()?;
        let name = e.file_name().to_string_lossy();
        if e.file_type().is_symlink() {
            return None;
        }
        if !e.file_type().is_file() || FOLDER_NOISE.contains(&name.as_ref()) {
            continue;
        }
        if entries.len() + small.len() > 20_000 {
            return None;
        }
        let len = e.metadata().ok()?.len();
        size += len;
        let rel = e.path().strip_prefix(dir).ok()?.to_string_lossy().into_owned();
        match id.get(e.path()) {
            Some(gi) => {
                let mut c = [0u8; 32];
                c[..8].copy_from_slice(&(*gi as u64).to_le_bytes());
                entries.push((rel, c));
            }
            // Small files weren't compared: read them now (only for folders that could match).
            None if len < min => small.push((rel, e.path().to_path_buf())),
            None => return None,
        }
    }
    if entries.is_empty() {
        return None;
    }
    for (rel, path) in small {
        entries.push((rel, hash_file(&path)?));
    }
    entries.sort();
    let mut h = blake3::Hasher::new();
    for (rel, c) in &entries {
        h.update(rel.as_bytes());
        h.update(&[0]);
        h.update(c);
    }
    Some((*h.finalize().as_bytes(), size))
}

/// The copies among files of one size: same first 64 KB, then the same whole content.
fn match_same_size(size: u64, paths: &[PathBuf]) -> Vec<DupGroup> {
    let mut by_head: HashMap<[u8; 32], Vec<&PathBuf>> = HashMap::new();
    for path in paths {
        if let Some(hash) = hash_head(path) {
            by_head.entry(hash).or_default().push(path);
        }
    }
    let mut by_hash: HashMap<[u8; 32], Vec<&PathBuf>> = HashMap::new();
    for path in by_head.into_values().filter(|v| v.len() > 1).flatten() {
        // Files of 64 KB or less were read whole already.
        let hash = if size <= 65536 { hash_head(path) } else { hash_file(path) };
        if let Some(hash) = hash {
            by_hash.entry(hash).or_default().push(path);
        }
    }
    by_hash
        .into_values()
        .filter(|v| v.len() > 1)
        .map(|paths| {
            let files: Vec<DupFile> = paths
                .into_iter()
                .map(|path| DupFile { modified: fs::metadata(path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH), path: path.clone(), selected: false })
                .collect();
            DupGroup { size, kind: kind_of(&files[0].path), files }
        })
        .collect()
}

pub fn scan_duplicates(folders: &[PathBuf], s: &Settings, p: &Progress) -> Vec<DupGroup> {
    let min = DUP_SIZES[s.dup_min_idx.min(4)];
    let h = home();
    // App data folders hold files apps rely on; duplicates there are not the person's to remove.
    let skip: Vec<PathBuf> = ["Library", "AppData", ".Trash"].iter().map(|x| h.join(x)).collect();
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    for f in folders {
        say(p, format!("Preparing scan… {}", f.display()));
        for e in WalkDir::new(f)
            .into_iter()
            // Package folders hold copies tools made, not the person's files.
            .filter_entry(|e| !skip.contains(&e.path().to_path_buf()) && e.file_name() != "node_modules" && !(s.dup_ignore_hidden && e.depth() > 0 && is_hidden(e.path())))
            .filter_map(Result::ok)
        {
            if e.file_type().is_dir() {
                say(p, e.path().display().to_string());
                continue;
            }
            if !e.file_type().is_file() {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            if m.len() >= min && !protected(e.path()) && seen.insert(e.path().to_path_buf()) {
                by_size.entry(m.len()).or_default().push(e.path().to_path_buf());
            }
        }
    }
    set_frac(p, 0.3);
    let mut candidates: Vec<(u64, Vec<PathBuf>)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
    // Big files first, dealt round-robin to the threads so each gets a fair share of reading.
    candidates.sort_by_key(|c| std::cmp::Reverse(c.0));
    let total = candidates.len();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let done = std::sync::atomic::AtomicUsize::new(0);
    let mut groups: Vec<DupGroup> = std::thread::scope(|sc| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let (candidates, done) = (&candidates, &done);
                sc.spawn(move || {
                    let mut out = Vec::new();
                    for (size, paths) in candidates.iter().skip(t).step_by(threads) {
                        let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        say(p, format!("Matching files… {n}/{total}"));
                        set_frac(p, 0.3 + 0.7 * n as f32 / total.max(1) as f32);
                        out.extend(match_same_size(*size, paths));
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    // Whole folders that are copies; their files aren't listed again on their own.
    let folders_found = match_folders(&groups, folders, min, s.dup_ignore_hidden, p);
    let inside = |f: &Path| folders_found.iter().flat_map(|g| &g.files).any(|d| f.starts_with(&d.path));
    groups.retain(|g| !g.files.iter().all(|f| inside(&f.path)));
    groups.extend(folders_found);
    groups.sort_by(|a, b| (b.size * b.files.len() as u64).cmp(&(a.size * a.files.len() as u64)));
    preselect(&mut groups, Preselect::KeepNewest);
    say(p, "");
    groups
}

// ---------- removing ----------

fn remove_path(path: &Path, to_trash: bool, r: &mut CleanReport) {
    if protected(path) {
        r.failed += 1;
        return;
    }
    let size = size_of(path, &[]);
    // Err(true): the system refused for lack of permission (an administrator could remove it).
    let denied = |e: std::io::Error| e.kind() == std::io::ErrorKind::PermissionDenied;
    let res = if to_trash {
        trash::delete(path).map_err(|_| false)
    } else if fs::symlink_metadata(path).map_or(false, |m| m.is_dir()) {
        fs::remove_dir_all(path).map_err(denied)
    } else {
        fs::remove_file(path).map_err(denied)
    };
    match res {
        Ok(()) => {
            r.freed += size;
            r.removed += 1;
        }
        Err(refused) => {
            r.failed += 1;
            let left = size_of(path, &[]);
            r.freed += size.saturating_sub(left);
            if refused && left > 0 {
                r.needs_admin.push((path.to_path_buf(), left));
            }
        }
    }
}

/// Remove an item. A `keep_dir` item keeps its folder (and excluded parts) and loses the rest.
pub fn clean_item(item: &Item, to_trash: bool, r: &mut CleanReport) {
    if !item.keep_dir || !fs::symlink_metadata(&item.path).map_or(false, |m| m.is_dir()) {
        return remove_path(&item.path, to_trash, r);
    }
    for c in children(&item.path) {
        if item.excludes.iter().any(|x| x == &c) {
            continue;
        }
        if item.excludes.iter().any(|x| x.starts_with(&c)) {
            let sub = Item { path: c, excludes: item.excludes.clone(), ..item.clone() };
            clean_item(&sub, to_trash, r);
        } else {
            remove_path(&c, to_trash, r);
        }
    }
}

/// Which browser database a file is (by name), so it can be edited instead of deleted.
fn db_kind(path: &Path, chromium: bool) -> crate::browser_pro::Db {
    use crate::browser_pro::Db;
    match (chromium, path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().as_str()) {
        (true, "Cookies") => Db::ChromiumCookies,
        (true, "History") => Db::ChromiumHistory,
        (true, "Visited Links" | "Top Sites" | "Shortcuts") => Db::HistoryExtra,
        (true, "Web Data") => Db::ChromiumAutofill,
        (true, "Login Data" | "Login Data For Account") => Db::ChromiumLogins,
        (false, "cookies.sqlite") => Db::FirefoxCookies,
        (false, "places.sqlite") => Db::FirefoxHistory,
        (false, "History.db") => Db::SafariHistory,
        (false, "Cookies.binarycookies") => Db::SafariCookies,
        _ => Db::None,
    }
}

pub fn clean_categories(cats: &[Category], p: &Progress) -> CleanReport {
    clean_categories_with(cats, p, &crate::browser_pro::BrowserOpts::default())
}

/// Clean, editing browser databases where the cookie allowlist or "only older history" asks for it.
pub fn clean_categories_with(cats: &[Category], p: &Progress, o: &crate::browser_pro::BrowserOpts) -> CleanReport {
    use crate::browser_pro::{clean_cookies, clean_history, Db};
    let edit = |i: &Item| match i.db {
        Db::ChromiumCookies | Db::FirefoxCookies => !o.allow.is_empty(),
        Db::ChromiumHistory | Db::SafariHistory => o.history_before.is_some(),
        Db::FirefoxHistory | Db::ChromiumAutofill | Db::ChromiumLogins => true,
        _ => false,
    };
    let edited: Vec<PathBuf> = cats.iter().flat_map(|c| &c.items).filter(|i| i.selected && edit(i)).map(|i| i.path.clone()).collect();
    let mut r = CleanReport::default();
    let total: usize = cats.iter().map(|c| c.selected_count()).sum();
    let mut done = 0;
    for c in cats {
        let (freed, removed) = (r.freed, r.removed);
        for i in c.items.iter().filter(|i| i.selected) {
            done += 1;
            set_frac(p, done as f32 / total.max(1) as f32);
            say(p, format!("Cleaning {}", i.label));
            let name = i.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            // The journal of a database being edited belongs to it: leave it to SQLite.
            let main = ["-journal", "-wal", "-shm"].iter().find_map(|x| name.strip_suffix(x)).map(|b| i.path.with_file_name(b));
            if main.is_some_and(|m| edited.contains(&m)) {
                continue;
            }
            match i.db {
                _ if edit(i) => {
                    let res = if matches!(i.db, Db::ChromiumCookies | Db::FirefoxCookies) {
                        clean_cookies(&i.path, i.db, &o.allow)
                    } else if matches!(i.db, Db::ChromiumAutofill | Db::ChromiumLogins) {
                        crate::browser_pro::clean_rows(&i.path, i.db)
                    } else {
                        clean_history(&i.path, i.db, o.history_before.unwrap_or_else(crate::schedule::now))
                    };
                    match res {
                        Ok(b) => {
                            r.freed += b;
                            r.removed += 1;
                        }
                        Err(_) => r.failed += 1,
                    }
                }
                Db::SafariCookies if !o.allow.is_empty() => match crate::browser_pro::clean_safari_cookies(&i.path, &o.allow) {
                    Ok(b) => {
                        r.freed += b;
                        r.removed += 1;
                    }
                    Err(e) => {
                        r.failed += 1;
                        let note = format!("Safari's cookies were kept: {e}.");
                        if !r.notes.contains(&note) {
                            r.notes.push(note);
                        }
                    }
                },
                Db::HistoryExtra if o.history_before.is_some() => {}
                _ => clean_item(i, c.to_trash, &mut r),
            }
        }
        r.by_cat.push((c.title.clone(), c.browser, r.freed - freed, r.removed - removed));
    }
    say(p, "");
    r
}

pub fn clean_duplicates(groups: &[DupGroup], to_trash: bool, p: &Progress) -> CleanReport {
    let mut r = CleanReport::default();
    for f in groups.iter().flat_map(|g| g.files.iter()).filter(|f| f.selected) {
        say(p, format!("Removing {}", f.path.display()));
        remove_path(&f.path, to_trash, &mut r);
    }
    say(p, "");
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_dir_cleans_contents_but_keeps_folder_and_excludes() {
        let root = std::env::temp_dir().join(format!("claybroom-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("keep/inner")).unwrap();
        fs::write(root.join("a.bin"), [0u8; 10]).unwrap();
        fs::write(root.join("keep/inner/x"), [0u8; 5]).unwrap();
        fs::write(root.join("keep/y"), [0u8; 7]).unwrap();
        let outside = std::env::temp_dir().join(format!("claybroom-outside-{}", std::process::id()));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("precious"), [1u8; 3]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let item = Item {
            path: root.clone(),
            label: String::new(),
            size: 22,
            selected: true,
            keep_dir: true,
            excludes: vec![root.join("keep/inner")],
            ..Default::default()
        };
        let mut r = CleanReport::default();
        clean_item(&item, false, &mut r);
        assert!(root.is_dir() && root.join("keep/inner/x").exists());
        assert!(!root.join("a.bin").exists() && !root.join("keep/y").exists());
        assert!(outside.join("precious").exists(), "cleaning must not follow links");
        assert_eq!(r.freed, 17);
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn duplicates_need_identical_content() {
        let root = std::env::temp_dir().join(format!("claybroom-dup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let big = vec![7u8; 20_000];
        let mut other = big.clone();
        other[0] = 1;
        fs::write(root.join("one.jpg"), &big).unwrap();
        fs::write(root.join("copy of one.jpg"), &big).unwrap();
        fs::write(root.join("same size.jpg"), &other).unwrap();
        let s = Settings { dup_min_idx: 0, ..Settings::default() };
        let g = scan_duplicates(&[root.clone()], &s, &Progress::default());
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].files.len(), 2);
        assert_eq!(g[0].files.iter().filter(|f| f.selected).count(), 1);
        assert_eq!(g[0].kind, Kind::Pictures);
        fs::remove_dir_all(&root).unwrap();
    }

    /// Read-only scan of this computer: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn scan_this_computer() {
        let p = Progress::default();
        for c in scan_quick(&Settings::default(), &p) {
            println!("{:<28} {:>10} {:>5} items, {} selected", c.title, fmt_size(c.size()), c.items.len(), c.selected_count());
        }
    }

    /// Read-only clutter scan with progress every 10 s: `cargo test -- --ignored scan_clutter_here --nocapture`.
    #[test]
    #[ignore]
    fn scan_clutter_here() {
        let p = Progress::default();
        let w = p.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
            let st = w.lock().unwrap().clone();
            println!("{:>3.0}% {}", st.frac * 100.0, st.text);
        });
        let t = std::time::Instant::now();
        for c in scan_clutter(&Settings::default(), &p) {
            println!("{:<28} {:>10} {:>5} items", c.title, fmt_size(c.size()), c.items.len());
        }
        println!("took {:?}", t.elapsed());
    }

    #[test]
    fn system_folder_is_protected() {
        #[cfg(target_os = "macos")]
        assert!(protected(Path::new("/System/Library")));
        assert!(protected(Path::new("/")));
        assert!(!protected(&home().join("Library/Caches/x")));
    }
}

#[cfg(test)]
mod whole_disk {
    /// Read-only: the whole-disk large-file search lists each file once (macOS reaches the data
    /// volume through both /Users and /System, which is skipped). `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn whole_disk_large_files_once_each() {
        let h = super::home();
        let found = super::scan_large_in(&super::computer_root(), &h, 1 << 30, &super::Progress::default());
        let mut paths: Vec<_> = found.iter().map(|i| i.path.clone()).collect();
        let n = paths.len();
        paths.sort();
        paths.dedup();
        println!("{n} large files over 1 GB on the whole disk");
        assert_eq!(n, paths.len());
        assert!(found.iter().all(|i| !i.path.starts_with("/System") && !i.path.starts_with(h.join("Library"))));
    }
}

#[cfg(test)]
mod dup_folders {
    /// Two identical folders show once as a folder pair; a folder missing a file doesn't match,
    /// and files already covered by the folder pair aren't listed again.
    #[test]
    fn identical_folders_are_found_once() {
        let root = std::env::temp_dir().join(format!("claybroom-dupdirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let big: Vec<u8> = (0..700_000u32).map(|i| (i.wrapping_mul(2654435761u32) >> 13) as u8).collect();
        let big2: Vec<u8> = big.iter().map(|b| b.wrapping_add(7)).collect();
        for d in ["a", "b"] {
            std::fs::create_dir_all(root.join(d).join("sub")).unwrap();
            std::fs::write(root.join(d).join("x.bin"), &big).unwrap();
            std::fs::write(root.join(d).join("sub/y.bin"), &big2).unwrap();
            std::fs::write(root.join(d).join("note.txt"), b"same small file").unwrap();
        }
        std::fs::write(root.join("a/.DS_Store"), b"differs").unwrap();
        std::fs::create_dir_all(root.join("c")).unwrap();
        std::fs::write(root.join("c/x.bin"), &big).unwrap();
        let s = super::Settings::default();
        let groups = super::scan_duplicates(&[root.clone()], &s, &super::Progress::default());
        let folders: Vec<_> = groups.iter().filter(|g| g.kind == super::Kind::Folders).collect();
        assert_eq!(folders.len(), 1, "one folder pair");
        let mut names: Vec<_> = folders[0].files.iter().map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["a", "b"]);
        // x.bin has a third copy outside the pair, so it stays; y.bin is covered by the pair.
        assert!(groups.iter().any(|g| g.kind != super::Kind::Folders && g.files.len() == 3));
        assert!(!groups.iter().any(|g| g.files.iter().any(|f| f.path.ends_with("sub/y.bin"))));
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(all(test, unix))]
mod needs_admin {
    /// A file the system won't let Claybroom remove (its folder is read-only) is offered to an
    /// administrator instead of only being counted as failed.
    #[test]
    fn refused_items_are_offered_to_an_administrator() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("claybroom-admin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("locked.log");
        std::fs::write(&f, [1u8; 4096]).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let mut r = super::CleanReport::default();
        super::remove_path(&f, false, &mut r);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(r.failed, 1);
        assert_eq!(r.needs_admin.len(), 1);
        assert_eq!(r.needs_admin[0].0, f);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
