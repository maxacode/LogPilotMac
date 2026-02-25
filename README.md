# LockPilot (Mac)

A macOS desktop app to schedule multiple one-time timers with actions:
- popup message
- lock screen
- shut down
- restart

You can add any number of timers, see active timers with due times/countdown, and cancel any timer.

## Features
- Multiple one-time timers
- Actions: popup, lock, shutdown, reboot
- Cancel active timers
- GitHub-based update checks
- Auto-check for updates on launch (toggle in app)
- Install latest version from GitHub release
- Rollback by selecting a specific release tag/version

## Changelog
- `v0.2.0`:
  - Added GitHub release update checks in-app.
  - Added auto-check on launch toggle.
  - Added install latest update flow.
  - Added rollback flow with manual version selection.
  - Added CI workflow for PR checks and signed/notarized release pipeline.

## Install on Mac
If you distribute an unsigned or non-notarized `.app`, macOS may block it with a "damaged and can't be opened" message.

You can remove the quarantine flag manually:

```bash
sudo xattr -dr com.apple.quarantine "/Applications/LockPilot.app"
```

WARNING: This bypasses a Gatekeeper safety check and allows the app to run without notarization.

WARNING: If your GitHub account/repository is compromised, an attacker could replace release assets with a malicious app.

## Updater and Rollback
- LockPilot checks GitHub Releases from:
  - `https://api.github.com/repos/maxacode/LogPilotMac/releases`
- Update behavior:
  - You can manually click `Check Now`.
  - You can enable `Check for updates on launch`.
  - Installing an update downloads the matching DMG and opens it.
- Rollback behavior:
  - Select any release tag from the rollback dropdown and install it.
  - This lets you return to an older version if a newer one has issues.

Recommendation: always ship signed + notarized releases. This is safer than removing quarantine and avoids Gatekeeper warnings on other Macs.

## Project Layout
- `src-tauri/`: Rust backend + Tauri app config
- `ui/`: static frontend (HTML/CSS/JS)

## macOS behavior notes
- `Lock` uses a fallback chain: lock shortcut (`Ctrl+Cmd+Q`), then screen saver, then `pmset displaysleepnow`.
- `Shutdown` and `Reboot` use AppleScript (`System Events`) and may require macOS permissions.
- `Popup` uses AppleScript dialog.

## Dev Run (no JS framework required)
1. Install Rust and cargo.
2. Install Tauri CLI:
   - `cargo install tauri-cli`
3. In one terminal, serve the `ui/` folder:
   - `python3 -m http.server 1420 --directory ui`
4. In another terminal, run:
   - `cd src-tauri`
   - `cargo tauri dev`

## Build .app
From `src-tauri/`:
- `cargo tauri build`

Output app bundle will be under `src-tauri/target/release/bundle/`.

