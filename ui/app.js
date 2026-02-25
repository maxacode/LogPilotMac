const { invoke } = window.__TAURI__.core;
const { getVersion } = window.__TAURI__.app;

const form = document.getElementById("timer-form");
const actionInput = document.getElementById("action");
const targetTimeInput = document.getElementById("target-time");
const recurrencePresetInput = document.getElementById("recurrence-preset");
const intervalWrap = document.getElementById("interval-wrap");
const intervalHoursInput = document.getElementById("interval-hours");
const messageWrap = document.getElementById("message-wrap");
const messageInput = document.getElementById("message");
const timersEl = document.getElementById("timers");
const statusEl = document.getElementById("status");
const refreshBtn = document.getElementById("refresh");

const checkUpdatesBtn = document.getElementById("check-updates");
const autoCheckUpdatesInput = document.getElementById("auto-check-updates");
const updateChannelSelect = document.getElementById("update-channel");
const currentVersionEl = document.getElementById("current-version");
const updateStatusEl = document.getElementById("update-status");
const updateResultEl = document.getElementById("update-result");
const latestVersionEl = document.getElementById("latest-version");
const latestNotesEl = document.getElementById("latest-notes");
const installLatestBtn = document.getElementById("install-latest");
const rollbackVersionSelect = document.getElementById("rollback-version");
const rollbackInstallBtn = document.getElementById("rollback-install");

const AUTO_UPDATE_KEY = "lockpilot.autoCheckUpdates";
const UPDATE_CHANNEL_KEY = "lockpilot.updateChannel";
const LAUNCH_TIME = new Date();
let currentVersion = "";
let latestUpdate = null;

const showStatus = (text, isError = false) => {
  statusEl.textContent = text;
  statusEl.style.color = isError ? "#c30e2e" : "#4f7480";
};

const showUpdateStatus = (text, isError = false) => {
  updateStatusEl.textContent = text;
  updateStatusEl.style.color = isError ? "#c30e2e" : "#4f7480";
};

const selectedChannel = () => updateChannelSelect.value;

const toLocalDateTimeValue = (date) => {
  const pad = (n) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
};

const toggleMessage = () => {
  const isPopup = actionInput.value === "popup";
  messageWrap.style.display = isPopup ? "grid" : "none";
  messageInput.required = isPopup;
};

const toggleRecurrence = () => {
  const recurring = recurrencePresetInput.value !== "none";
  const needsInterval =
    recurrencePresetInput.value === "every_n_hours" ||
    recurrencePresetInput.value === "every_n_minutes";
  intervalWrap.classList.toggle("hidden", !needsInterval);
  intervalHoursInput.required = needsInterval;
  intervalHoursInput.max = recurrencePresetInput.value === "every_n_minutes" ? "1440" : "24";

  if (!recurring) {
    intervalWrap.classList.add("hidden");
  }
};

const fmtDate = (iso) => new Date(iso).toLocaleString();

const fmtRemaining = (iso) => {
  const ms = new Date(iso).getTime() - Date.now();
  if (ms <= 0) {
    return "due now";
  }

  const total = Math.floor(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return `${hours}h ${minutes}m ${seconds}s`;
};

const recurrenceLabel = (recurrence) => {
  if (!recurrence) {
    return "One-time";
  }

  if (recurrence.preset === "daily") {
    return "Repeats daily";
  }

  if (recurrence.preset === "weekdays") {
    return "Repeats weekdays";
  }

  if (recurrence.preset === "every_n_hours") {
    return `Repeats every ${recurrence.intervalHours ?? "?"} hour(s)`;
  }

  if (recurrence.preset === "every_n_minutes") {
    return `Repeats every ${recurrence.intervalMinutes ?? "?"} minute(s)`;
  }

  return "Recurring";
};

const renderTimers = (timers) => {
  timersEl.innerHTML = "";

  if (!timers.length) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = "No active timers.";
    timersEl.appendChild(empty);
    return;
  }

  for (const timer of timers) {
    const item = document.createElement("li");
    item.className = "timer-item";

    const top = document.createElement("div");
    top.className = "timer-top";

    const title = document.createElement("strong");
    title.textContent = timer.action.toUpperCase();

    const cancelBtn = document.createElement("button");
    cancelBtn.className = "danger";
    cancelBtn.textContent = "Cancel";
    cancelBtn.addEventListener("click", async () => {
      try {
        await invoke("cancel_timer", { id: timer.id });
        await loadTimers();
        showStatus("Timer canceled.");
      } catch (err) {
        showStatus(String(err), true);
      }
    });

    top.append(title, cancelBtn);

    const when = document.createElement("div");
    when.className = "timer-meta";
    when.textContent = `Runs at ${fmtDate(timer.targetTime)} (${fmtRemaining(timer.targetTime)})`;

    const recurrence = document.createElement("div");
    recurrence.className = "timer-meta";
    recurrence.textContent = recurrenceLabel(timer.recurrence);

    item.append(top, when, recurrence);

    if (timer.action === "popup" && timer.message) {
      const msg = document.createElement("div");
      msg.className = "timer-meta";
      msg.textContent = `Message: ${timer.message}`;
      item.append(msg);
    }

    timersEl.append(item);
  }
};

const loadTimers = async () => {
  try {
    const timers = await invoke("list_timers");
    renderTimers(timers);
  } catch (err) {
    showStatus(String(err), true);
  }
};

const renderUpdateResult = (update) => {
  if (!update) {
    updateResultEl.classList.add("hidden");
    latestUpdate = null;
    return;
  }

  latestUpdate = update;
  updateResultEl.classList.remove("hidden");
  latestVersionEl.textContent = update.tag;
  latestNotesEl.textContent = update.notes?.trim()
    ? update.notes.split("\n")[0]
    : "No release notes provided.";
};

const loadRollbackVersions = async () => {
  rollbackVersionSelect.innerHTML = "";

  try {
    const versions = await invoke("list_release_versions");
    versions.forEach((version) => {
      const option = document.createElement("option");
      option.value = version.tag;
      option.textContent = `${version.tag}${version.publishedAt ? ` (${new Date(version.publishedAt).toLocaleDateString()})` : ""}`;
      rollbackVersionSelect.appendChild(option);
    });
  } catch (err) {
    showUpdateStatus(`Could not load release versions: ${String(err)}`, true);
  }
};

const checkForUpdates = async (silentWhenUpToDate = false) => {
  if (!currentVersion) {
    return;
  }

  const channel = selectedChannel();

  try {
    showUpdateStatus(`Checking ${channel} channel on GitHub releases...`);
    const update = await invoke("check_channel_update", {
      currentVersion,
      channel,
    });
    renderUpdateResult(update);

    if (update) {
      showUpdateStatus(`Update available in ${channel}: ${update.tag}`);
    } else if (!silentWhenUpToDate) {
      showUpdateStatus(`No newer version found in ${channel}.`);
    } else {
      showUpdateStatus("");
    }
  } catch (err) {
    showUpdateStatus(`Update check failed: ${String(err)}`, true);
  }
};

const installChannelUpdate = async () => {
  const channel = selectedChannel();

  try {
    const result = await invoke("install_channel_update", { channel });
    showUpdateStatus(`${result}. Complete install from the opened DMG.`);
  } catch (err) {
    showUpdateStatus(`Install failed: ${String(err)}`, true);
  }
};

const installTag = async (tag) => {
  try {
    const result = await invoke("install_release", { tag });
    showUpdateStatus(`${result}. Complete install from the opened DMG.`);
  } catch (err) {
    showUpdateStatus(`Install failed: ${String(err)}`, true);
  }
};

form.addEventListener("submit", async (event) => {
  event.preventDefault();

  if (!targetTimeInput.value) {
    showStatus("Choose a valid time.", true);
    return;
  }

  const recurrencePreset = recurrencePresetInput.value;
  let recurrence = null;
  if (recurrencePreset !== "none") {
    recurrence = {
      preset: recurrencePreset,
      intervalHours: recurrencePreset === "every_n_hours" ? Number(intervalHoursInput.value || 0) : null,
      intervalMinutes: recurrencePreset === "every_n_minutes" ? Number(intervalHoursInput.value || 0) : null,
    };
  }

  const request = {
    action: actionInput.value,
    targetTime: new Date(targetTimeInput.value).toISOString(),
    recurrence,
    message: actionInput.value === "popup" ? messageInput.value : null,
  };

  try {
    await invoke("create_timer", { request });
    form.reset();
    targetTimeInput.value = toLocalDateTimeValue(LAUNCH_TIME);
    recurrencePresetInput.value = "none";
    intervalHoursInput.value = "2";
    toggleMessage();
    toggleRecurrence();
    showStatus("Timer created.");
    await loadTimers();
  } catch (err) {
    showStatus(String(err), true);
  }
});

refreshBtn.addEventListener("click", loadTimers);
actionInput.addEventListener("change", toggleMessage);
recurrencePresetInput.addEventListener("change", toggleRecurrence);

checkUpdatesBtn.addEventListener("click", () => checkForUpdates(false));
installLatestBtn.addEventListener("click", installChannelUpdate);

rollbackInstallBtn.addEventListener("click", async () => {
  const selectedTag = rollbackVersionSelect.value;
  if (!selectedTag) {
    showUpdateStatus("Pick a version to install.", true);
    return;
  }

  await installTag(selectedTag);
});

autoCheckUpdatesInput.addEventListener("change", () => {
  localStorage.setItem(AUTO_UPDATE_KEY, autoCheckUpdatesInput.checked ? "1" : "0");
});

updateChannelSelect.addEventListener("change", () => {
  localStorage.setItem(UPDATE_CHANNEL_KEY, selectedChannel());
  renderUpdateResult(null);
});

const initialize = async () => {
  targetTimeInput.value = toLocalDateTimeValue(LAUNCH_TIME);
  toggleMessage();
  toggleRecurrence();
  await loadTimers();
  setInterval(loadTimers, 1000);

  currentVersion = await getVersion();
  currentVersionEl.textContent = currentVersion;

  const savedChannel = localStorage.getItem(UPDATE_CHANNEL_KEY);
  updateChannelSelect.value = savedChannel === "dev" ? "dev" : "main";

  const autoCheckSetting = localStorage.getItem(AUTO_UPDATE_KEY);
  autoCheckUpdatesInput.checked = autoCheckSetting !== "0";

  await loadRollbackVersions();

  if (autoCheckUpdatesInput.checked) {
    await checkForUpdates(true);
  }
};

initialize().catch((err) => {
  showStatus(`Initialization failed: ${String(err)}`, true);
});
