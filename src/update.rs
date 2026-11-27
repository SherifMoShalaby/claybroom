//! Opt-in check for a newer Claybroom on the project's GitHub releases page (off by default:
//! it's the only thing Claybroom ever asks the internet). Uses the system's `curl`, which macOS,
//! Windows 10+ and Linux desktops ship, so no network library is bundled.

use std::process::Command;

pub struct Release {
    pub version: String,
    pub url: String,
}

/// "owner/name" from `repository` in Cargo.toml, once the project is published on GitHub.
pub fn repo() -> Option<&'static str> {
    env!("CARGO_PKG_REPOSITORY").strip_prefix("https://github.com/").map(|r| r.trim_end_matches('/')).filter(|r| r.contains('/'))
}

fn quiet(cmd: &mut Command) -> &mut Command {
    // No console window flashing up on Windows.
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(cmd, 0x0800_0000);
    cmd
}

/// The latest release, if it's newer than this build.
pub fn latest() -> Result<Option<Release>, String> {
    let repo = repo().ok_or("Claybroom isn't published on GitHub yet, so there's nothing to check")?;
    let out = quiet(&mut Command::new("curl")).args(["-fsSL", "--max-time", "15", "-H", "Accept: application/vnd.github+json"])
        .arg(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .output()
        .map_err(|_| "curl isn't available".to_string())?;
    if !out.status.success() {
        return Err("couldn't reach GitHub".into());
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| "GitHub's answer wasn't readable".to_string())?;
    let tag = v["tag_name"].as_str().ok_or("no release found")?.trim_start_matches('v').to_string();
    let url = v["html_url"].as_str().unwrap_or_default().to_string();
    Ok(newer(&tag, env!("CARGO_PKG_VERSION")).then_some(Release { version: tag, url }))
}

/// "0.10.0" is newer than "0.9.2": compared number by number.
pub fn newer(a: &str, b: &str) -> bool {
    let nums = |v: &str| v.split(['.', '-']).map(|x| x.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    nums(a) > nums(b)
}

/// Open a web page in the default browser.
pub fn open_page(url: &str) {
    if !url.starts_with("https://") {
        return;
    }
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();
    #[cfg(windows)]
    let _ = quiet(&mut Command::new("explorer")).arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(url).spawn();
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions_compare_by_number() {
        assert!(super::newer("0.10.0", "0.9.2"));
        assert!(super::newer("1.0.0", "0.1.0"));
        assert!(!super::newer("0.1.0", "0.1.0"));
        assert!(!super::newer("0.0.9", "0.1.0"));
    }
}
