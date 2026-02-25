# LockPilot - Made by Maks (Mac)
![LockPilot Screenshot](assets/app-screenshot.png)

LockPilot lets you schedule Mac actions at exact times, including recurring schedules.

## Download and Install
Quick Start:
- [Download latest stable release](https://github.com/maxacode/LockPilotMac/releases/latest)

After download:
1. Open the DMG.
2. Drag `LockPilot.app` into `Applications`.
3. Launch `LockPilot`.

If macOS blocks launch with "damaged and can't be opened", run:

```bash
sudo xattr -dr com.apple.quarantine "/Applications/LockPilot.app"
```

WARNING: This bypasses a Gatekeeper safety check and allows the app to run without notarization.

## What You Can Do
- Create multiple timers
- Actions per timer:
  - Popup message
  - Lock screen
  - Shut down
  - Restart
- One-time and recurring schedules:
  - One-time
  - Daily
  - Weekdays
  - Every N hours (1-24)
- Cancel any active timer
- See active timers with next run time and live countdown
- Keep timers after restart (timers are saved and restored on app launch)
- Check for app updates in-app
- Install latest update from selected channel
- Roll back by installing a specific release version

## Recurring Timers (Simple Guide)
In the timer form:
1. Set action and trigger time.
2. Pick `Repeat`:
   - `One-time`
   - `Daily`
   - `Weekdays`
   - `Every N Hours`
3. If using `Every N Hours`, set interval hours.
4. Click `Add Timer`.

## Updates and Rollback
- Use `Check Now` in the Updates section.
- Enable `Check for updates on launch` if desired.
- To roll back, choose a release tag/version and install it.

## macOS Notes
- Lock action uses a fallback flow:
  - `Ctrl+Cmd+Q`
  - Start screen saver
  - `pmset displaysleepnow`
- Shutdown/Restart/Lock may require macOS permissions (`System Events`).

## Security Notes
- If your GitHub repo/account is compromised, release assets could be replaced by malicious files.
- Signed + notarized releases are strongly recommended for safer distribution.

## What's New
- `v0.4.1`
  - Updated app icon and release polish.
  - README aligned for end-user install/use.
- `v0.4.0`
  - Added recurring timers (daily, weekdays, every N hours).
  - Added persistent timer storage and auto-restore on launch.
  - Added glass-style UI refinements.
