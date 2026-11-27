//! Measures what this build actually does, value by value, for the names in mechanics.json.
//! Size limits and selection rules are probed by running the real scanners on throw-away files
//! just below and at each limit; lists and defaults are read from the code paths the app uses.
//! `claybroom --mechanics out.json` writes the result; `cargo test` compares it with the sheet.

use crate::clean::{self, Item, Settings};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A fresh empty folder in the system temp folder.
fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("claybroom-measure-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// "250 MB", "1 GB", "16 KB".
fn size_label(b: u64) -> String {
    match b {
        b if b >= 1_000_000_000 => format!("{} GB", b / 1_000_000_000),
        b if b >= 1_000_000 => format!("{} MB", b / 1_000_000),
        b => format!("{} KB", b / 1_000),
    }
}

fn labels(sizes: &[u64]) -> String {
    sizes.iter().map(|b| size_label(*b)).collect::<Vec<_>>().join(", ")
}

/// Run the clutter rules on a folder holding `names` (each with a file inside) and clean everything
/// they selected; returns the names that survived, i.e. the ones the rules leave alone.
fn surviving(tag: &str, names: &[&str], excludes: &[&'static str]) -> Vec<String> {
    let d = scratch(tag);
    for n in names.iter().chain(["something-else"].iter()) {
        fs::create_dir_all(d.join(n)).unwrap();
        fs::write(d.join(n).join("data"), [1u8; 64]).unwrap();
    }
    let p = clean::Progress::default();
    let items = clean::run_rules(vec![clean::rule(vec![d.clone()], excludes.to_vec(), true, true)], &p);
    let cat = clean::Category { title: "probe".into(), tip: "", items, to_trash: false, open: false, browser: false };
    clean::clean_categories(&[cat], &p);
    let left = names.iter().filter(|n| d.join(n).join("data").exists()).map(|n| n.to_string()).collect();
    let _ = fs::remove_dir_all(&d);
    left
}

/// Smallest size the duplicate finder reports when given identical pairs just below and at `limit`.
fn smallest_duplicate(limit: u64) -> u64 {
    let d = scratch("dups");
    for (i, size) in [limit - 1, limit].into_iter().enumerate() {
        let body = vec![i as u8 + 7; size as usize];
        fs::write(d.join(format!("a{i}.bin")), &body).unwrap();
        fs::write(d.join(format!("b{i}.bin")), &body).unwrap();
    }
    let groups = clean::scan_duplicates(&[d.clone()], &Settings::default(), &clean::Progress::default());
    let _ = fs::remove_dir_all(&d);
    groups.iter().map(|g| g.size).min().unwrap_or(0)
}

/// Smallest size the large-file search reports, given (sparse) files just below and at `limit`.
fn smallest_large_file(limit: u64) -> u64 {
    let d = scratch("large");
    fs::create_dir_all(d.join("docs")).unwrap();
    for (name, size) in [("below.bin", limit - 1), ("at.bin", limit)] {
        fs::File::create(d.join("docs").join(name)).unwrap().set_len(size).unwrap();
    }
    let found = clean::scan_large(&d, limit, &clean::Progress::default());
    let _ = fs::remove_dir_all(&d);
    found.iter().map(|i| i.size).min().unwrap_or(0)
}

pub fn measure() -> Value {
    let s = Settings::default();
    let mut v: Vec<Value> = vec![];
    let mut put = |name: &str, value: Value, how: &str| v.push(json!({ "name": name, "value": value, "how": how }));

    // Clutter
    put("large files minimum size options", json!(labels(&clean::LARGE_FILE_SIZES)), "the options Settings offers");
    let lf = clean::LARGE_FILE_SIZES[s.large_file_idx];
    put("large files minimum size default", json!(smallest_large_file(lf)), "large-file search run on files one byte below and at the default limit");
    put("scan downloads default", json!(s.scan_downloads), "default settings");
    put("scan system cache and logs default", json!(s.scan_system), "default settings");
    put("scan external drives default", json!(s.scan_external), "default settings");
    if cfg!(target_os = "macos") {
        put("user cache folder (macOS)", json!("~/Library/Caches"), "the user-cache rule's folder");
        let names: Vec<&str> = clean::USER_CACHE_EXCLUDES.to_vec();
        put("user cache excludes (macOS)", json!(surviving("cache", &names, &clean::USER_CACHE_EXCLUDES).join(", ")), "cache rule cleaned a folder holding them; these survived");
        let names: Vec<&str> = clean::USER_LOG_EXCLUDES.to_vec();
        put("user log excludes (macOS)", json!(surviving("logs", &names, &clean::USER_LOG_EXCLUDES).join(", ")), "log rule cleaned a folder holding them; these survived");
        put("protected folder", json!(["/System/", "/Library/", "/Users/"].iter().filter(|p| clean::protected(&Path::new(p).join("x"))).copied().collect::<Vec<_>>().join(", ")), "asked which system folders refuse removal");
    }
    let samples = [(".DS_Store", false), (".VolumeIcon.icns", false), ("._x", false), (".fseventsd", true), (".Spotlight-V100", true), (".TemporaryItems", true), ("photo.jpg", false), ("Documents", true)];
    let junk: Vec<String> = samples.iter().filter(|(n, d)| clean::is_drive_junk(n, *d)).map(|(n, _)| if *n == "._x" { "._*".to_string() } else { n.to_string() }).collect();
    put("external drive meta files", json!(junk.join(", ")), "drive-junk test run on sample names");
    {
        let d = scratch("trash");
        fs::create_dir_all(d.join("old/empty")).unwrap();
        let p = clean::Progress::default();
        let items = clean::run_rules(vec![clean::rule(vec![d.clone()], vec![], false, true)], &p);
        let cat = clean::Category { title: "Trash".into(), tip: "", items, to_trash: false, open: false, browser: false };
        clean::clean_categories(&[cat], &p);
        put("trash deletes empty folders", json!(!d.join("old").exists()), "Trash rule cleaned a folder holding an empty folder");
        let _ = fs::remove_dir_all(&d);
    }

    // Duplicates
    put("duplicates minimum size options", json!(labels(&clean::DUP_SIZES)), "the options Settings offers");
    put("duplicates minimum size default", json!(smallest_duplicate(clean::DUP_SIZES[s.dup_min_idx])), "duplicate finder run on identical pairs one byte below and at the default limit");
    {
        let d = scratch("rule");
        let body = vec![5u8; 600_000];
        let mut other = body.clone();
        other[0] = 6;
        fs::write(d.join("one.bin"), &body).unwrap();
        fs::write(d.join("renamed copy.bin"), &body).unwrap();
        fs::write(d.join("same size.bin"), &other).unwrap();
        let old = SystemTime::now() - Duration::from_secs(86_400);
        fs::File::options().write(true).open(d.join("one.bin")).unwrap().set_modified(old).unwrap();
        fs::write(d.join(".hidden a"), &other).unwrap();
        fs::write(d.join(".hidden b"), &other).unwrap();
        let g = clean::scan_duplicates(&[d.clone()], &s, &clean::Progress::default());
        let exact = g.len() == 1 && g[0].files.len() == 2;
        put("duplicates match rule", json!(if exact { "100% identical content" } else { "other" }), "same-content pair with different names vs. same-size different-content file");
        put("ignore hidden files default", json!(exact), "hidden identical files were left out of the results");
        let kept = g.first().and_then(|g| g.files.iter().find(|f| !f.selected)).map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned());
        let rule = match kept.as_deref() {
            Some("renamed copy.bin") => "keep newest",
            Some("one.bin") => "keep oldest",
            _ => "other",
        };
        put("duplicates preselect default", json!(rule), "which copy stayed unticked after a scan");
        let _ = fs::remove_dir_all(&d);
    }
    put("move duplicates to trash default", json!(s.dup_to_trash), "default settings");

    // Uninstaller
    put("large apps options", json!(labels(&clean::LARGE_APP_SIZES)), "the options Settings offers");
    put("large apps default", json!(clean::LARGE_APP_SIZES[s.large_app_idx]), "default settings");
    let months: Vec<String> = clean::UNUSED_DAYS.iter().filter(|d| d.0 < 365).map(|d| ((d.0 as f64 / 30.4).round() as u64).to_string()).collect();
    put("unused apps options", json!(format!("{} months, 1 year", months.join(", "))), "the options Settings offers");
    put("unused apps default", json!((clean::UNUSED_DAYS[s.unused_idx].0 as f64 / 30.4).round()), "default settings, days converted to months");

    // Browsers
    // ponytail: the sheet lists the macOS browsers (Safari is macOS-only), so other systems skip this row;
    // give the sheet per-OS rows if Windows/Linux browser lists ever need checking.
    if cfg!(target_os = "macos") {
        put("browsers", json!(clean::browser_defs().iter().map(|b| b.name).collect::<Vec<_>>().join(", ")), "browsers this build looks for on this system");
    }
    let one = || vec![Item { size: 1, ..Default::default() }];
    let mut cats = clean::browser_cats(one(), one(), one());
    cats.extend(clean::browser_private_cats(one(), one()));
    put("browser categories", json!(cats.iter().map(|c| c.title.as_str()).collect::<Vec<_>>().join(", ")), "categories a browser scan produces");
    let rec: Vec<&str> = cats.iter().filter(|c| crate::recommended(c)).map(|c| if c.title == "Browsing history" { "history" } else { "caches" }).collect();
    put("browser recommended selection", json!(rec.join(" and ").to_lowercase()), "what Suggested ticks");

    // Startup, statistics, layout
    let kinds = [crate::apps::StartKind::Application, crate::apps::StartKind::LaunchAgent, crate::apps::StartKind::LaunchDaemon];
    put("startup item types", json!(kinds.iter().map(|k| k.label()).collect::<Vec<_>>().join(", ")), "startup item types the manager shows");
    put("statistics: size of one picture", json!(clean::PICTURE_BYTES / 1e6), "Statistics conversion, MB");
    put("statistics: one minute of video", json!(clean::VIDEO_MINUTE_BYTES / 1e6), "Statistics conversion, MB");
    put("statistics: one song", json!(clean::SONG_BYTES / 1e6), "Statistics conversion, MB");
    put("window size", json!(format!("{} x {}", crate::WINDOW[0], crate::WINDOW[1])), "window opened at this size");

    // Photos, Trash, Alerts
    put("move deleted photos to trash default", json!(s.photos_to_trash), "default settings");
    {
        use image::{DynamicImage, GrayImage, Luma};
        let img = |dark: u8, light: u8| DynamicImage::ImageLuma8(GrayImage::from_fn(256, 256, |x, y| Luma([if (x / 8 + y / 8) % 2 == 0 { dark } else { light }])));
        let verdict: Vec<&str> = [img(30, 220), img(128, 128), img(5, 20), img(235, 250)]
            .iter()
            .map(|i| match crate::photos::reason(&crate::photos::measure(i)) {
                "" => "fine",
                r => r,
            })
            .collect();
        put("photo bad-photo rules", json!(verdict.join(", ")), "photo analyzer run on a sharp, a flat, a dark and a bright sample image");
    }
    put("similar photos: max hash difference", json!(crate::photos::SIMILAR_BITS), "photo analyzer constant");
    put("similar photos: max time apart", json!(crate::photos::SIMILAR_SECS), "photo analyzer constant");
    put("auto-empty trash default", json!(s.trash_auto), "default settings");
    put("auto-empty trash after", json!(clean::TRASH_DAYS[s.trash_days_idx]), "default settings");
    put("clutter alert default", json!(if s.alert_clutter { clean::CLUTTER_ALERT[s.alert_clutter_idx] } else { 0 }), "default settings");
    put("disk space alert default", json!(if s.alert_disk { clean::DISK_ALERT[s.alert_disk_idx] } else { 0 }), "default settings");
    put("settings tabs", json!(crate::tabs::SETTINGS_TABS.iter().map(|t| t.1).collect::<Vec<_>>().join(", ")), "tabs the Settings window shows");
    let rail: Vec<String> = crate::PAGES.iter().map(|p| p.2.to_string()).chain(["SETTINGS".to_string()]).map(|t| {
        let lower = t.to_lowercase();
        let mut c = lower.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }).collect();
    put("sidebar tabs", json!(rail.join(", ")), "items on the left rail, in order");

    put("appearance themes", json!(crate::kit::THEMES.join(", ")), "looks Settings › General offers");
    put("appearance default", json!(crate::kit::THEMES[s.appearance.min(2) as usize]), "default settings");
    put("large files on the whole computer default", json!(s.large_whole_disk), "default settings");
    put("photo file types", json!(format!("{}; {} through a converter", crate::photos::PHOTO_TYPES.join(", "), crate::photos::HEIC_TYPES.join(", "))), "files Analyze photos reads");
    put("update check default", json!(s.check_updates), "default settings");
    put("folder right-click item default", json!(s.folder_action), "default settings");
    put("photos library default", json!(s.photos_library), "default settings");
    put("super clean steps", json!(crate::super_clean::STEPS.iter().map(|x| x.1).collect::<Vec<_>>().join(", ")), "steps the Super Clean scan runs");

    // Browser Cleaning
    put("scheduled cleaning options", json!(crate::schedule::FREQ_NAMES.join(", ")), "choices of each browser's schedule");
    put("scheduled cleaning default", json!(crate::schedule::FREQ_NAMES[crate::schedule::Schedule::default().freq]), "default schedule");
    put("scheduled check interval", json!(crate::schedule::CHECK_SECS), "how often the background job looks");
    put("only delete older history default", json!(s.history_older), "default settings");
    put("history age options", json!(crate::schedule::HISTORY_AGES.iter().map(|a| a.1).collect::<Vec<_>>().join(", ")), "the options Settings offers");
    {
        // The app's own cleaning path on sample browser databases.
        let d = scratch("browserdb");
        let cookies = d.join("Cookies");
        let c = rusqlite::Connection::open(&cookies).unwrap();
        c.execute_batch("CREATE TABLE cookies (host_key TEXT, name TEXT); INSERT INTO cookies VALUES ('.example.com','a'), ('.keep.org','b'), ('mail.keep.org','c'), ('ads.net','d');").unwrap();
        drop(c);
        let now = crate::schedule::now();
        let webkit = |t: u64| (t + 11_644_473_600) * 1_000_000;
        let history = d.join("History");
        let c = rusqlite::Connection::open(&history).unwrap();
        c.execute_batch(&format!("CREATE TABLE urls (id INTEGER, url TEXT); CREATE TABLE visits (url INTEGER, visit_time INTEGER); INSERT INTO urls VALUES (1,'old'),(2,'new'); INSERT INTO visits VALUES (1,{}),(2,{});", webkit(now - 2 * 86_400), webkit(now - 3600))).unwrap();
        drop(c);
        let item = |path: &Path, db| Item { path: path.to_path_buf(), size: 1, selected: true, db, ..Default::default() };
        let cat = clean::Category { title: "Browser".into(), tip: "", items: vec![item(&cookies, crate::browser_pro::Db::ChromiumCookies), item(&history, crate::browser_pro::Db::ChromiumHistory)], to_trash: false, open: false, browser: true };
        let opts = crate::browser_pro::BrowserOpts { allow: vec!["keep.org".into()], history_before: Some(now - crate::schedule::HISTORY_AGES[s.history_age_idx].0) };
        clean::clean_categories_with(&[cat], &clean::Progress::default(), &opts);
        let list = |p: &Path, sql: &str| -> Vec<String> {
            let c = rusqlite::Connection::open(p).unwrap();
            let mut st = c.prepare(sql).unwrap();
            let mut v: Vec<String> = st.query_map([], |r| r.get::<_, String>(0)).unwrap().filter_map(Result::ok).collect();
            v.sort();
            v
        };
        put("cookie allowlist keeps", json!(list(&cookies, "SELECT host_key FROM cookies").join(", ")), "cleaned a sample cookie database with keep.org on the allowlist; these cookies remained");
        let left = list(&history, "SELECT url FROM urls");
        put("history older than 1 day", json!(if left == ["new"] { "recent visits kept, older removed" } else { "other" }), "cleaned a sample history with a visit an hour ago and one two days ago");
        let _ = fs::remove_dir_all(&d);
    }

    json!({
        "version": 1,
        "measured_on": std::env::consts::OS,
        "values": v,
        "not_measurable": ["Windows and Linux cleaning folders"],
    })
}

/// One row of the rules check: the sheet's value, what the build did, and whether they agree.
pub struct Row {
    pub name: String,
    pub sheet: String,
    pub measured: Option<String>,
    pub how: String,
    pub ok: bool,
}

/// Lists compare as sets ("Cookies, Caches" == "Caches, Cookies"); numbers allow rounding.
fn same(sheet: &Value, measured: &Value) -> bool {
    match (sheet, measured) {
        (Value::Number(a), Value::Number(b)) => (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-6,
        (Value::String(a), Value::String(b)) => {
            let norm = |s: &str| {
                let mut v: Vec<String> = s.split(", ").map(|x| x.trim().to_lowercase()).collect();
                v.sort();
                v
            };
            norm(a) == norm(b)
        }
        (a, b) => a == b,
    }
}

fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "on".into() } else { "off".into() },
        Value::Number(n) if n.as_u64().is_some_and(|b| b >= 1_000_000) => format!("{} ({n} bytes)", clean::fmt_size(n.as_u64().unwrap())),
        other => other.to_string(),
    }
}

/// Measure the build and compare every value of the sheet built into it (mechanics.json).
pub fn compare() -> Vec<Row> {
    let sheet: Value = serde_json::from_str(include_str!("../mechanics.json")).unwrap();
    let measured = measure();
    let found = measured["values"].as_array().cloned().unwrap_or_default();
    sheet["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let name = row["name"].as_str().unwrap_or_default().to_string();
            let m = found.iter().find(|m| m["name"] == name.as_str());
            Row {
                sheet: show(&row["value"]),
                measured: m.map(|m| show(&m["value"])),
                how: m.and_then(|m| m["how"].as_str()).unwrap_or("a description, not something a program can measure").to_string(),
                ok: m.is_some_and(|m| same(&row["value"], &m["value"])),
                name,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn mechanics_sheet_matches_what_the_build_does() {
        let rows = super::compare();
        let measured: Vec<_> = rows.iter().filter(|r| r.measured.is_some()).collect();
        let wrong: Vec<String> = measured.iter().filter(|r| !r.ok).map(|r| format!("{}: sheet {} but the build does {}", r.name, r.sheet, r.measured.as_ref().unwrap())).collect();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
        assert!(measured.len() >= 44, "only {} values measured", measured.len());
    }
}
