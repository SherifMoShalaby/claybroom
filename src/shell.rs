//! Places Claybroom reaches outside its window: a "Find duplicates with
//! Claybroom" item on folders' right-click menu (a Finder Quick
//! Action, an Explorer menu item, a GNOME Files script) and system notifications for the alerts.

use std::path::PathBuf;
use std::process::Command;

pub const FOLDER_ACTION: &str = "Find duplicates with Claybroom";

/// What the folder action runs: Claybroom's app (or program), opened on that folder's duplicates.
fn exe() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// The .app bundle holding this program, when it runs from one.
#[cfg(target_os = "macos")]
fn bundle(exe: &std::path::Path) -> Option<PathBuf> {
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(|p| p.to_path_buf())
}

#[cfg(target_os = "macos")]
fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Add or remove the folder right-click item. Errors are returned as text.
pub fn set_folder_action(on: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let dir = crate::clean::home().join("Library/Services").join(format!("{FOLDER_ACTION}.workflow"));
        if !on {
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
            }
            let _ = Command::new("/System/Library/CoreServices/pbs").arg("-update").status();
            return Ok(());
        }
        let exe = exe().ok_or("no program path")?;
        let q = |p: &std::path::Path| format!("'{}'", p.display().to_string().replace('\'', r"'\''"));
        let run = match bundle(&exe) {
            Some(app) => format!("open -n -a {} --args --page duplicates --scan-folder \"$1\"", q(&app)),
            None => format!("{} --page duplicates --scan-folder \"$1\" >/dev/null 2>&1 &", q(&exe)),
        };
        write_workflow(&dir, &run)?;
        let _ = Command::new("/System/Library/CoreServices/pbs").arg("-update").status();
        Ok(())
    }
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let path = r"Software\Classes\Directory\shell\ClaybroomDuplicates";
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if !on {
            let _ = hkcu.delete_subkey_all(path);
            return Ok(());
        }
        let exe = exe().ok_or("no program path")?.display().to_string();
        let (key, _) = hkcu.create_subkey(path).map_err(|e| e.to_string())?;
        key.set_value("", &FOLDER_ACTION).map_err(|e| e.to_string())?;
        key.set_value("Icon", &exe).map_err(|e| e.to_string())?;
        let (cmd, _) = key.create_subkey("command").map_err(|e| e.to_string())?;
        cmd.set_value("", &format!("\"{exe}\" --page duplicates --scan-folder \"%1\"")).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let file = crate::clean::home().join(".local/share/nautilus/scripts").join(FOLDER_ACTION);
        if !on {
            let _ = std::fs::remove_file(&file);
            return Ok(());
        }
        let exe = exe().ok_or("no program path")?.display().to_string().replace('\'', r"'\''");
        let script = format!(
            "#!/bin/sh\n# Added by Claybroom (Settings › General). GNOME Files: right-click a folder › Scripts.\np=$(printf '%s\\n' \"$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS\" | head -n1)\n[ -n \"$p\" ] || p=\"$PWD/$1\"\nexec '{exe}' --page duplicates --scan-folder \"$p\"\n"
        );
        std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&file, script).map_err(|e| e.to_string())?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// The album photos from the Photos library are put in for the person to delete.
pub const REVIEW_ALBUM: &str = "Claybroom: review and delete";

/// The AppleScript that puts these Photos media items in the review album (ids are checked to be
/// plain "<uuid>/L0/001" text first).
#[cfg(target_os = "macos")]
pub fn review_album_script(ids: &[String]) -> String {
    let list: Vec<String> = ids
        .iter()
        .filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/'))
        .map(|id| format!("\"{id}\""))
        .collect();
    format!(
        "tell application \"Photos\"\n\
         \tif not (exists album \"{a}\") then make new album named \"{a}\"\n\
         \tset found to {{}}\n\
         \trepeat with i in {{{l}}}\n\
         \t\ttry\n\
         \t\t\tset end of found to media item id (contents of i)\n\
         \t\tend try\n\
         \tend repeat\n\
         \tif (count of found) > 0 then add found to album \"{a}\"\n\
         \treturn count of found\n\
         end tell",
        a = REVIEW_ALBUM,
        l = list.join(", ")
    )
}

/// Ask Photos to put these library photos in the review album; how many it found.
pub fn photos_review_album(ids: &[String]) -> Result<usize, String> {
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("osascript").arg("-e").arg(review_album_script(ids)).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            let e = String::from_utf8_lossy(&out.stderr);
            return Err(if e.contains("-1743") { "Claybroom isn't allowed to control Photos (System Settings › Privacy & Security › Automation)".into() } else { e.trim().to_string() });
        }
        String::from_utf8_lossy(&out.stdout).trim().parse().map_err(|_| "Photos gave no answer".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ids;
        Err("the Photos app is only on macOS".into())
    }
}

/// A path quoted for /bin/sh.
#[cfg(unix)]
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Text inside an AppleScript string literal.
#[cfg(target_os = "macos")]
fn as_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Run a /bin/sh script as administrator, asking with the system's own prompt: macOS's
/// administrator dialog (which offers Touch ID where macOS allows it), or pkexec on Linux.
#[cfg(unix)]
pub fn admin_sh(script: &str, why: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let out = {
        let apple = format!("do shell script \"{}\" with prompt \"{}\" with administrator privileges", as_string(script), as_string(why));
        Command::new("osascript").arg("-e").arg(apple).output().map_err(|e| e.to_string())?
    };
    #[cfg(not(target_os = "macos"))]
    let out = {
        let _ = why;
        Command::new("pkexec").args(["/bin/sh", "-c", script]).output().map_err(|e| e.to_string())?
    };
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    // osascript reports a cancelled dialog as error -128; pkexec as status 126.
    if err.contains("-128") || out.status.code() == Some(126) {
        Err("cancelled".into())
    } else {
        Err(err.trim().to_string())
    }
}

/// Remove files Claybroom found but couldn't remove without an administrator, after asking.
pub fn admin_delete(paths: &[PathBuf]) -> Result<(), String> {
    let paths: Vec<&PathBuf> = paths.iter().filter(|p| p.is_absolute() && !crate::clean::protected(p)).collect();
    if paths.is_empty() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        let list: Vec<String> = paths.iter().map(|p| sh_quote(&p.display().to_string())).collect();
        admin_sh(&format!("/bin/rm -rf -- {}", list.join(" ")), &format!("Claybroom needs an administrator to remove {} system item(s) it found.", paths.len()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // One elevated cmd (Windows asks with its UAC prompt) removing every item.
        let steps: Vec<String> = paths
            .iter()
            .map(|p| if p.is_dir() { format!("rmdir /s /q \"{}\"", p.display()) } else { format!("del /f /q \"{}\"", p.display()) })
            .collect();
        let ps = format!("Start-Process -FilePath cmd.exe -ArgumentList '/c {}' -Verb RunAs -Wait -WindowStyle Hidden", steps.join(" & ").replace('\'', "''"));
        let out = Command::new("powershell").args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps]).creation_flags(0x0800_0000).output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok(()) } else { Err("cancelled".into()) }
    }
}

/// The Quick Action bundle: its service entry and its one shell step.
#[cfg(target_os = "macos")]
fn write_workflow(dir: &std::path::Path, run: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir.join("Contents")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("Contents/Info.plist"), INFO_PLIST.replace("NAME", FOLDER_ACTION)).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("Contents/document.wflow"), WFLOW.replace("SCRIPT", &xml(run))).map_err(|e| e.to_string())
}

/// A system notification (the alerts, while Claybroom runs in the background).
pub fn notify(body: &str) {
    #[cfg(target_os = "macos")]
    {
        let esc = body.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = Command::new("osascript").arg("-e").arg(format!("display notification \"{esc}\" with title \"Claybroom\"")).spawn();
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let esc = body.replace('\'', "''");
        let ps = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
             $t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
             $x = $t.GetElementsByTagName('text'); $x.Item(0).AppendChild($t.CreateTextNode('Claybroom')) > $null; \
             $x.Item(1).AppendChild($t.CreateTextNode('{esc}')) > $null; \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\\WindowsPowerShell\\v1.0\\powershell.exe').Show([Windows.UI.Notifications.ToastNotification]::new($t))"
        );
        let _ = Command::new("powershell").args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps]).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("notify-send").args(["Claybroom", body]).spawn();
    }
}

#[cfg(target_os = "macos")]
const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSBackgroundColorName</key>
			<string>background</string>
			<key>NSIconName</key>
			<string>NSActionTemplate</string>
			<key>NSMenuItem</key>
			<dict>
				<key>default</key>
				<string>NAME</string>
			</dict>
			<key>NSMessage</key>
			<string>runWorkflowAsService</string>
			<key>NSRequiredContext</key>
			<dict>
				<key>NSApplicationIdentifier</key>
				<string>com.apple.finder</string>
			</dict>
			<key>NSSendFileTypes</key>
			<array>
				<string>public.folder</string>
			</array>
		</dict>
	</array>
</dict>
</plist>
"#;

/// An Automator Quick Action with one "Run Shell Script" step that gets the folder as `$1`.
#[cfg(target_os = "macos")]
const WFLOW: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key>
	<string>523</string>
	<key>AMApplicationVersion</key>
	<string>2.10</string>
	<key>AMDocumentVersion</key>
	<string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Optional</key>
					<true/>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>AMActionVersion</key>
				<string>2.0.3</string>
				<key>AMApplication</key>
				<array>
					<string>Automator</string>
				</array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key>
					<dict/>
					<key>CheckedForUserDefaultShell</key>
					<dict/>
					<key>inputMethod</key>
					<dict/>
					<key>shell</key>
					<dict/>
					<key>source</key>
					<dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>ActionBundlePath</key>
				<string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key>
				<string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key>
					<string>SCRIPT</string>
					<key>CheckedForUserDefaultShell</key>
					<true/>
					<key>inputMethod</key>
					<integer>1</integer>
					<key>shell</key>
					<string>/bin/sh</string>
					<key>source</key>
					<string></string>
				</dict>
				<key>BundleIdentifier</key>
				<string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key>
				<string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key>
				<false/>
				<key>CanShowWhenRun</key>
				<true/>
				<key>Category</key>
				<array>
					<string>AMCategoryUtilities</string>
				</array>
				<key>Class Name</key>
				<string>RunShellScriptAction</string>
				<key>InputUUID</key>
				<string>6F1D7F2A-3C3B-4C55-9E1A-0A1E5C0D2B11</string>
				<key>Keywords</key>
				<array>
					<string>Shell</string>
					<string>Script</string>
				</array>
				<key>OutputUUID</key>
				<string>8A2E1C44-5B6D-4E77-8F90-1B2C3D4E5F60</string>
				<key>UUID</key>
				<string>2C4D6E8F-0A1B-4C3D-9E5F-7A8B9C0D1E2F</string>
				<key>UnlocalizedApplications</key>
				<array>
					<string>Automator</string>
				</array>
				<key>arguments</key>
				<dict/>
				<key>isViewVisible</key>
				<integer>1</integer>
				<key>location</key>
				<string>309.000000:253.000000</string>
				<key>nibPath</key>
				<string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib</string>
			</dict>
			<key>isViewVisible</key>
			<integer>1</integer>
		</dict>
	</array>
	<key>connectors</key>
	<dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key>
		<dict/>
		<key>applicationPaths</key>
		<array/>
		<key>inputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject</string>
		<key>outputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>presentationMode</key>
		<integer>15</integer>
		<key>processesInput</key>
		<integer>0</integer>
		<key>serviceInputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject.folder</string>
		<key>serviceOutputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key>
		<integer>0</integer>
		<key>systemImageName</key>
		<string>NSActionTemplate</string>
		<key>useAutomaticInputType</key>
		<integer>0</integer>
		<key>workflowTypeIdentifier</key>
		<string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#;

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// The Quick Action's files are valid property lists that keep the command intact.
    #[test]
    fn quick_action_files_are_valid() {
        let dir = std::env::temp_dir().join(format!("claybroom-qa-{}/x.workflow", std::process::id()));
        let run = "open -n -a '/Applications/Open & Sweep.app' --args --page duplicates --scan-folder \"$1\"";
        super::write_workflow(&dir, run).unwrap();
        for f in ["Info.plist", "document.wflow"] {
            let ok = std::process::Command::new("plutil").arg("-lint").arg(dir.join("Contents").join(f)).output().unwrap();
            assert!(ok.status.success(), "{f}: {}", String::from_utf8_lossy(&ok.stdout));
        }
        let v = plist::Value::from_file(dir.join("Contents/document.wflow")).unwrap();
        let cmd = v.as_dictionary().unwrap()["actions"].as_array().unwrap()[0].as_dictionary().unwrap()["action"].as_dictionary().unwrap()["ActionParameters"]
            .as_dictionary().unwrap()["COMMAND_STRING"].as_string().unwrap().to_string();
        assert_eq!(cmd, run);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}

#[cfg(all(test, unix))]
mod quoting {
    const NASTY: &str = "/tmp/My \"odd\" folder/it's a \\ test $HOME `x`";

    /// A path quoted for the shell reaches the command exactly as it is.
    #[test]
    fn shell_quoting_keeps_any_path() {
        let out = std::process::Command::new("/bin/sh").arg("-c").arg(format!("printf %s {}", super::sh_quote(NASTY))).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), NASTY);
    }

    /// The administrator script survives AppleScript's string escaping unchanged.
    #[cfg(target_os = "macos")]
    #[test]
    fn applescript_escaping_keeps_the_script() {
        let script = format!("/bin/echo -- {}", super::sh_quote(NASTY));
        let out = std::process::Command::new("osascript").arg("-e").arg(format!("return \"{}\"", super::as_string(&script))).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end_matches('\n'), script);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod album {
    /// The Photos script compiles against Photos' own dictionary (without running it) and keeps
    /// only well-formed ids.
    #[test]
    fn review_album_script_compiles() {
        let s = super::review_album_script(&["A1B2-C3/L0/001".into(), "bad\" & do shell script \"x".into()]);
        assert!(s.contains("\"A1B2-C3/L0/001\"") && !s.contains("do shell script"));
        let out = std::env::temp_dir().join(format!("claybroom-album-{}.scpt", std::process::id()));
        let ok = std::process::Command::new("osacompile").arg("-o").arg(&out).arg("-e").arg(&s).output().unwrap();
        assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
        let _ = std::fs::remove_file(&out);
    }
}
