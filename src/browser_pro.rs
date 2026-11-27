//! Browser extras: a cookie allowlist that
//! keeps chosen websites signed in, importing that list from bookmarks, and deleting only history
//! older than a chosen age. They edit the browsers' own databases row by row instead of deleting
//! whole files. Browsers are always closed when this runs (running ones are skipped).

use rusqlite::Connection;
use std::path::Path;

/// Which browser database a cleaning item is, when it can be edited instead of deleted.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Db {
    #[default]
    None,
    ChromiumCookies,
    FirefoxCookies,
    SafariCookies,
    ChromiumHistory,
    FirefoxHistory,
    SafariHistory,
    /// Whole-file history extras (Visited Links, Top Sites, Shortcuts): kept when only old
    /// history is deleted.
    HistoryExtra,
    /// Chrome-family "Web Data": its autofill tables are emptied, search engines and the rest kept.
    ChromiumAutofill,
    /// Chrome-family "Login Data": its saved passwords are emptied.
    ChromiumLogins,
}

/// The tables `clean_rows` empties (only those a file has).
const AUTOFILL_TABLES: &[&str] = &[
    "autofill", "autofill_profiles", "autofill_profile_names", "autofill_profile_emails", "autofill_profile_phones",
    "autofill_profile_addresses", "credit_cards", "masked_credit_cards", "unmasked_credit_cards", "server_card_metadata",
    "server_addresses", "local_addresses", "local_addresses_type_tokens", "contact_info", "contact_info_type_tokens",
    "local_ibans", "masked_ibans",
];
const LOGIN_TABLES: &[&str] = &["logins", "insecure_credentials", "password_notes", "stats"];

/// Empty a Chrome-family database's autofill or saved-password tables, keeping the rest of the
/// file. Returns bytes freed.
pub fn clean_rows(path: &Path, db: Db) -> Result<u64, String> {
    let wanted = match db {
        Db::ChromiumAutofill => AUTOFILL_TABLES,
        Db::ChromiumLogins => LOGIN_TABLES,
        _ => return Err("not an autofill database".into()),
    };
    let before = file_len(path);
    let c = Connection::open(path).map_err(|e| e.to_string())?;
    let tables: Vec<String> = {
        let mut st = c.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").map_err(|e| e.to_string())?;
        st.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?.filter_map(Result::ok).collect()
    };
    for t in tables.iter().filter(|t| wanted.contains(&t.as_str())) {
        c.execute(&format!("DELETE FROM \"{t}\""), []).map_err(|e| e.to_string())?;
    }
    let _ = c.execute_batch("VACUUM");
    drop(c);
    Ok(before.saturating_sub(file_len(path)))
}

/// The browser options the cleaning follows.
#[derive(Clone, Default)]
pub struct BrowserOpts {
    pub allow: Vec<String>,
    /// Delete only history older than this moment (seconds since 1970).
    pub history_before: Option<u64>,
}

/// "mail.example.com" is kept by "example.com"; hosts may start with a dot in cookie databases.
pub fn host_allowed(host: &str, allow: &[String]) -> bool {
    let h = host.trim_start_matches('.').to_lowercase();
    allow.iter().any(|d| {
        let d = d.trim_start_matches('.').to_lowercase();
        h == d || h.ends_with(&format!(".{d}"))
    })
}

/// Safari's `Cookies.binarycookies`: "cook", the page count and each page's size (big-endian),
/// the pages, a checksum (big-endian sum of every 4th byte of every page), then a fixed marker
/// and a settings plist. A page: 00 00 01 00, the cookie count and each cookie's offset
/// (little-endian), 00 00 00 00, then the cookies. A cookie starts with its own size, and its
/// domain's offset sits 16 bytes in, both little-endian and relative to the cookie.
pub struct SafariCookies {
    pages: Vec<Vec<Vec<u8>>>,
    tail: Vec<u8>,
}

fn be32(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}
fn le32(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

impl SafariCookies {
    pub fn parse(b: &[u8]) -> Option<Self> {
        if b.get(..4)? != b"cook" {
            return None;
        }
        let n = be32(b, 4)?;
        let mut at = 8 + 4 * n;
        let mut pages = Vec::new();
        for k in 0..n {
            let size = be32(b, 8 + 4 * k)?;
            let page = b.get(at..at + size)?;
            if be32(page, 0)? != 0x100 {
                return None;
            }
            let count = le32(page, 4)?;
            let mut cookies = Vec::new();
            for c in 0..count {
                let off = le32(page, 8 + 4 * c)?;
                let len = le32(page, off)?;
                cookies.push(page.get(off..off + len)?.to_vec());
            }
            pages.push(cookies);
            at += size;
        }
        // The checksum is rebuilt on writing; keep what follows it as it is.
        let tail = b.get(at + 4..)?.to_vec();
        Some(Self { pages, tail })
    }

    pub fn domain(cookie: &[u8]) -> String {
        let off = le32(cookie, 16).unwrap_or(usize::MAX);
        let s = cookie.get(off..).unwrap_or_default();
        String::from_utf8_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]).into_owned()
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.pages.iter().map(Vec::len).sum()
    }

    /// Keep only the cookies `keep` says yes to; pages left empty go.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        for p in &mut self.pages {
            p.retain(|c| keep(&Self::domain(c)));
        }
        self.pages.retain(|p| !p.is_empty());
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let pages: Vec<Vec<u8>> = self
            .pages
            .iter()
            .map(|cookies| {
                let mut p = vec![0, 0, 1, 0];
                p.extend((cookies.len() as u32).to_le_bytes());
                let mut off = 12 + 4 * cookies.len();
                for c in cookies {
                    p.extend((off as u32).to_le_bytes());
                    off += c.len();
                }
                p.extend([0, 0, 0, 0]);
                cookies.iter().for_each(|c| p.extend(c));
                p
            })
            .collect();
        let mut out = b"cook".to_vec();
        out.extend((pages.len() as u32).to_be_bytes());
        pages.iter().for_each(|p| out.extend((p.len() as u32).to_be_bytes()));
        let mut sum: u32 = 0;
        for p in &pages {
            out.extend(p);
            sum = p.iter().step_by(4).fold(sum, |s, &x| s.wrapping_add(x as u32));
        }
        out.extend(sum.to_be_bytes());
        out.extend(&self.tail);
        out
    }
}

/// Delete Safari's cookies except the allowlisted sites'. Returns the bytes freed.
pub fn clean_safari_cookies(path: &Path, allow: &[String]) -> Result<u64, String> {
    let before = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut jar = SafariCookies::parse(&before).ok_or("Safari's cookie file isn't in a known format")?;
    // Only write what reads back exactly as it was: an unknown variant is left alone.
    if jar.to_bytes() != before {
        return Err("Safari's cookie file isn't in a known format".into());
    }
    jar.retain(|d| host_allowed(d, allow));
    let after = jar.to_bytes();
    let tmp = path.with_extension("claybroom-tmp");
    std::fs::write(&tmp, &after).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(before.len().saturating_sub(after.len()) as u64)
}

/// "https://www.example.com/page" → "example.com".
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', ':', '?', '#']).next()?.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    (host.contains('.') && !host.is_empty()).then_some(host)
}

fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// Delete every cookie except the allowlisted sites'. Returns bytes freed.
pub fn clean_cookies(path: &Path, db: Db, allow: &[String]) -> Result<u64, String> {
    let (table, col) = match db {
        Db::ChromiumCookies => ("cookies", "host_key"),
        Db::FirefoxCookies => ("moz_cookies", "host"),
        _ => return Err("not a cookie database".into()),
    };
    let before = file_len(path);
    let c = Connection::open(path).map_err(|e| e.to_string())?;
    let hosts: Vec<String> = {
        let mut st = c.prepare(&format!("SELECT DISTINCT {col} FROM {table}")).map_err(|e| e.to_string())?;
        st.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?.filter_map(Result::ok).collect()
    };
    for h in hosts.iter().filter(|h| !host_allowed(h, allow)) {
        c.execute(&format!("DELETE FROM {table} WHERE {col} = ?1"), [h]).map_err(|e| e.to_string())?;
    }
    let _ = c.execute_batch("VACUUM");
    drop(c);
    Ok(before.saturating_sub(file_len(path)))
}

/// Delete history visits from before `before` (seconds since 1970), and pages no longer visited.
/// Firefox keeps bookmarked pages. Returns bytes freed.
pub fn clean_history(path: &Path, db: Db, before: u64) -> Result<u64, String> {
    let size = file_len(path);
    let c = Connection::open(path).map_err(|e| e.to_string())?;
    let (main, tidy): (String, &[&str]) = match db {
        // Chrome counts microseconds since 1601.
        Db::ChromiumHistory => (
            format!("DELETE FROM visits WHERE visit_time < {}", (before + 11_644_473_600) * 1_000_000),
            &[
                "DELETE FROM urls WHERE id NOT IN (SELECT url FROM visits)",
                "DELETE FROM keyword_search_terms WHERE url_id NOT IN (SELECT id FROM urls)",
                "DELETE FROM segment_usage WHERE segment_id NOT IN (SELECT id FROM segments)",
            ],
        ),
        // Firefox counts microseconds since 1970; bookmarked pages (foreign_count > 0) stay.
        Db::FirefoxHistory => (
            format!("DELETE FROM moz_historyvisits WHERE visit_date < {}", before * 1_000_000),
            &["DELETE FROM moz_places WHERE foreign_count = 0 AND id NOT IN (SELECT place_id FROM moz_historyvisits)"],
        ),
        // Safari counts seconds since 2001.
        Db::SafariHistory => (
            format!("DELETE FROM history_visits WHERE visit_time < {}", before as i64 - 978_307_200),
            &["DELETE FROM history_items WHERE id NOT IN (SELECT history_item FROM history_visits)"],
        ),
        _ => return Err("not a history database".into()),
    };
    c.execute_batch(&main).map_err(|e| e.to_string())?;
    for t in tidy {
        let _ = c.execute_batch(t); // older browser versions may lack a table
    }
    let _ = c.execute_batch("VACUUM");
    drop(c);
    Ok(size.saturating_sub(file_len(path)))
}

/// Websites from the bookmarks of Chrome-family browsers, Firefox and Safari (Pro: "Import
/// bookmarks" into the cookie allowlist).
pub fn bookmark_hosts() -> Vec<String> {
    let mut urls: Vec<String> = vec![];
    for b in crate::clean::browser_defs() {
        let Some(data) = &b.data else { continue };
        if b.chromium {
            let mut profiles: Vec<std::path::PathBuf> = std::fs::read_dir(data).into_iter().flatten().filter_map(Result::ok).map(|e| e.path()).collect();
            profiles.push(data.clone());
            for pr in profiles {
                let Ok(text) = std::fs::read_to_string(pr.join("Bookmarks")) else { continue };
                let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
                    if let Some(u) = v.get("url").and_then(|u| u.as_str()) {
                        out.push(u.to_string());
                    }
                    match v {
                        serde_json::Value::Object(m) => m.values().for_each(|x| walk(x, out)),
                        serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
                        _ => {}
                    }
                }
                walk(&json, &mut urls);
            }
        } else if b.name == "Firefox" {
            for pr in std::fs::read_dir(data).into_iter().flatten().filter_map(Result::ok) {
                let Ok(c) = Connection::open_with_flags(pr.path().join("places.sqlite"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
                if let Ok(mut st) = c.prepare("SELECT p.url FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id") {
                    if let Ok(rows) = st.query_map([], |r| r.get::<_, String>(0)) {
                        urls.extend(rows.filter_map(Result::ok));
                    }
                }
            }
        } else {
            // Safari (macOS only).
            #[cfg(target_os = "macos")]
            if let Ok(v) = plist::Value::from_file(crate::clean::home().join("Library/Safari/Bookmarks.plist")) {
                fn walk(v: &plist::Value, out: &mut Vec<String>) {
                    if let Some(d) = v.as_dictionary() {
                        if let Some(u) = d.get("URLString").and_then(|u| u.as_string()) {
                            out.push(u.to_string());
                        }
                        d.values().for_each(|x| walk(x, out));
                    } else if let Some(a) = v.as_array() {
                        a.iter().for_each(|x| walk(x, out));
                    }
                }
                walk(&v, &mut urls);
            }
        }
    }
    let mut hosts: Vec<String> = urls.iter().filter_map(|u| host_of(u)).collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(tag: &str, sql: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("claybroom-{tag}-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&p);
        Connection::open(&p).unwrap().execute_batch(sql).unwrap();
        p
    }

    fn rows(p: &Path, sql: &str) -> Vec<String> {
        let c = Connection::open(p).unwrap();
        let mut st = c.prepare(sql).unwrap();
        let mut v: Vec<String> = st.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect();
        v.sort();
        v
    }

    #[test]
    fn hosts_and_allowlist() {
        assert_eq!(host_of("https://www.Example.com:8080/a?b").as_deref(), Some("example.com"));
        assert_eq!(host_of("about:blank"), None);
        let allow = vec!["keep.org".to_string()];
        assert!(host_allowed(".keep.org", &allow) && host_allowed("mail.keep.org", &allow));
        assert!(!host_allowed("notkeep.org", &allow) && !host_allowed("keep.org.evil.com", &allow));
    }

    #[test]
    fn cookie_allowlist_keeps_only_listed_sites() {
        let p = temp_db("cookies", "CREATE TABLE cookies (host_key TEXT, name TEXT);
            INSERT INTO cookies VALUES ('.example.com','a'), ('.keep.org','b'), ('mail.keep.org','c'), ('ads.net','d');");
        clean_cookies(&p, Db::ChromiumCookies, &["keep.org".into()]).unwrap();
        assert_eq!(rows(&p, "SELECT host_key FROM cookies"), vec![".keep.org", "mail.keep.org"]);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn history_older_than_keeps_recent_visits_and_bookmarks() {
        let now = 1_800_000_000u64;
        let day = 86_400;
        // Chrome
        let old = (now - 2 * day + 11_644_473_600) * 1_000_000;
        let new = (now - 3600 + 11_644_473_600) * 1_000_000;
        let p = temp_db("chist", &format!("CREATE TABLE urls (id INTEGER, url TEXT); CREATE TABLE visits (url INTEGER, visit_time INTEGER);
            INSERT INTO urls VALUES (1,'old'),(2,'new'); INSERT INTO visits VALUES (1,{old}),(2,{new});"));
        clean_history(&p, Db::ChromiumHistory, now - day).unwrap();
        assert_eq!(rows(&p, "SELECT url FROM urls"), vec!["new"]);
        let _ = std::fs::remove_file(p);
        // Firefox: the old bookmarked page stays, the old plain page goes.
        let (o, n) = ((now - 2 * day) * 1_000_000, (now - 3600) * 1_000_000);
        let p = temp_db("fhist", &format!("CREATE TABLE moz_places (id INTEGER, url TEXT, foreign_count INTEGER); CREATE TABLE moz_historyvisits (place_id INTEGER, visit_date INTEGER);
            INSERT INTO moz_places VALUES (1,'old',0),(2,'new',0),(3,'bookmarked',1);
            INSERT INTO moz_historyvisits VALUES (1,{o}),(2,{n}),(3,{o});"));
        clean_history(&p, Db::FirefoxHistory, now - day).unwrap();
        assert_eq!(rows(&p, "SELECT url FROM moz_places"), vec!["bookmarked", "new"]);
        let _ = std::fs::remove_file(p);
        // Safari
        let (o, n) = (now as i64 - 2 * day as i64 - 978_307_200, now as i64 - 3600 - 978_307_200);
        let p = temp_db("shist", &format!("CREATE TABLE history_items (id INTEGER, url TEXT); CREATE TABLE history_visits (history_item INTEGER, visit_time REAL);
            INSERT INTO history_items VALUES (1,'old'),(2,'new'); INSERT INTO history_visits VALUES (1,{o}),(2,{n});"));
        clean_history(&p, Db::SafariHistory, now - day).unwrap();
        assert_eq!(rows(&p, "SELECT url FROM history_items"), vec!["new"]);
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod safari_cookies {
    use super::SafariCookies;

    fn cookie(domain: &str) -> Vec<u8> {
        // size, flags…, then the domain at offset 56 (the real layout's string area).
        let mut c = vec![0u8; 56];
        c[16..20].copy_from_slice(&56u32.to_le_bytes());
        c.extend(domain.as_bytes());
        c.push(0);
        let n = c.len() as u32;
        c[0..4].copy_from_slice(&n.to_le_bytes());
        c
    }

    #[test]
    fn allowlist_keeps_only_its_sites_and_the_file_stays_readable() {
        let jar = SafariCookies { pages: vec![vec![cookie(".google.com"), cookie("news.example.com")], vec![cookie(".tracker.net")]], tail: vec![7, 0x17, 0x20, 5, 0, 0, 0, 0x4b] };
        let bytes = jar.to_bytes();
        let mut back = SafariCookies::parse(&bytes).unwrap();
        assert_eq!(back.to_bytes(), bytes);
        back.retain(|d| super::host_allowed(d, &["example.com".into()]));
        assert_eq!(back.count(), 1);
        let again = SafariCookies::parse(&back.to_bytes()).unwrap();
        assert_eq!(SafariCookies::domain(&again.pages[0][0]), "news.example.com");
    }

    /// Read-only: this Mac's real Safari cookie file reads and rebuilds byte for byte.
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore]
    #[cfg(target_os = "macos")]
    fn real_safari_cookie_file_round_trips() {
        let p = crate::clean::home().join("Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies");
        let b = std::fs::read(&p).unwrap();
        let jar = SafariCookies::parse(&b).unwrap();
        println!("{} cookies in {} pages", jar.count(), jar.pages.len());
        assert!(jar.to_bytes() == b, "rebuild differs from the real file");
    }
}

#[cfg(test)]
mod autofill {
    /// Autofill rows go; search engines in the same file stay.
    #[test]
    fn autofill_tables_are_emptied_and_the_rest_kept() {
        let dir = std::env::temp_dir().join(format!("claybroom-autofill-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("Web Data");
        let c = rusqlite::Connection::open(&f).unwrap();
        c.execute_batch("CREATE TABLE autofill(name, value); INSERT INTO autofill VALUES ('email','a@b.c'); CREATE TABLE keywords(short_name); INSERT INTO keywords VALUES ('Search');").unwrap();
        drop(c);
        super::clean_rows(&f, super::Db::ChromiumAutofill).unwrap();
        let c = rusqlite::Connection::open(&f).unwrap();
        let n = |t: &str| c.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!((n("autofill"), n("keywords")), (0, 1));
        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
