# Super Clean: design options

Built: **Option B** (two lanes), named Super Clean. A and C are kept here for reference.

A Home-page action: one scan that runs Quick Clean, Clean clutter,
Clean browser, Find duplicates and Analyze photos, and also recommends unused apps to uninstall and
startup items to turn off, then shows everything in one organised place.

## What every option shares

* **One button on Home**: a wide card above the others, "Super Clean · Everything in one scan", with
  a SCAN button.
* **One scan, one progress screen**: the progress ring plus a list of the six steps ticking off
  (Junk, Browsers, Duplicates, Photos, Apps, Startup), each showing what it found so far. Steps
  run one after another; Cancel stops cleanly and keeps what was found.
* **Safety rule**: only regenerable junk starts ticked (caches, logs, Trash, browser caches).
  Personal files (duplicates, photos), apps and startup items always start unticked: they're
  recommendations, never cleaned without you choosing them.
* **No double counting**: Quick Clean and Clean clutter overlap (both hold caches, logs, Trash),
  so the shared items are listed once.
* **One finish screen**: "You freed up 14.2 GB", what was done per area, and anything skipped
  (an open browser, a file in use).

## Option A: Dashboard of tiles

```
 Super Clean found 68.4 GB you can free
 ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
 │ Junk         │ │ Browsers     │ │ Duplicates   │
 │ 12.3 GB  ✓   │ │ 1.8 GB  ✓    │ │ 4.1 GB · 37  │
 │ REVIEW       │ │ REVIEW       │ │ REVIEW       │
 └──────────────┘ └──────────────┘ └──────────────┘
 ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
 │ Photos       │ │ Unused apps  │ │ Startup      │
 │ 2.0 GB · 54  │ │ 3 apps 9 GB  │ │ 5 can stop   │
 │ REVIEW       │ │ REVIEW       │ │ REVIEW       │
 └──────────────┘ └──────────────┘ └──────────────┘
                      [ CLEAN 14.1 GB ]
```
Each tile opens the screen that already exists for it (the clutter list, the duplicate groups,
the photo grid, the app list filtered to unused, the startup list), already filled in.
* Good: tidy and calm; reuses screens people already know; easy to build well.
* Less good: reviewing personal items means opening tiles one by one.

## Option B: Two lanes, "Safe now" and "Your call" (recommended)

```
 Super Clean found 68.4 GB
 ┌─ Safe to clean now ─────────┐ ┌─ Your call ──────────────────────┐
 │ ☑ App caches        8.1 GB  │ │ Duplicates · 37 files · 4.1 GB ▸ │
 │ ☑ Logs              0.9 GB  │ │   ☐ IMG_2201 (copy).jpg   12 MB  │
 │ ☑ Trash             3.3 GB  │ │   ☐ report-final(1).pdf    3 MB  │
 │ ☑ Browser caches    1.8 GB  │ │ Bad photos · 54 · 2.0 GB       ▸ │
 │ ☐ Cookies           (keeps  │ │ Unused apps · 3 · 9 GB         ▸ │
 │   you signed in if off)     │ │   ☐ GarageBand  last used 2024   │
 │                             │ │ Startup · 5 you could turn off ▸ │
 └─────────────────────────────┘ └──────────────────────────────────┘
          14.1 GB selected · [ CLEAN SELECTED ]
```
Left: everything that is safe, ticked, one glance. Right: each recommendation as a folding
section showing its top three, ticked by you, with ▸ opening the full existing screen.
* Good: sorted by risk, which is the decision people actually make; one click for the safe
  part; recommendations are visible without extra screens.
* Less good: busier screen; the right column needs care to stay readable on small windows.

## Option C: Guided steps

```
 Step 3 of 6 · Duplicates
 37 copies take 4.1 GB. We kept the newest of each.
 [ list of groups ]
            [ SKIP ]   [ CLEAN THESE ]   → Step 4: Photos
```
A short walk through each area with plain advice, then a summary.
* Good: least overwhelming for someone new; each step has room to explain.
* Less good: slowest; six screens to finish; easy to stop halfway.

## Name ideas

Super Clean · Deep Clean · Clean Everything · Full Checkup

## Recommendation

Option B, named "Super Clean": it sorts by risk, keeps the safe part one click, and still puts
the personal-file and app suggestions on the same screen. Option A's tiles could be its narrow-window
layout.
