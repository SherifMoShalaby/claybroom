# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Claybroom: a free, MIT-licensed disk cleaner for macOS, Windows and Linux, in Rust + egui (eframe).
Crate name `claybroom`.

## Commands

```sh
cargo build --release && ./target/release/claybroom
cargo test                                   # unit tests + mechanics.json check
cargo test <name>                            # single test, e.g. cargo test system_folder_is_protected
cargo test -- --ignored --nocapture          # read-only scans of the real computer
./target/release/claybroom --page clutter    # open a screen directly (quick|clutter|browser|duplicates|photos|apps|startup|monitor|stats|settings|tray)
./target/release/claybroom --page clutter --scan --screenshot out.png   # screenshot after the scan (--during: while scanning)
./target/release/claybroom --mechanics out.json   # measure what the build does, no window
sh packaging/macos-dmg.sh                    # universal .app + dist/Claybroom-v<version>.dmg
```

Other CLI modes in `src/main.rs`: `--scheduled-clean` (what the hourly background job runs) and
`--write-icon <png> <size>` (the icon is drawn in code by `kit::app_icon`; used by the DMG script).

Releases: `.github/workflows/release.yml` runs `cargo test` and builds on macOS/Windows/Ubuntu when a `v*` tag
is pushed. Linux builds need GTK 3 / appindicator dev packages (list in the workflow).

## Architecture

- **One egui app struct** `Sweep` in `src/main.rs` (`impl eframe::App`), with a `Page` enum for screens.
  Long work runs off the UI thread via `spawn(ctx, f) -> Receiver<T>` and is collected each frame with
  `poll(&mut job)`; follow that pattern for new scans.
- **Engine vs UI split**: `clean.rs` is the scanning/removal engine (clutter categories, browsers, large files,
  BLAKE3 duplicates); `photos.rs`, `apps.rs` (uninstall + startup items per OS), `browser_pro.rs` (edits browser
  SQLite DBs row by row: cookie allowlist, history-older-than), `schedule.rs`, `sensors.rs`, `update.rs` are
  other engines. `screens.rs` (scan/results/done) and `tabs.rs` (photos, apps, startup, monitor, stats,
  settings) are UI. `kit.rs` = theme, fonts, icons, widgets; `illus.rs` = code-drawn animated illustrations.
- `super_clean.rs` runs every scanner at once and splits results into "Safe to clean now" vs "Your call"
  (never pre-ticked); its "See all" reuses the existing screens with the same results.
- `tray.rs` + `panel.rs`: menu-bar/tray icon; the main window turns into the panel (`--page tray`) and keeps
  running when closed. `tour.rs`: first run (macOS Full Disk Access guide). `shell.rs`: Finder Quick Action /
  Explorer menu / GNOME script and notifications.
- Per-OS code is `#[cfg(target_os = ...)]` inline, not separate modules.

## Rules that must hold

- **Safety**: all removal goes through `clean::remove_path`, which refuses anything `protected()` (`/System/`,
  `/`, plus estimated Windows/Linux system dirs). Personal files go to the Trash (`trash` crate); only
  regenerable junk is deleted for good. Nothing is removed without a confirmation. Saved passwords/autofill are
  never ticked for you or cleaned on a schedule; running browsers are skipped.
- **Mechanics sheet**: cleaning rules and thresholds are described in `MECHANICS.md`; unchecked values are
  marked *estimated* (all Windows/Linux paths).
  `mechanics.json` lists the values; `src/mechanics.rs` measures what the build actually does, and `cargo test`
  fails if they disagree. When changing a rule or default, update `MECHANICS.md`, `mechanics.json` and the code
  together.
- **Privacy**: the only network call is the opt-in update check (`update.rs`, via the system `curl`, off by
  default). Don't add network dependencies.
- Size labels are decimal (1 KB = 1000 bytes).
