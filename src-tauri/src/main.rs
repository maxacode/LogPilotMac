#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::mpsc,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

const GITHUB_OWNER: &str = "maxacode";
const GITHUB_REPO: &str = "LogPilotMac";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TimerAction {
    Popup,
    Lock,
    Shutdown,
    Reboot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimerInfo {
    id: String,
    action: TimerAction,
    target_time: DateTime<Utc>,
    message: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTimerRequest {
    action: TimerAction,
    target_time: String,
    message: Option<String>,
}

struct TimerEntry {
    info: TimerInfo,
    cancel_tx: mpsc::Sender<()>,
}

#[derive(Clone, Default)]
struct TimerStore {
    inner: Arc<Mutex<HashMap<String, TimerEntry>>>,
}

#[derive(Debug, Deserialize, Clone)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize, Clone)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseVersion {
    tag: String,
    name: String,
    published_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    tag: String,
    name: String,
    notes: Option<String>,
    published_at: Option<String>,
}

#[tauri::command]
fn list_timers(state: State<'_, TimerStore>) -> Result<Vec<TimerInfo>, String> {
    let store = state
        .inner
        .lock()
        .map_err(|_| "Failed to lock timer store".to_string())?;

    let mut timers: Vec<TimerInfo> = store.values().map(|entry| entry.info.clone()).collect();
    timers.sort_by_key(|timer| timer.target_time);

    Ok(timers)
}

#[tauri::command]
fn cancel_timer(id: String, state: State<'_, TimerStore>) -> Result<bool, String> {
    let mut store = state
        .inner
        .lock()
        .map_err(|_| "Failed to lock timer store".to_string())?;

    if let Some(entry) = store.remove(&id) {
        let _ = entry.cancel_tx.send(());
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
fn create_timer(request: CreateTimerRequest, state: State<'_, TimerStore>) -> Result<TimerInfo, String> {
    let target = DateTime::parse_from_rfc3339(&request.target_time)
        .map_err(|_| "Invalid date/time format".to_string())?
        .with_timezone(&Utc);

    let now = Utc::now();
    if target <= now {
        return Err("Selected time must be in the future".to_string());
    }

    if matches!(request.action, TimerAction::Popup)
        && request
            .message
            .as_ref()
            .map(|msg| msg.trim().is_empty())
            .unwrap_or(true)
    {
        return Err("Popup timers require a message".to_string());
    }

    let id = Uuid::new_v4().to_string();
    let info = TimerInfo {
        id: id.clone(),
        action: request.action,
        target_time: target,
        message: request.message.map(|msg| msg.trim().to_string()),
        created_at: now,
    };

    let (cancel_tx, cancel_rx) = mpsc::channel();

    {
        let mut store = state
            .inner
            .lock()
            .map_err(|_| "Failed to lock timer store".to_string())?;

        store.insert(
            id.clone(),
            TimerEntry {
                info: info.clone(),
                cancel_tx,
            },
        );
    }

    let store = state.inner.clone();
    let task_info = info.clone();
    thread::spawn(move || {
        let wait = match (target - Utc::now()).to_std() {
            Ok(duration) => duration,
            Err(_) => Duration::from_secs(0),
        };

        if cancel_rx.recv_timeout(wait).is_err() {
            run_action(&task_info.action, task_info.message.as_deref());
            if let Ok(mut locked) = store.lock() {
                locked.remove(&id);
            }
        }
    });

    Ok(info)
}

#[tauri::command]
fn list_release_versions() -> Result<Vec<ReleaseVersion>, String> {
    let mut releases = stable_releases(fetch_releases()?);
    releases.sort_by(release_version_desc);

    Ok(releases
        .into_iter()
        .map(|release| ReleaseVersion {
            tag: release.tag_name.clone(),
            name: release.name.unwrap_or_else(|| release.tag_name.clone()),
            published_at: release.published_at,
        })
        .collect())
}

#[tauri::command]
fn check_for_updates(current_version: String) -> Result<Option<UpdateInfo>, String> {
    let current = normalize_version(&current_version)
        .ok_or_else(|| format!("Invalid current version: {current_version}"))?;

    let mut releases = stable_releases(fetch_releases()?);
    releases.sort_by(release_version_desc);

    let update = releases.into_iter().find(|release| {
        normalize_version(&release.tag_name)
            .map(|version| version > current)
            .unwrap_or(false)
    });

    Ok(update.map(|release| UpdateInfo {
        tag: release.tag_name.clone(),
        name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        notes: release.body,
        published_at: release.published_at,
    }))
}

#[tauri::command]
fn install_release(tag: String) -> Result<String, String> {
    let releases = stable_releases(fetch_releases()?);
    let release = releases
        .into_iter()
        .find(|release| tags_match(&release.tag_name, &tag))
        .ok_or_else(|| format!("Release not found for tag: {tag}"))?;

    let dmg_asset = pick_dmg_asset(&release.assets)
        .ok_or_else(|| format!("No DMG asset found for release {}", release.tag_name))?;

    let local_dmg = download_asset_to_temp(&dmg_asset.browser_download_url, &release.tag_name)?;
    Command::new("/usr/bin/open")
        .arg(&local_dmg)
        .spawn()
        .map_err(|err| format!("Failed to open installer DMG: {err}"))?;

    Ok(format!(
        "Opened installer for {} from {}",
        release.tag_name,
        local_dmg.display()
    ))
}

fn run_action(action: &TimerAction, message: Option<&str>) {
    match action {
        TimerAction::Popup => {
            if let Some(msg) = message {
                let escaped = msg.replace('"', "\\\"");
                let script = format!(
                    "display dialog \"{}\" with title \"LockPilot\" buttons {{\"OK\"}} default button \"OK\"",
                    escaped
                );
                let _ = run_osascript(&script);
            }
        }
        TimerAction::Lock => {
            // Modern macOS fallback chain for locking:
            // 1) trigger Ctrl+Cmd+Q lock shortcut
            // 2) start screen saver
            // 3) force display sleep
            let locked = run_osascript(
                "tell application \"System Events\" to keystroke \"q\" using {control down, command down}",
            )
            .is_ok()
                || run_osascript("tell application \"System Events\" to start current screen saver")
                    .is_ok();

            if !locked {
                let _ = Command::new("/usr/bin/pmset").arg("displaysleepnow").spawn();
            }
        }
        TimerAction::Shutdown => {
            let _ = run_osascript("tell application \"System Events\" to shut down");
        }
        TimerAction::Reboot => {
            let _ = run_osascript("tell application \"System Events\" to restart");
        }
    }
}

fn run_osascript(script: &str) -> Result<(), String> {
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|err| format!("Failed to run osascript: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn fetch_releases() -> Result<Vec<GithubRelease>, String> {
    let client = Client::builder()
        .user_agent("LockPilot-Updater")
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {err}"))?;

    let url = format!(
        "https://api.github.com/repos/{}/{}/releases?per_page=100",
        GITHUB_OWNER, GITHUB_REPO
    );

    let response = client
        .get(url)
        .send()
        .map_err(|err| format!("Failed to fetch GitHub releases: {err}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "GitHub releases API returned status {}",
            response.status()
        ));
    }

    response
        .json::<Vec<GithubRelease>>()
        .map_err(|err| format!("Failed to parse GitHub releases: {err}"))
}

fn normalize_version(version: &str) -> Option<Version> {
    Version::parse(version.trim().trim_start_matches('v')).ok()
}

fn release_version_desc(a: &GithubRelease, b: &GithubRelease) -> std::cmp::Ordering {
    let av = normalize_version(&a.tag_name);
    let bv = normalize_version(&b.tag_name);
    bv.cmp(&av)
}

fn stable_releases(releases: Vec<GithubRelease>) -> Vec<GithubRelease> {
    releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter(|release| normalize_version(&release.tag_name).is_some())
        .collect()
}

fn tags_match(a: &str, b: &str) -> bool {
    a.trim() == b.trim() || a.trim_start_matches('v') == b.trim_start_matches('v')
}

fn pick_dmg_asset(assets: &[GithubAsset]) -> Option<GithubAsset> {
    let arch = std::env::consts::ARCH;
    let dmg_assets: Vec<GithubAsset> = assets
        .iter()
        .filter(|asset| asset.name.to_lowercase().ends_with(".dmg"))
        .cloned()
        .collect();

    let arch_match = match arch {
        "aarch64" => dmg_assets
            .iter()
            .find(|asset| asset.name.contains("aarch64") || asset.name.contains("arm64"))
            .cloned(),
        "x86_64" => dmg_assets
            .iter()
            .find(|asset| asset.name.contains("x86_64") || asset.name.contains("amd64"))
            .cloned(),
        _ => None,
    };

    arch_match.or_else(|| dmg_assets.into_iter().next())
}

fn download_asset_to_temp(url: &str, tag: &str) -> Result<PathBuf, String> {
    let client = Client::builder()
        .user_agent("LockPilot-Updater")
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {err}"))?;
    let response = client
        .get(url)
        .send()
        .map_err(|err| format!("Failed to download release asset: {err}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Release asset download failed with status {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|err| format!("Failed to read release asset body: {err}"))?;
    let safe_tag = tag.replace('/', "-");
    let path = std::env::temp_dir().join(format!("LockPilot-{safe_tag}.dmg"));
    fs::write(&path, bytes).map_err(|err| format!("Failed to write installer DMG: {err}"))?;
    Ok(path)
}

fn main() {
    tauri::Builder::default()
        .manage(TimerStore::default())
        .invoke_handler(tauri::generate_handler![
            create_timer,
            list_timers,
            cancel_timer,
            list_release_versions,
            check_for_updates,
            install_release
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
