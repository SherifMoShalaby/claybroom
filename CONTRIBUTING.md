# Contributing to Claybroom

Thanks for helping. Bug reports, testing on Windows and Linux, and pull requests are all welcome. Everyone taking part is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Reporting a bug or asking for a feature

Use the [issue forms](https://github.com/SherifMoShalaby/claybroom/issues/new/choose): they ask for your operating system and Claybroom version, which most bugs need.

Before you attach a screenshot or a log, remove anything personal: file names, folder paths with your username, and browser data. Claybroom's scan results can show all of these.

For a **security problem** (for example, a way to make Claybroom remove something it should never touch), please don't open a public issue. Send it through [a private security report](https://github.com/SherifMoShalaby/claybroom/security/advisories/new) instead; only the maintainers can see it.

## Building

You need [Rust](https://rustup.rs) (stable). On Linux, also the GTK 3 and appindicator development packages; the exact list is in [`.github/workflows/release.yml`](.github/workflows/release.yml).

```sh
cargo run --release                      # build and start the app
cargo test                               # unit tests and the rules check
cargo test <name>                        # one test, e.g. cargo test system_folder_is_protected
cargo test -- --ignored --nocapture      # read-only scans of your real computer
```

Handy while working on the interface:

```sh
./target/release/claybroom --page clutter                          # open one screen directly
./target/release/claybroom --page clutter --scan --screenshot x.png # picture of the results
./target/release/claybroom --theme dark --page settings             # force light or dark
```

## Where things live

| Area | Files |
|---|---|
| Scanning and removing (clutter, browsers, large files, duplicates) | `src/clean.rs` |
| Photos, apps and startup items, scheduled cleaning, sensors, updates | `src/photos.rs`, `src/apps.rs`, `src/schedule.rs`, `src/sensors.rs`, `src/update.rs` |
| Screens | `src/screens.rs`, `src/tabs.rs`, `src/super_clean.rs`, `src/tour.rs`, `src/panel.rs` |
| Look and feel (colours, fonts, icons, widgets, illustrations) | `src/kit.rs`, `src/illus.rs` |
| The app icon | `assets/icon.svg`, rendered to `assets/icon-1024.png` |

Long-running work runs on a background thread through `spawn(...)` and is collected each frame with `poll(...)` in `src/main.rs`; new scans should follow that pattern. Code for one operating system sits inline behind `#[cfg(target_os = "...")]`.

## Rules every change must keep

- **Every removal goes through `clean::remove_path`**, which refuses protected system folders. Don't delete files any other way.
- **Personal files go to the Trash.** Only regenerable junk (caches, logs, browser data) is deleted permanently, and nothing is removed without a confirmation.
- **Nothing personal is ticked for you.** Saved passwords, autofill, cookies, downloads, duplicates and photos stay unticked by default.
- **No network access** beyond the opt-in update check. Please don't add network libraries or telemetry.
- **The rules sheet stays true.** `MECHANICS.md` describes what Claybroom cleans and its limits; `mechanics.json` lists the same values, and `cargo test` checks them against what the build actually does. If you change a folder, a limit or a default, update all three in the same pull request.

## Pull requests

1. For anything larger than a small fix, open an issue first so we can agree on the approach.
2. Keep each pull request to one change, and say which operating systems you tested it on.
3. Make sure `cargo test` passes and `cargo build --release` shows no warnings.
4. If you changed a screen, include a before and after screenshot (from a test folder, not your own files).

By contributing, you agree that your work is released under the project's [MIT licence](LICENSE).
