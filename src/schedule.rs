//! Scheduled browser cleaning and automatic Trash emptying. A
//! small background job starts `claybroom --scheduled-clean` every hour, even while Claybroom is
//! closed; it cleans what is due and exits. The job exists only while a schedule is on.

use crate::browser_pro::BrowserOpts;
use crate::clean::{self, Settings};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Never, Daily, Weekly, Monthly.
pub const FREQ_NAMES: [&str; 4] = ["Never", "Daily", "Weekly", "Monthly"];
pub const FREQ_SECS: [u64; 4] = [0, 86_400, 7 * 86_400, 30 * 86_400];
/// How often the background job wakes up to look (estimated).
pub const CHECK_SECS: u64 = 3600;
/// "Only delete browser history older than" choices.
pub const HISTORY_AGES: [(u64, &str); 5] = [(86_400, "1 day"), (7 * 86_400, "1 week"), (14 * 86_400, "2 weeks"), (30 * 86_400, "1 month"), (91 * 86_400, "3 months")];

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Schedule {
    pub freq: usize,
    pub cache: bool,
    pub history: bool,
    pub cookies: bool,
}

impl Default for Schedule {
    fn default() -> Self {
        Self { freq: 0, cache: true, history: true, cookies: false }
    }
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn due(last: u64, now: u64, freq: usize) -> bool {
    freq > 0 && now.saturating_sub(last) >= FREQ_SECS[freq.min(3)]
}

/// Settings › Browser Cleaning as cleaning options.
pub fn browser_opts(s: &Settings) -> BrowserOpts {
    BrowserOpts {
        allow: s.allowlist.clone(),
        history_before: s.history_older.then(|| now().saturating_sub(HISTORY_AGES[s.history_age_idx.min(4)].0)),
    }
}

/// Whether the background job is needed.
pub fn wanted(s: &Settings) -> bool {
    s.trash_auto || s.schedules.values().any(|x| x.freq > 0)
}

fn dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(clean::home).join("claybroom")
}

/// The app keeps a copy of its settings here for the background job.
pub fn write_settings(s: &Settings) {
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(dir().join("settings.json"), serde_json::to_vec_pretty(s).unwrap_or_default());
}

fn read_json<T: serde::de::DeserializeOwned + Default>(name: &str) -> T {
    std::fs::read(dir().join(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// When each browser was last cleaned on schedule.
pub fn last_runs() -> BTreeMap<String, u64> {
    read_json("schedule-state.json")
}

/// Cleanings the background job did, for the Statistics page: (time, key, bytes, items).
pub fn take_pending_log() -> Vec<(u64, String, u64, u64)> {
    let v: Vec<(u64, String, u64, u64)> = read_json("scheduled-log.json");
    if !v.is_empty() {
        let _ = std::fs::write(dir().join("scheduled-log.json"), b"[]");
    }
    v
}

/// `claybroom --scheduled-clean`: clean the browsers that are due and closed, empty old Trash.
pub fn run_scheduled() {
    let s: Settings = read_json("settings.json");
    let mut last = last_runs();
    let mut log: Vec<(u64, String, u64, u64)> = read_json("scheduled-log.json");
    let t = now();
    let running = clean::running_processes();
    let opts = browser_opts(&s);
    let p = clean::Progress::default();
    for (name, sch) in &s.schedules {
        if !due(last.get(name).copied().unwrap_or(0), t, sch.freq) || s.browsers_off.contains(name) {
            continue;
        }
        let others: Vec<String> = clean::browser_defs().iter().map(|b| b.name.to_string()).filter(|n| n != name).collect();
        let Some(b) = clean::scan_browsers(&p, &others).into_iter().next() else { continue };
        let mut cats: Vec<clean::Category> = b
            .cats
            .into_iter()
            .filter(|c| match c.title.as_str() {
                "Caches" => sch.cache,
                "Browsing history" => sch.history,
                "Cookies" => sch.cookies,
                // Autofill and saved passwords are only ever cleaned by hand.
                _ => false,
            })
            .collect();
        if cats.iter().flat_map(|c| &c.items).any(|i| i.procs.iter().any(|n| running.iter().any(|r| r == n))) {
            continue; // open: try again next hour
        }
        cats.iter_mut().flat_map(|c| c.items.iter_mut()).for_each(|i| i.selected = true);
        let r = clean::clean_categories_with(&cats, &p, &opts);
        for (title, browser, bytes, items) in &r.by_cat {
            if *items > 0 {
                log.push((t, crate::stat_key(title, *browser).to_string(), *bytes, *items as u64));
            }
        }
        last.insert(name.clone(), t);
    }
    if s.trash_auto {
        let r = clean::empty_old_trash(clean::TRASH_DAYS[s.trash_days_idx.min(4)]);
        if r.removed > 0 {
            log.push((t, "Trash".into(), r.freed, r.removed as u64));
        }
    }
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(dir().join("schedule-state.json"), serde_json::to_vec(&last).unwrap_or_default());
    let _ = std::fs::write(dir().join("scheduled-log.json"), serde_json::to_vec(&log).unwrap_or_default());
}

/// Set up or remove the hourly background job (a launch agent on macOS, a scheduled task on
/// Windows, a systemd user timer on Linux).
pub fn sync_agent(on: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        let plist = clean::home().join("Library/LaunchAgents/io.github.claybroom.schedule.plist");
        let _ = std::process::Command::new("launchctl").arg("unload").arg(&plist).output();
        if !on {
            let _ = std::fs::remove_file(&plist);
            return Ok(());
        }
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>io.github.claybroom.schedule</string>
  <key>ProgramArguments</key><array><string>{}</string><string>--scheduled-clean</string></array>
  <key>StartInterval</key><integer>{CHECK_SECS}</integer>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Background</string>
</dict></plist>
"#,
            exe.display()
        );
        std::fs::create_dir_all(plist.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&plist, body).map_err(|e| e.to_string())?;
        std::process::Command::new("launchctl").arg("load").arg(&plist).output().map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(windows)]
    {
        let mut c = std::process::Command::new("schtasks");
        if on {
            c.args(["/Create", "/F", "/SC", "HOURLY", "/TN", "Claybroom schedule", "/TR"]).arg(format!("\"{}\" --scheduled-clean", exe.display()));
        } else {
            c.args(["/Delete", "/F", "/TN", "Claybroom schedule"]);
        }
        c.output().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let d = clean::home().join(".config/systemd/user");
        let ctl = |args: &[&str]| std::process::Command::new("systemctl").arg("--user").args(args).output();
        if !on {
            let _ = ctl(&["disable", "--now", "claybroom-schedule.timer"]);
            let _ = std::fs::remove_file(d.join("claybroom-schedule.timer"));
            let _ = std::fs::remove_file(d.join("claybroom-schedule.service"));
            return Ok(());
        }
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        std::fs::write(d.join("claybroom-schedule.service"), format!("[Unit]\nDescription=Claybroom scheduled cleaning\n\n[Service]\nType=oneshot\nExecStart=\"{}\" --scheduled-clean\n", exe.display())).map_err(|e| e.to_string())?;
        std::fs::write(d.join("claybroom-schedule.timer"), format!("[Unit]\nDescription=Claybroom scheduled cleaning\n\n[Timer]\nOnBootSec=5min\nOnUnitActiveSec={CHECK_SECS}s\n\n[Install]\nWantedBy=timers.target\n")).map_err(|e| e.to_string())?;
        ctl(&["daemon-reload"]).map_err(|e| e.to_string())?;
        ctl(&["enable", "--now", "claybroom-schedule.timer"]).map(|_| ()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn due_follows_the_frequency() {
        use super::due;
        let day = 86_400;
        assert!(!due(0, 10 * day, 0), "Never is never due");
        assert!(due(0, day, 1) && !due(day, day + 3600, 1), "Daily");
        assert!(!due(0, 6 * day, 2) && due(0, 7 * day, 2), "Weekly");
        assert!(!due(0, 29 * day, 3) && due(0, 30 * day, 3), "Monthly");
    }
}
