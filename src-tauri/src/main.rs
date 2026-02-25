#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use chrono::{DateTime, Datelike, Duration as ChronoDuration, TimeZone, Utc, Weekday};
use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use uuid::Uuid;

const GITHUB_OWNER: &str = "maxacode";
const GITHUB_REPO: &str = "LockPilotMac";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TimerAction {
    Popup,
    Lock,
    Shutdown,
    Reboot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum UpdateChannel {
    Main,
    Dev,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecurrencePreset {
    Daily,
    Weekdays,
    EveryNHours,
    EveryNMinutes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecurrenceConfig {
    preset: RecurrencePreset,
    interval_hours: Option<u32>,
    interval_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimerInfo {
    id: String,
    action: TimerAction,
    target_time: DateTime<Utc>,
    recurrence: Option<RecurrenceConfig>,
    message: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTimerRequest {
    action: TimerAction,
    target_time: String,
    recurrence: Option<RecurrenceConfig>,
    message: Option<String>,
}

struct TimerEntry {
    info: TimerInfo,
    cancel_tx: mpsc::Sender<()>,
}

#[derive(Clone)]
struct TimerStore {
    inner: Arc<Mutex<HashMap<String, TimerEntry>>>,
    storage_path: Arc<PathBuf>,
}

impl TimerStore {
    fn new(storage_path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            storage_path: Arc::new(storage_path),
        }
    }

    fn persist(&self) -> Result<(), String> {
        let locked = self
            .inner
            .lock()
            .map_err(|_| "Failed to lock timer store".to_string())?;

        let mut timers: Vec<TimerInfo> = locked.values().map(|entry| entry.info.clone()).collect();
        timers.sort_by_key(|timer| timer.target_time);
        drop(locked);

        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Failed to create timer storage directory: {err}"))?;
        }

        let data = serde_json::to_string_pretty(&PersistedTimers { timers })
            .map_err(|err| format!("Failed to encode timer data: {err}"))?;
        fs::write(self.storage_path.as_ref(), data)
            .map_err(|err| format!("Failed to write timer data: {err}"))?;
        Ok(())
    }

    fn load_persisted_infos(&self) -> Result<Vec<TimerInfo>, String> {
        if !self.storage_path.exists() {
            return Ok(Vec::new());
        }

        let raw = fs::read_to_string(self.storage_path.as_ref())
            .map_err(|err| format!("Failed to read timer data: {err}"))?;
        let persisted = serde_json::from_str::<PersistedTimers>(&raw)
            .map_err(|err| format!("Failed to parse timer data: {err}"))?;
        Ok(persisted.timers)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedTimers {
    timers: Vec<TimerInfo>,
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
        drop(store);
        state.persist()?;
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

    validate_recurrence(request.recurrence.as_ref())?;

    let id = Uuid::new_v4().to_string();
    let recurrence = request.recurrence.clone();
    let info = TimerInfo {
        id: id.clone(),
        action: request.action,
        target_time: target,
        recurrence: recurrence.clone(),
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

    state.persist()?;
    schedule_timer_thread(
        state.inner.clone(),
        state.storage_path.as_ref(),
        id.clone(),
        target,
        info.clone(),
        recurrence,
        cancel_rx,
    );

    Ok(info)
}

fn schedule_timer_thread(
    store: Arc<Mutex<HashMap<String, TimerEntry>>>,
    storage_path: &Path,
    id: String,
    initial_target: DateTime<Utc>,
    task_info: TimerInfo,
    recurrence: Option<RecurrenceConfig>,
    cancel_rx: mpsc::Receiver<()>,
) {
    let storage_path = storage_path.to_path_buf();
    thread::spawn(move || {
        let mut next_run = initial_target;
        loop {
            let wait = match (next_run - Utc::now()).to_std() {
                Ok(duration) => duration,
                Err(_) => Duration::from_secs(0),
            };

            if cancel_rx.recv_timeout(wait).is_ok() {
                break;
            }

            run_action(&task_info.action, task_info.message.as_deref());

            let Some(recurrence_cfg) = recurrence.as_ref() else {
                if let Ok(mut locked) = store.lock() {
                    locked.remove(&id);
                }
                let _ = persist_inner_store(&store, &storage_path);
                break;
            };

            let computed_next = compute_next_run(next_run, recurrence_cfg);
            let Some(updated_next) = computed_next else {
                if let Ok(mut locked) = store.lock() {
                    locked.remove(&id);
                }
                let _ = persist_inner_store(&store, &storage_path);
                break;
            };
            next_run = updated_next;

            if let Ok(mut locked) = store.lock() {
                if let Some(entry) = locked.get_mut(&id) {
                    entry.info.target_time = next_run;
                } else {
                    break;
                }
            }
            let _ = persist_inner_store(&store, &storage_path);
        }
    });
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
fn check_channel_update(
    current_version: String,
    channel: UpdateChannel,
) -> Result<Option<UpdateInfo>, String> {
    let current = normalize_version(&current_version)
        .ok_or_else(|| format!("Invalid current version: {current_version}"))?;

    let mut releases = releases_for_channel(fetch_releases()?, &channel);
    releases.sort_by(release_version_desc);

    let update = match channel {
        // Dev channel should always point users to the latest prerelease train,
        // even when local build metadata/version style differs.
        UpdateChannel::Dev => releases.into_iter().find(|release| {
            normalize_version(&release.tag_name)
                .map(|version| version != current)
                .unwrap_or(true)
        }),
        UpdateChannel::Main => releases.into_iter().find(|release| {
            normalize_version(&release.tag_name)
                .map(|version| version > current)
                .unwrap_or(false)
        }),
    };

    Ok(update.map(|release| UpdateInfo {
        tag: release.tag_name.clone(),
        name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        notes: release.body,
        published_at: release.published_at,
    }))
}

#[tauri::command]
fn install_channel_update(channel: UpdateChannel) -> Result<String, String> {
    let mut releases = releases_for_channel(fetch_releases()?, &channel);
    releases.sort_by(release_version_desc);
    let release = releases
        .into_iter()
        .next()
        .ok_or_else(|| format!("No releases found for {} channel", channel_name(&channel)))?;

    let dmg_asset = pick_dmg_asset(&release.assets)
        .ok_or_else(|| format!("No DMG asset found for release {}", release.tag_name))?;

    let local_dmg = download_asset_to_temp(&dmg_asset.browser_download_url, &release.tag_name)?;
    Command::new("/usr/bin/open")
        .arg(&local_dmg)
        .spawn()
        .map_err(|err| format!("Failed to open installer DMG: {err}"))?;

    Ok(format!(
        "Opened {} channel installer {} from {}",
        channel_name(&channel),
        release.tag_name,
        local_dmg.display()
    ))
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

fn validate_recurrence(recurrence: Option<&RecurrenceConfig>) -> Result<(), String> {
    let Some(recurrence) = recurrence else {
        return Ok(());
    };

    match recurrence.preset {
        RecurrencePreset::Daily | RecurrencePreset::Weekdays => Ok(()),
        RecurrencePreset::EveryNHours => {
            let Some(hours) = recurrence.interval_hours else {
                return Err("Every N Hours requires an interval.".to_string());
            };
            if (1..=24).contains(&hours) {
                Ok(())
            } else {
                Err("Interval hours must be between 1 and 24.".to_string())
            }
        }
        RecurrencePreset::EveryNMinutes => {
            let Some(minutes) = recurrence.interval_minutes else {
                return Err("Every N Minutes requires an interval.".to_string());
            };
            if (1..=1440).contains(&minutes) {
                Ok(())
            } else {
                Err("Interval minutes must be between 1 and 1440.".to_string())
            }
        }
    }
}

fn compute_next_run(current_target: DateTime<Utc>, recurrence: &RecurrenceConfig) -> Option<DateTime<Utc>> {
    match recurrence.preset {
        RecurrencePreset::Daily => {
            let mut next = current_target + ChronoDuration::days(1);
            while next <= Utc::now() {
                next += ChronoDuration::days(1);
            }
            Some(next)
        }
        RecurrencePreset::EveryNHours => {
            let interval = recurrence.interval_hours?;
            let mut next = current_target + ChronoDuration::hours(interval as i64);
            while next <= Utc::now() {
                next += ChronoDuration::hours(interval as i64);
            }
            Some(next)
        }
        RecurrencePreset::EveryNMinutes => {
            let interval = recurrence.interval_minutes?;
            let mut next = current_target + ChronoDuration::minutes(interval as i64);
            while next <= Utc::now() {
                next += ChronoDuration::minutes(interval as i64);
            }
            Some(next)
        }
        RecurrencePreset::Weekdays => {
            let time = current_target.time();
            let mut date = current_target.date_naive() + ChronoDuration::days(1);

            for _ in 0..14 {
                let weekday = date.weekday();
                if weekday != Weekday::Sat && weekday != Weekday::Sun {
                    let candidate = Utc.from_utc_datetime(&date.and_time(time));
                    if candidate > Utc::now() {
                        return Some(candidate);
                    }
                }
                date += ChronoDuration::days(1);
            }
            None
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

fn persist_inner_store(store: &Arc<Mutex<HashMap<String, TimerEntry>>>, storage_path: &Path) -> Result<(), String> {
    let locked = store
        .lock()
        .map_err(|_| "Failed to lock timer store".to_string())?;
    let mut timers: Vec<TimerInfo> = locked.values().map(|entry| entry.info.clone()).collect();
    timers.sort_by_key(|timer| timer.target_time);
    drop(locked);

    if let Some(parent) = storage_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create timer storage directory: {err}"))?;
    }

    let data = serde_json::to_string_pretty(&PersistedTimers { timers })
        .map_err(|err| format!("Failed to encode timer data: {err}"))?;
    fs::write(storage_path, data).map_err(|err| format!("Failed to write timer data: {err}"))?;
    Ok(())
}

fn restore_timers(store: &TimerStore) -> Result<(), String> {
    let restored = store.load_persisted_infos()?;
    if restored.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    for mut info in restored {
        if info.target_time <= now {
            if let Some(recurrence) = info.recurrence.as_ref() {
                let mut next = info.target_time;
                while next <= now {
                    let Some(updated) = compute_next_run(next, recurrence) else {
                        next = now;
                        break;
                    };
                    next = updated;
                }
                if next <= now {
                    continue;
                }
                info.target_time = next;
            } else {
                continue;
            }
        }

        let (cancel_tx, cancel_rx) = mpsc::channel();
        {
            let mut locked = store
                .inner
                .lock()
                .map_err(|_| "Failed to lock timer store".to_string())?;
            locked.insert(
                info.id.clone(),
                TimerEntry {
                    info: info.clone(),
                    cancel_tx,
                },
            );
        }

        schedule_timer_thread(
            store.inner.clone(),
            store.storage_path.as_ref(),
            info.id.clone(),
            info.target_time,
            info.clone(),
            info.recurrence.clone(),
            cancel_rx,
        );
    }

    store.persist()?;
    Ok(())
}

fn timer_storage_path(app: &tauri::AppHandle) -> PathBuf {
    let base = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("lockpilot"));
    base.join("timers.json")
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

fn releases_for_channel(releases: Vec<GithubRelease>, channel: &UpdateChannel) -> Vec<GithubRelease> {
    let base = releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter(|release| normalize_version(&release.tag_name).is_some());

    match channel {
        UpdateChannel::Main => base.filter(|release| !release.prerelease).collect(),
        UpdateChannel::Dev => base.filter(|release| release.prerelease).collect(),
    }
}

fn channel_name(channel: &UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Main => "main",
        UpdateChannel::Dev => "dev",
    }
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
        .setup(|app| {
            let store = TimerStore::new(timer_storage_path(app.handle()));
            if let Err(err) = restore_timers(&store) {
                eprintln!("Failed to restore timers: {err}");
            }
            app.manage(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_timer,
            list_timers,
            cancel_timer,
            list_release_versions,
            check_channel_update,
            install_channel_update,
            install_release
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
