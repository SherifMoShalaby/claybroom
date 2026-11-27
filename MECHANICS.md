# How Claybroom cleans

The rules Claybroom follows: what it scans, what it ticks for you, and what it never touches.
`mechanics.json` lists the same values in machine-readable form, and `cargo test` checks that the
build does what it says (see the end of this file). Values marked **estimated** are best guesses
that haven't been checked on that operating system yet.

## Safety

* Nothing is removed without a Clean button and a confirmation.
* Personal files (Downloads, large files, duplicates, photos) go to the Trash. Regenerable junk
  (caches, logs, browser data) and the Trash itself are deleted permanently.
* Protected folders are never touched: `/System/` on macOS; `C:\Windows\System32` on Windows and
  `/usr /bin /sbin /etc /boot /lib` on Linux (estimated).
* Symbolic links are never followed; unreadable files are skipped and counted.
* Saved passwords and autofill are never ticked for you and never part of scheduled cleaning.

## Clutter categories

| Category | Folder scanned (macOS) | Not cleaned |
|---|---|---|
| App caches | `~/Library/Caches`, each sandboxed app's `Data/Library/Caches` | `com.apple.Safari/Extensions`, `fontconfig`, `Java` |
| | `/Library/Caches` and `getconf DARWIN_USER_CACHE_DIR` (setting "Include system caches and logs") | `com.apple.FontRegistry`, `com.apple.metadata.mdworker` |
| Log files | `~/Library/Logs`, sandboxed apps' `Data/Library/Logs`; `/Library/Logs` (system setting) | |
| Trash | `~/.Trash` (empty folders removed too) | |
| Downloads | `~/Downloads` (setting "Include Downloads") and Mail's download folders | |
| Developer leftovers | `~/Library/Developer/Xcode/{DerivedData,Simulators}`, npm's `_cacache` | |
| Junk on external drives | each `/Volumes/*`: `.Trashes`, `.DS_Store`, `._*` files, `.fseventsd`, `.Spotlight-V100`, `.TemporaryItems`, `.VolumeIcon.icns` | `MobileBackups`; Mac OS Extended (HFS) drives |
| Large files | the home folder (or the whole computer, setting) | Applications |

Windows (estimated): app caches = `%TEMP%`, `%LOCALAPPDATA%\Temp`; system cache = `C:\Windows\Temp`;
logs = `%LOCALAPPDATA%\CrashDumps`, `C:\Windows\Logs`; Trash = `X:\$Recycle.Bin\<own SID>`.
Linux (estimated): app caches = `~/.cache`; system cache = `/var/cache`; logs = `/var/log`;
Trash = `~/.local/share/Trash/{files,info}`.

## Thresholds and defaults

| Setting | Options | Default |
|---|---|---|
| Large files minimum size | 250 MB, 500 MB, 1 GB, 2 GB, 5 GB, 10 GB | 1 GB |
| Duplicates minimum size | 16 KB, 500 KB, 1 MB, 10 MB, 50 MB | 500 KB |
| Large apps | 500 MB, 1, 2, 5, 10, 15 GB | 1 GB |
| Unused apps | 1, 3, 6 months, 1 year | 6 months or more |

Skip hidden files when finding duplicates: **on**. Send removed copies to the Trash: **on**.
Include Downloads, external drives, system caches and logs: **on**.
Size labels use decimal units (1 KB = 1000 bytes).

## Browsers

Safari, Chrome, Firefox, Opera, Edge, Brave, Vivaldi. Categories: Cookies, Caches, Browsing
history, Saved form data, Autofill passwords. Suggested: caches and history
ticked, everything else unticked. A running browser is skipped and Claybroom says which ones to close.

Extras:
* **Cookie allowlist** (with import from bookmarks): Chrome-family and Firefox cookie rows of other
  sites are deleted; Safari's `Cookies.binarycookies` is rewritten keeping the allowlisted sites'
  records byte for byte. A file that doesn't rebuild exactly as read is left alone.
* **Only delete history older than…**: Chrome-family, Firefox and Safari history databases are
  edited row by row. Firefox bookmarks in `places.sqlite` are kept.
* **Scheduled cleaning** (Never / Daily / Weekly / Monthly per browser) and **automatic Trash
  emptying** (items older than 1/3/7/14/30 days, default off, 7 days): an hourly background job
  (launch agent on macOS, scheduled task on Windows, systemd user timer on Linux) runs
  `claybroom --scheduled-clean`. The job exists only while a schedule is on.

## Duplicates

* Duplicates are files with 100% identical content, names may differ. Same-size files are compared
  by a hash of their first 64 KB, then whole (BLAKE3). `node_modules` is skipped.
* Whole folders that are copies are listed too (every file inside has an identical twin at the same
  place; `.DS_Store`, `Thumbs.db`, `desktop.ini` don't count).
* Groups: Pictures, Songs, Videos, Documents, Others (by extension). Preselect: Keep newest
  (default) / Keep oldest / Select none. A warning appears when every copy in a group is ticked.

## Photos

Looks in Pictures, Desktop, Documents, Downloads and (macOS, setting) the Photos library, read only.
JPEG, PNG and HEIC over 50 KB, at most 3000, read at 256 px:
* weak photo: brightness under 35 or over 225, variance of the Laplacian under 60 (blurry),
  brightness spread under 18 (poor contrast) (estimated);
* look-alikes: 64-bit difference hash at most 5 bits from the group's first photo, same shape,
  taken within 10 minutes (estimated). The sharpest stays unticked.

Photos-library photos are never deleted as files: Claybroom puts them in a Photos album to review.
HEIC is read through `sips` on macOS, or ImageMagick, `heif-convert`, ffmpeg or Windows' own HEIF
decoder elsewhere.

## Apps and startup items

* Apps are listed at once, then sizes, last-used dates and leftovers are measured in the background.
  macOS leftovers: `~/Library/{Caches, Preferences, Application Support, Logs, Containers, Saved
  Application State, HTTPStorages, WebKit, Cookies, LaunchAgents, Group Containers, Application
  Scripts}` and `$TMPDIR`, by bundle id or name. The app and its leftovers go to the Trash.
  Windows runs the app's own uninstaller. Linux removes Flatpak and Snap apps, and apps owned by
  dpkg/apt, rpm/dnf or pacman (through pkexec).
* Startup items: login items, launch agents and launch daemons. Off moves the item to Claybroom's
  own store so On can put it back. System-wide items need an administrator.
* Items the system refuses for lack of permission can be removed with the system's own
  administrator prompt; only those exact items, never protected paths.

## Super Clean

One scan runs junk and browsers, duplicates, photos, unused apps and startup items at the same
time. Results come in two lanes: **Safe to clean now** (junk, recommended selection) and **Your
call** (duplicates, photos, apps, startup items, never ticked for you). Design notes:
`docs/super-clean-options.md`.

## Statistics

All time or past month: space and items removed, as "enough space for about" N pictures (3 MB),
minutes of video (16 MB/min) or songs (4 MB). Includes what the background job cleaned.

## Privacy and updates

Claybroom sends nothing anywhere. The only network call is the update check (off by default): it
asks GitHub's releases API, through the system's `curl`, for the repository named in `Cargo.toml`.
Updates are downloaded by the person, never installed silently.

## How this file is checked against the build

`claybroom --mechanics mechanics-measured.json` runs the real scanners on throw-away files in the
temp folder and writes what the build actually does: size limits from files one byte below and at
each limit, excludes from folders holding the excluded names, selection rules from sample files,
and lists and defaults from the code the app uses. `cargo test` compares every measured value with
`mechanics.json` and fails on any difference.
