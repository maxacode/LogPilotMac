# LockPilot - Made by Maks (Mac)

A macOS desktop app to schedule system actions for specific times, including recurring schedules, update channels, and rollback support.

## Install on Mac
Quick Start:
- Stable (main): [Download latest release](https://github.com/maxacode/LockPilotMac/releases/latest)
- Dev (dev): [Download latest dev prerelease](https://github.com/maxacode/LockPilotMac/releases/tag/dev-latest)

If you distribute an unsigned or non-notarized `.app`, macOS may block it with a "damaged and can't be opened" message.

You can remove the quarantine flag manually:

```bash
sudo xattr -dr com.apple.quarantine "/Applications/LockPilot.app"
```

WARNING: This bypasses a Gatekeeper safety check and allows the app to run without notarization.

WARNING: If your GitHub account/repository is compromised, an attacker could replace release assets with a malicious app.

Recommendation: always ship signed + notarized releases for production distribution.

## Features
- Multiple concurrent timers
- One-time timer execution
- Actions:
  - Popup message
  - Lock screen
  - Shut down
  - Restart/reboot
- Recurring schedules (Option 2):
  - Daily
  - Weekdays
  - Every N hours (1-24)
- Cancel any active timer
- Live timer list with next run time and countdown
- Timer persistence to local app data and automatic restore on launch
- In-app updater:
  - Check now
  - Auto-check on launch toggle
  - Update channels: `main` (stable) and `dev` (prerelease)
  - Install latest build from selected channel
- Rollback:
  - Select a specific release tag/version and install it
- macOS-first UI refinements:
  - Glass-inspired theme
  - Narrow, borderless card style
  - Subtle side accents
- Custom app icon themed to lock + timer behavior

## Updater and Release Source
LockPilot checks GitHub Releases from:

- `https://api.github.com/repos/maxacode/LockPilotMac/releases`

Channel mapping:

- `main` channel -> stable releases (`prerelease=false`)
- `dev` channel -> prereleases (`prerelease=true`)

## macOS behavior notes
- `Lock` uses a fallback chain:
  - `Ctrl+Cmd+Q` lock shortcut via `System Events`
  - current screen saver
  - `pmset displaysleepnow`
- `Shutdown` and `Reboot` use AppleScript (`System Events`) and may require macOS permissions.
- `Popup` uses AppleScript dialog.

## Timer Persistence
Timers are saved to app data (`timers.json`) when created/updated/canceled and restored automatically on app launch.

## Project Layout
- `src-tauri/`: Rust backend + Tauri app config
- `ui/`: static frontend (HTML/CSS/JS)

## Dev Run (no JS framework required)
1. Install Rust and cargo.
2. Install Tauri CLI:
   - `cargo install tauri-cli`
3. In one terminal, serve the `ui/` folder:
   - `python3 -m http.server 1420 --directory ui`
4. In another terminal, run:
   - `cd src-tauri`
   - `cargo tauri dev`

## Build
From `src-tauri/`:

- Build app: `cargo tauri build --bundles app`
- Build dmg: `cargo tauri build --bundles dmg`

Output bundles are under `src-tauri/target/release/bundle/`.

## Changelog
- `v0.4.0`:
  - Added recurring timers (daily, weekdays, every N hours).
  - Added persistent timer storage and auto-restore on app launch.
  - Updated app icon to lock + timer themed design.
  - Refined glass UI styling with narrower, borderless mac-style layout.
- `v0.3.0`:
  - Removed GitHub Actions workflow from the repository.
  - Bumped app version to `0.3.0`.
- `v0.2.2`:
  - Added update channel support: `main` and `dev`.
  - `main` channel checks stable GitHub releases.
  - `dev` channel checks prerelease GitHub releases.
  - Added install action for selected channel.
  - Kept explicit rollback to any stable tag.
- `v0.2.1`:
  - Updated app/window title to `LockPilot - Made by Maks`.
  - Bumped application version metadata.
- `v0.2.0`:
  - Added GitHub release update checks in-app.
  - Added auto-check on launch toggle.
  - Added install latest update flow.
  - Added rollback flow with manual version selection.
