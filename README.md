# Claybroom

A free, open-source disk cleaner for macOS, Windows and Linux, written in Rust with egui.

* **Super Clean** (Claybroom's own): one scan for junk, browser data, duplicates, photos, unused apps and
  startup items; safe junk is ticked, everything personal is a suggestion you tick yourself
* **Quick Clean**: caches, logs, the Trash and browser data in one pass
* **Clean clutter**: app caches, logs, Trash, Downloads, developer junk, external-drive junk, large files
* **Clean browser**: caches, history and cookies of Safari, Chrome, Firefox, Opera, Edge, Brave, Vivaldi
  * scheduled cleaning (daily, weekly, monthly), a cookie allowlist with bookmark import, and
    "only delete history older than…"
* **Find duplicates**: files with identical content (BLAKE3), keep newest or oldest
* **Analyze photos**: blurry, too dark or bright, low-contrast and look-alike photos
* **Uninstall apps**: with leftovers on macOS, the app's own uninstaller on Windows, Flatpak on Linux
* **Manage startups**: turn login items, launch agents and autostart entries off/on or remove them
* **System monitor** and **Statistics**
* **Large files** in your home folder or on the whole computer; **HEIC photos** on macOS
* **Update check** (off by default): asks GitHub for a newer release, nothing else is sent
* **Menu-bar / system-tray icon**: Claybroom keeps running there when its window is closed. Click it
  for a small System monitor panel (recommendations, CPU, memory, sensors, disks, battery); right-click
  for Open, Quick Clean and Quit. Turn it off in Settings › General. On Linux the icon shows the menu only.

Nothing is removed without a confirmation. Personal files go to the Trash; regenerable junk is deleted.

## Download

* **macOS** (Apple silicon and Intel): `Claybroom-v<version>.dmg`. Open it, drag Claybroom to
  Applications. The first time, right-click Claybroom › Open (it isn't notarised by Apple yet).
* **Windows**: `Claybroom-v<version>-windows-x64.exe`. Windows SmartScreen may ask once: More info ›
  Run anyway (it isn't code-signed yet).
* **Linux**: `Claybroom-v<version>-linux-x64.tar.gz`, then run `claybroom` inside it. The tray icon
  needs GTK 3 and libayatana-appindicator3 (installed on most desktops).

Releases are built by `.github/workflows/release.yml` when a tag like `v0.1.0` is pushed.
To make the macOS disk image yourself: `sh packaging/macos-dmg.sh` (writes `dist/`).

## Build

```sh
cargo build --release
./target/release/claybroom
```

Options: `--page <quick|clutter|browser|duplicates|photos|apps|startup|monitor|stats|settings|tray>` opens a
screen directly; `--screenshot <file.png>` saves a picture of the window and exits.

Tests: `cargo test` (includes a check that `mechanics.json` matches what the build measurably does; `--mechanics out.json` writes the measurement). In the app: Settings › General › CHECK THE RULES shows each rule with ✓ or ✗. A read-only scan of your computer: `cargo test -- --ignored --nocapture`.

On macOS, allow Claybroom in System Settings › Privacy & Security › Full Disk Access to see the
Trash, Mail and every browser.

The cleaning rules and where each value comes from are listed in `MECHANICS.md`.

## Licence

MIT (see `LICENSE`). Bundled fonts: Roboto (Apache 2.0) and Source Serif 4 (SIL OFL 1.1); see `assets/FONTS-LICENSE.txt`.
