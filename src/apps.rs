//! Installed apps (uninstaller) and startup items, per operating system.

use crate::clean::{home, Progress};
#[cfg(target_os = "macos")]
use crate::clean::size_of;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ---------- uninstaller ----------

#[derive(Clone)]
pub struct App {
    pub name: String,
    pub path: Option<PathBuf>,
    pub size: Option<u64>,
    pub last_used: Option<SystemTime>,
    /// macOS: leftover files (category, path, size).
    pub leftovers: Vec<(String, PathBuf, u64)>,
    /// Windows: the app's own uninstaller command. Linux: a flatpak id.
    pub uninstall_cmd: Option<String>,
    pub selected: bool,
    /// macOS bundle id, for finding leftovers.
    pub id: String,
    /// Size, last use and leftovers are known. On macOS they're measured after the list shows.
    pub measured: bool,
}

/// What measuring one app finds (the slow part: walking the bundle and ~/Library).
pub struct Measured {
    pub size: Option<u64>,
    pub last_used: Option<SystemTime>,
    pub leftovers: Vec<(String, PathBuf, u64)>,
}

impl App {
    pub fn total(&self) -> u64 {
        self.size.unwrap_or(0) + self.leftovers.iter().map(|l| l.2).sum::<u64>()
    }
    pub fn can_uninstall(&self) -> bool {
        cfg!(target_os = "macos") && self.path.is_some() || self.uninstall_cmd.is_some()
    }
}

/// List the installed apps fast: sizes come after, from
/// `measure_in_background`.
pub fn scan_apps(p: &Progress) -> Vec<App> {
    let mut apps = platform_apps(p);
    apps.sort_by_key(|a| a.name.to_lowercase());
    crate::clean::say(p, "");
    apps
}

/// Measure the apps the list doesn't know yet on a few threads, in list order, sending each
/// result as soon as it's ready. Stops when the receiver is dropped (the list was closed).
pub fn measure_in_background(list: &[App], tx: std::sync::mpsc::Sender<(PathBuf, Measured)>, ctx: eframe::egui::Context) {
    let mut todo: Vec<(PathBuf, String, String)> =
        list.iter().filter(|a| !a.measured).filter_map(|a| Some((a.path.clone()?, a.id.clone(), a.name.clone()))).collect();
    todo.reverse();
    let todo = std::sync::Arc::new(std::sync::Mutex::new(todo));
    for _ in 0..4 {
        let (todo, tx, ctx) = (todo.clone(), tx.clone(), ctx.clone());
        std::thread::spawn(move || loop {
            let Some((path, id, name)) = todo.lock().unwrap().pop() else { break };
            let m = measure(&path, &id, &name);
            if tx.send((path, m)).is_err() {
                break;
            }
            ctx.request_repaint();
        });
    }
}

/// Measure every app now (Super Clean needs last-used dates before it can suggest anything).
pub fn measure_now(list: &mut [App], ctx: eframe::egui::Context) {
    let (tx, rx) = std::sync::mpsc::channel();
    measure_in_background(list, tx, ctx);
    for (path, m) in rx {
        if let Some(a) = list.iter_mut().find(|a| a.path.as_ref() == Some(&path)) {
            (a.size, a.last_used, a.leftovers, a.measured) = (m.size, m.last_used, m.leftovers, true);
        }
    }
}

#[cfg(target_os = "macos")]
fn measure(path: &Path, id: &str, name: &str) -> Measured {
    Measured { size: Some(size_of(path, &[])), last_used: mac_last_used(path), leftovers: mac_leftovers(&home(), id, name) }
}

#[cfg(not(target_os = "macos"))]
fn measure(_path: &Path, _id: &str, _name: &str) -> Measured {
    Measured { size: None, last_used: None, leftovers: vec![] }
}

#[cfg(target_os = "macos")]
fn platform_apps(_p: &Progress) -> Vec<App> {
    let h = home();
    let mut out = Vec::new();
    for dir in [PathBuf::from("/Applications"), h.join("Applications")] {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.filter_map(Result::ok) {
            let path = e.path();
            if path.extension().map_or(true, |x| x != "app") {
                continue;
            }
            let info = plist::Value::from_file(path.join("Contents/Info.plist")).ok();
            let id = info
                .as_ref()
                .and_then(|v| v.as_dictionary())
                .and_then(|d| d.get("CFBundleIdentifier"))
                .and_then(|v| v.as_string())
                .unwrap_or_default()
                .to_string();
            if id.starts_with("com.apple.") {
                continue; // part of macOS
            }
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            out.push(App { size: None, last_used: None, name, path: Some(path), leftovers: vec![], uninstall_cmd: None, selected: false, id, measured: false });
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn mac_last_used(app: &Path) -> Option<SystemTime> {
    let o = std::process::Command::new("mdls").args(["-raw", "-name", "kMDItemLastUsedDate"]).arg(app).output().ok()?;
    // Format: "2026-11-20 09:15:02 +0000"
    let s = String::from_utf8_lossy(&o.stdout);
    let date = s.split_whitespace().next()?;
    let mut it = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    // days since 1970-01-01 (civil-from-days inverse, Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs((days.max(0) * 86400) as u64))
}

#[cfg(target_os = "macos")]
fn mac_leftovers(h: &Path, id: &str, name: &str) -> Vec<(String, PathBuf, u64)> {
    let lib = h.join("Library");
    let places = [
        ("Caches", "Caches"),
        ("Preferences", "Preferences"),
        ("Supporting files", "Application Support"),
        ("Logs", "Logs"),
        ("Containers", "Containers"),
        ("Saved state", "Saved Application State"),
        ("Caches", "HTTPStorages"),
        ("Caches", "WebKit"),
        ("Cookies", "Cookies"),
        ("Launchd Services", "LaunchAgents"),
        ("Extensions", "Application Scripts"),
        ("Shared files", "Group Containers"),
    ];
    let mut out = Vec::new();
    for (cat, sub) in places {
        let Ok(rd) = fs::read_dir(lib.join(sub)) else { continue };
        for e in rd.filter_map(Result::ok) {
            let f = e.file_name().to_string_lossy().into_owned();
            // Group containers are "<team id>.<bundle id>" or "group.<bundle id>".
            let group = sub == "Group Containers" && !id.is_empty() && (f.ends_with(&format!(".{id}")) || f == format!("group.{id}"));
            let hit = (!id.is_empty() && (f == id || f.starts_with(&format!("{id}.")))) || f == name || group;
            if hit {
                out.push((cat.to_string(), e.path(), size_of(&e.path(), &[])));
            }
        }
    }
    // Temporary files the app keeps in its own folder under the system temp folder.
    if !id.is_empty() {
        let tmp = std::env::temp_dir().join(id);
        if tmp.is_dir() {
            out.push(("Temporary".to_string(), tmp.clone(), size_of(&tmp, &[])));
        }
    }
    out
}

#[cfg(windows)]
fn platform_apps(_p: &Progress) -> Vec<App> {
    use winreg::enums::*;
    use winreg::RegKey;
    let mut out = Vec::new();
    let roots = [
        (HKEY_LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKEY_CURRENT_USER, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
    ];
    let mut seen = std::collections::HashSet::new();
    for (hive, path) in roots {
        let Ok(key) = RegKey::predef(hive).open_subkey(path) else { continue };
        for sub in key.enum_keys().filter_map(Result::ok) {
            let Ok(k) = key.open_subkey(&sub) else { continue };
            let Ok(name) = k.get_value::<String, _>("DisplayName") else { continue };
            if k.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1 || !seen.insert(name.clone()) {
                continue;
            }
            let size = k.get_value::<u32, _>("EstimatedSize").ok().map(|kb| kb as u64 * 1024);
            let cmd = k
                .get_value::<String, _>("QuietUninstallString")
                .or_else(|_| k.get_value::<String, _>("UninstallString"))
                .ok();
            let path = k.get_value::<String, _>("InstallLocation").ok().filter(|s| !s.is_empty()).map(PathBuf::from);
            out.push(App { name, path, size, last_used: None, leftovers: vec![], uninstall_cmd: cmd, selected: false, id: String::new(), measured: true });
        }
    }
    out
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_apps(_p: &Progress) -> Vec<App> {
    let h = home();
    let dirs = [
        PathBuf::from("/usr/share/applications"),
        h.join(".local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        h.join(".local/share/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
    ];
    let mut out = Vec::new();
    for d in dirs {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.filter_map(Result::ok) {
            let path = e.path();
            let Ok(text) = fs::read_to_string(&path) else { continue };
            let field = |k: &str| text.lines().find_map(|l| l.strip_prefix(k)).map(str::to_string);
            if field("NoDisplay=").as_deref() == Some("true") || field("Type=").as_deref() != Some("Application") {
                continue;
            }
            let Some(name) = field("Name=") else { continue };
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            let shown = path.to_string_lossy();
            // How it was installed: Flatpak, Snap, or the system's package manager.
            let how = if shown.contains("flatpak") {
                Some(format!("flatpak:{stem}"))
            } else if shown.contains("snapd") {
                Some(format!("snap:{}", stem.split('_').next().unwrap_or(&stem)))
            } else if shown.starts_with("/usr/") {
                Some("package".to_string())
            } else {
                None
            };
            out.push(App { name, path: Some(path), size: None, last_used: None, leftovers: vec![], uninstall_cmd: how, selected: false, id: String::new(), measured: true });
        }
    }
    out
}

/// The package that owns a .desktop file and the command that removes it: dpkg/apt (Debian,
/// Ubuntu), rpm/dnf (Fedora), or pacman (Arch), whichever answers.
#[cfg(all(unix, not(target_os = "macos")))]
fn linux_package(desktop: &Path) -> Option<(String, &'static [&'static str])> {
    use std::process::Command;
    let ask = |prog: &str, args: &[&str]| -> Option<String> {
        let o = Command::new(prog).args(args).arg(desktop).output().ok()?;
        o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty())
    };
    if let Some(line) = ask("dpkg-query", &["-S"]) {
        // "package[:arch]: /usr/share/applications/x.desktop"
        let pkg = line.lines().next()?.split(':').next()?.trim().to_string();
        return Some((pkg, &["apt-get", "remove", "-y"]));
    }
    if let Some(pkg) = ask("rpm", &["-qf", "--queryformat", "%{NAME}"]) {
        return Some((pkg, &["dnf", "remove", "-y"]));
    }
    if let Some(pkg) = ask("pacman", &["-Qqo"]) {
        return Some((pkg, &["pacman", "-R", "--noconfirm"]));
    }
    None
}

/// Uninstall one app. Returns a message on failure.
pub fn uninstall(app: &App) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let path = app.path.as_ref().ok_or("no app path")?;
        // Not measured yet: find its leftovers now, so none stay behind.
        let found;
        let leftovers = if app.measured {
            &app.leftovers
        } else {
            found = mac_leftovers(&home(), &app.id, &app.name);
            &found
        };
        trash::delete(path).map_err(|e| e.to_string())?;
        for (_, p, _) in leftovers {
            let _ = trash::delete(p);
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        let cmd = app.uninstall_cmd.as_ref().ok_or("no uninstaller")?;
        std::process::Command::new("cmd").args(["/C", cmd]).status().map_err(|e| e.to_string()).map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::process::Command;
        let how = app.uninstall_cmd.as_deref().ok_or("this app wasn't installed by a package manager")?;
        // System packages and Snaps need an administrator: pkexec asks for the password.
        let mut cmd = if let Some(id) = how.strip_prefix("flatpak:") {
            let mut c = Command::new("flatpak");
            c.args(["uninstall", "-y", id]);
            c
        } else if let Some(name) = how.strip_prefix("snap:") {
            let mut c = Command::new("pkexec");
            c.args(["snap", "remove", name]);
            c
        } else {
            let desktop = app.path.as_ref().ok_or("no app path")?;
            let (pkg, remove) = linux_package(desktop).ok_or("couldn't tell which package installed this app")?;
            let mut c = Command::new("pkexec");
            c.args(remove).arg(pkg);
            c
        };
        let ok = cmd.status().map_err(|e| e.to_string())?;
        ok.success().then_some(()).ok_or_else(|| format!("{} was not uninstalled", app.name))
    }
}

// ---------- startup items ----------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum StartKind {
    Application,
    LaunchAgent,
    LaunchDaemon,
}

impl StartKind {
    pub fn label(self) -> &'static str {
        match self {
            StartKind::Application => "Application",
            StartKind::LaunchAgent => "Launch agent",
            StartKind::LaunchDaemon => "Launch daemon",
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StartItem {
    pub name: String,
    pub kind: StartKind,
    /// Where it lives when it's on (a file, or "HKCU\\...\\Run\\Name" on Windows).
    pub location: String,
    pub on: bool,
    /// Needs administrator rights to change.
    pub system: bool,
    /// Windows registry command, kept while the item is off.
    #[serde(default)]
    pub reg_value: Option<String>,
}

fn store_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(home).join("claybroom").join("disabled")
}

fn store_file() -> PathBuf {
    store_dir().join("disabled.json")
}

fn load_disabled() -> Vec<StartItem> {
    fs::read(store_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_disabled(v: &[StartItem]) -> Result<(), String> {
    fs::create_dir_all(store_dir()).map_err(|e| e.to_string())?;
    fs::write(store_file(), serde_json::to_vec_pretty(v).unwrap()).map_err(|e| e.to_string())
}

/// A system-wide launch agent or daemon (in /Library) turned off or on with an administrator's
/// approval: off stops it and keeps its file in Claybroom's store, on puts it back.
#[cfg(target_os = "macos")]
fn set_on_system(item: &mut StartItem, on: bool) -> Result<(), String> {
    use crate::shell::{admin_sh, sh_quote};
    let loc = sh_quote(&item.location);
    let copy = sh_quote(&stored_copy(&item.location).display().to_string());
    let daemon = item.location.starts_with("/Library/LaunchDaemons/");
    let script = if on {
        let start = if daemon { format!("; /bin/launchctl bootstrap system {loc} 2>/dev/null; true") } else { String::new() };
        format!("/bin/mv {copy} {loc} && /usr/sbin/chown root:wheel {loc} && /bin/chmod 644 {loc}{start}")
    } else {
        let stop = if daemon { format!("/bin/launchctl bootout system {loc} 2>/dev/null; ") } else { String::new() };
        format!("{stop}/bin/mkdir -p {} && /bin/mv {loc} {copy}", sh_quote(&store_dir().display().to_string()))
    };
    admin_sh(&script, &format!("Claybroom needs an administrator to turn {} {}.", item.name, if on { "on" } else { "off" }))?;
    let mut store = load_disabled();
    item.on = on;
    if on {
        store.retain(|s| s.location != item.location);
    } else {
        store.push(item.clone());
    }
    save_disabled(&store)
}

#[cfg(target_os = "macos")]
fn remove_system(item: &StartItem) -> Result<(), String> {
    use crate::shell::{admin_sh, sh_quote};
    let script = if item.on {
        let loc = sh_quote(&item.location);
        let stop = if item.location.starts_with("/Library/LaunchDaemons/") { format!("/bin/launchctl bootout system {loc} 2>/dev/null; ") } else { String::new() };
        format!("{stop}/bin/rm -f {loc}")
    } else {
        format!("/bin/rm -f {}", sh_quote(&stored_copy(&item.location).display().to_string()))
    };
    admin_sh(&script, &format!("Claybroom needs an administrator to remove {} from startup items.", item.name))?;
    let mut store = load_disabled();
    store.retain(|s| s.location != item.location);
    save_disabled(&store)
}

fn stored_copy(location: &str) -> PathBuf {
    let name = Path::new(location).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    store_dir().join(name)
}

pub fn scan_startup() -> Vec<StartItem> {
    let mut v = platform_startup();
    v.extend(load_disabled());
    v.sort_by_key(|i| i.name.to_lowercase());
    v
}

#[cfg(target_os = "macos")]
fn plist_label(path: &Path) -> Option<String> {
    let v = plist::Value::from_file(path).ok()?;
    let d = v.as_dictionary()?;
    let prog = d
        .get("Program")
        .and_then(|x| x.as_string())
        .or_else(|| d.get("ProgramArguments")?.as_array()?.first()?.as_string());
    // A friendly name: the program's file name, else the label.
    prog.and_then(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()))
        .or_else(|| d.get("Label")?.as_string().map(str::to_string))
}

#[cfg(target_os = "macos")]
fn platform_startup() -> Vec<StartItem> {
    let h = home();
    let mut out = Vec::new();
    for (dir, kind, system) in [
        (h.join("Library/LaunchAgents"), StartKind::LaunchAgent, false),
        (PathBuf::from("/Library/LaunchAgents"), StartKind::LaunchAgent, true),
        (PathBuf::from("/Library/LaunchDaemons"), StartKind::LaunchDaemon, true),
    ] {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.filter_map(Result::ok).filter(|e| e.path().extension().map_or(false, |x| x == "plist")) {
            let path = e.path();
            let name = plist_label(&path).unwrap_or_else(|| path.file_stem().unwrap().to_string_lossy().into_owned());
            out.push(StartItem { name, kind, location: path.display().to_string(), on: true, system, reg_value: None });
        }
    }
    // Login items, through System Events.
    if let Ok(o) = std::process::Command::new("osascript")
        .args(["-e", "tell application \"System Events\" to get the name of every login item"])
        .output()
    {
        for name in String::from_utf8_lossy(&o.stdout).trim().split(", ").filter(|s| !s.is_empty()) {
            out.push(StartItem {
                name: name.to_string(),
                kind: StartKind::Application,
                location: "Login items".into(),
                on: true,
                system: false,
                reg_value: None,
            });
        }
    }
    out
}

#[cfg(windows)]
fn platform_startup() -> Vec<StartItem> {
    use winreg::enums::*;
    use winreg::RegKey;
    let mut out = Vec::new();
    let run = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    for (hive, tag, system) in [(HKEY_CURRENT_USER, "HKCU", false), (HKEY_LOCAL_MACHINE, "HKLM", true)] {
        let Ok(k) = RegKey::predef(hive).open_subkey(run) else { continue };
        for (name, _) in k.enum_values().filter_map(Result::ok) {
            out.push(StartItem {
                location: format!("{tag}\\{run}\\{name}"),
                name,
                kind: StartKind::Application,
                on: true,
                system,
                reg_value: None,
            });
        }
    }
    if let Some(dir) = dirs::config_dir().map(|c| c.join("Microsoft\\Windows\\Start Menu\\Programs\\Startup")) {
        for e in fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok) {
            let path = e.path();
            if path.file_name().map_or(false, |n| n == "desktop.ini") {
                continue;
            }
            out.push(StartItem {
                name: path.file_stem().unwrap().to_string_lossy().into_owned(),
                kind: StartKind::Application,
                location: path.display().to_string(),
                on: true,
                system: false,
                reg_value: None,
            });
        }
    }
    out
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_startup() -> Vec<StartItem> {
    let mut out = Vec::new();
    for (dir, system) in [(home().join(".config/autostart"), false), (PathBuf::from("/etc/xdg/autostart"), true)] {
        for e in fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok) {
            let path = e.path();
            let Ok(text) = fs::read_to_string(&path) else { continue };
            let name = text.lines().find_map(|l| l.strip_prefix("Name=")).unwrap_or("?").to_string();
            let hidden = text.lines().any(|l| l == "Hidden=true");
            out.push(StartItem { name, kind: StartKind::Application, location: path.display().to_string(), on: !hidden, system, reg_value: None });
        }
    }
    out
}

/// Turn an item off (moved into Claybroom's own folder) or back on.
pub fn set_on(item: &mut StartItem, on: bool) -> Result<(), String> {
    if item.system {
        #[cfg(target_os = "macos")]
        return set_on_system(item, on);
        #[cfg(not(target_os = "macos"))]
        return Err("This is a system-wide item; changing it needs administrator rights.".into());
    }
    if item.location == "Login items" {
        return Err("Login items can only be removed, not paused.".into());
    }
    let mut store = load_disabled();
    if !on {
        #[cfg(windows)]
        if item.location.starts_with("HKCU\\") {
            use winreg::{enums::HKEY_CURRENT_USER, RegKey};
            let (key, name) = item.location[5..].rsplit_once('\\').unwrap();
            let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(key, winreg::enums::KEY_ALL_ACCESS).map_err(|e| e.to_string())?;
            item.reg_value = Some(k.get_value::<String, _>(name).map_err(|e| e.to_string())?);
            k.delete_value(name).map_err(|e| e.to_string())?;
            item.on = false;
            store.push(item.clone());
            return save_disabled(&store);
        }
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("launchctl").args(["unload", &item.location]).status();
        fs::create_dir_all(store_dir()).map_err(|e| e.to_string())?;
        fs::rename(&item.location, stored_copy(&item.location)).map_err(|e| e.to_string())?;
        item.on = false;
        store.push(item.clone());
    } else {
        #[cfg(windows)]
        if let Some(val) = &item.reg_value {
            use winreg::{enums::HKEY_CURRENT_USER, RegKey};
            let (key, name) = item.location[5..].rsplit_once('\\').unwrap();
            let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(key).map_err(|e| e.to_string())?;
            k.set_value(name, val).map_err(|e| e.to_string())?;
        }
        if item.reg_value.is_none() {
            fs::rename(stored_copy(&item.location), &item.location).map_err(|e| e.to_string())?;
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("launchctl").args(["load", &item.location]).status();
        }
        item.on = true;
        item.reg_value = None;
        store.retain(|s| s.location != item.location);
    }
    save_disabled(&store)
}

/// Remove an item for good (files go to the Trash).
pub fn remove(item: &StartItem) -> Result<(), String> {
    if item.system {
        #[cfg(target_os = "macos")]
        return remove_system(item);
        #[cfg(not(target_os = "macos"))]
        return Err("This is a system-wide item; removing it needs administrator rights.".into());
    }
    let mut store = load_disabled();
    if item.location == "Login items" {
        let script = format!("tell application \"System Events\" to delete login item \"{}\"", item.name.replace('"', ""));
        let ok = std::process::Command::new("osascript").args(["-e", &script]).status().map_err(|e| e.to_string())?;
        return ok.success().then_some(()).ok_or_else(|| "System Events refused".into());
    }
    if !item.on {
        if item.reg_value.is_none() {
            trash::delete(stored_copy(&item.location)).map_err(|e| e.to_string())?;
        }
        store.retain(|s| s.location != item.location);
        return save_disabled(&store);
    }
    #[cfg(windows)]
    if item.location.starts_with("HKCU\\") {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let (key, name) = item.location[5..].rsplit_once('\\').unwrap();
        let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(key, winreg::enums::KEY_ALL_ACCESS).map_err(|e| e.to_string())?;
        return k.delete_value(name).map_err(|e| e.to_string());
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("launchctl").args(["unload", &item.location]).status();
    trash::delete(&item.location).map_err(|e| e.to_string())
}
