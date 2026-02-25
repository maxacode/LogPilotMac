const { invoke } = window.__TAURI__.core;
const { getVersion } = window.__TAURI__.app;

const form = document.getElementById("timer-form");
const actionInput = document.getElementById("action");
const targetTimeInput = document.getElementById("target-time");
const messageWrap = document.getElementById("message-wrap");
const messageInput = document.getElementById("message");
const timersEl = document.getElementById("timers");
const statusEl = document.getElementById("status");
const refreshBtn = document.getElementById("refresh");

const checkUpdatesBtn = document.getElementById("check-updates");
const autoCheckUpdatesInput = document.getElementById("auto-check-updates");
const currentVersionEl = document.getElementById("current-version");
const updateStatusEl = document.getElementById("update-status");
const updateResultEl = document.getElementById("update-result");
const latestVersionEl = document.getElementById("latest-version");
const latestNotesEl = document.getElementById("latest-notes");
const installLatestBtn = document.getElementById("install-latest");
const rollbackVersionSelect = document.getElementById("rollback-version");
const rollbackInstallBtn = document.getElementById("rollback-install");

const AUTO_UPDATE_KEY = "lockpilot.autoCheckUpdates";
let currentVersion = "";
let latestUpdate = null;

const showStatus = (text, isError = false) => {
  statusEl.textContent = text;
  statusEl.style.color = isError ? "#c30e2e" : "#475569";
};

const showUpdateStatus = (text, isError = false) => {
  updateStatusEl.textContent = text;
  updateStatusEl.style.color = isError ? "#c30e2e" : "#475569";
};

const toggleMessage = () => {
  const isPopup = actionInput.value === "popup";
  messageWrap.style.display = isPopup ? "grid" : "none";
  messageInput.required = isPopup;
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

    item.append(top, when);

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

  try {
    showUpdateStatus("Checking GitHub releases...");
    const update = await invoke("check_for_updates", { currentVersion });
    renderUpdateResult(update);

    if (update) {
      showUpdateStatus(`Update available: ${update.tag}`);
    } else if (!silentWhenUpToDate) {
      showUpdateStatus("You are on the latest version.");
    } else {
      showUpdateStatus("");
    }
  } catch (err) {
    showUpdateStatus(`Update check failed: ${String(err)}`, true);
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

  const request = {
    action: actionInput.value,
    targetTime: new Date(targetTimeInput.value).toISOString(),
    message: actionInput.value === "popup" ? messageInput.value : null,
  };

  try {
    await invoke("create_timer", { request });
    form.reset();
    toggleMessage();
    showStatus("Timer created.");
    await loadTimers();
  } catch (err) {
    showStatus(String(err), true);
  }
});

refreshBtn.addEventListener("click", loadTimers);
actionInput.addEventListener("change", toggleMessage);

checkUpdatesBtn.addEventListener("click", () => checkForUpdates(false));
installLatestBtn.addEventListener("click", async () => {
  if (!latestUpdate) {
    showUpdateStatus("No update selected.", true);
    return;
  }

  await installTag(latestUpdate.tag);
});

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

const initialize = async () => {
  toggleMessage();
  await loadTimers();
  setInterval(loadTimers, 1000);

  currentVersion = await getVersion();
  currentVersionEl.textContent = currentVersion;

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
