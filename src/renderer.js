import { invoke } from './vendor/@tauri-apps/api/core.js';
import { PhysicalPosition, PhysicalSize } from './vendor/@tauri-apps/api/dpi.js';
import { currentMonitor, getCurrentWindow } from './vendor/@tauri-apps/api/window.js';

const appWindow = getCurrentWindow();
const updatedAt = document.getElementById('updated-at');
const dailyBars = document.getElementById('daily-bars');
const weeklyPath = document.getElementById('weekly-path');
const weeklyShadow = document.getElementById('weekly-shadow');
const refreshBtn = document.getElementById('refresh-btn');
const settingsBtn = document.getElementById('settings-btn');
const settingsPanel = document.getElementById('settings-panel');
const statusBanner = document.getElementById('status-banner');
const statusBadge = document.getElementById('status-badge');
const statusDetail = document.getElementById('status-detail');
const privacyToggle = document.getElementById('privacy-toggle');
const themeToggle = document.getElementById('theme-toggle');
const themeToggleRow = document.querySelector('.toggle-row');
const snapToggle = document.getElementById('snap-toggle');
const autostartToggle = document.getElementById('autostart-toggle');
const logDirInput = document.getElementById('log-dir-input');
const refreshIntervalSelect = document.getElementById('refresh-interval-select');
const summaryModeSelect = document.getElementById('summary-mode-select');
const quitBtn = document.getElementById('quit-btn');
const detailsToggle = document.getElementById('details-toggle');
const detailsToggleIcon = document.getElementById('details-toggle-icon');
const tokenBreakdown = document.getElementById('token-breakdown');

const primaryPercent = document.getElementById('primary-percent');
const secondaryPercent = document.getElementById('secondary-percent');
const primaryMeta = document.getElementById('primary-meta');
const secondaryMeta = document.getElementById('secondary-meta');
const primaryMeterFill = document.getElementById('primary-meter-fill');
const secondaryMeterFill = document.getElementById('secondary-meter-fill');
const primaryRemaining = document.getElementById('primary-remaining');
const secondaryRemaining = document.getElementById('secondary-remaining');
const primaryReset = document.getElementById('primary-reset');
const secondaryReset = document.getElementById('secondary-reset');
const totalTokens = document.getElementById('total-tokens');
const lastTokens = document.getElementById('last-tokens');
const summaryTokensLabel = document.getElementById('summary-tokens-label');
const syncTime = document.getElementById('sync-time');
const contextWindow = document.getElementById('context-window');
const scannedFiles = document.getElementById('scanned-files');
const totalInput = document.getElementById('total-input');
const totalOutput = document.getElementById('total-output');
const totalCached = document.getElementById('total-cached');
const totalReasoning = document.getElementById('total-reasoning');
const lastInput = document.getElementById('last-input');
const lastOutput = document.getElementById('last-output');
const lastCached = document.getElementById('last-cached');
const lastReasoning = document.getElementById('last-reasoning');
const totalWindowLabel = document.getElementById('total-window-label');
const lastWindowLabel = document.getElementById('last-window-label');
const sourceText = document.getElementById('source-text');
const planText = document.getElementById('plan-text');

const SNAP_STORAGE_KEY = 'codexviewer:snap-enabled';
const THEME_STORAGE_KEY = 'codexviewer:dark-mode';
const DETAILS_STORAGE_KEY = 'codexviewer:details-open';
const LOG_DIR_STORAGE_KEY = 'codexviewer:log-dir';
const REFRESH_INTERVAL_STORAGE_KEY = 'codexviewer:refresh-interval';
const SUMMARY_MODE_STORAGE_KEY = 'codexviewer:summary-mode';
const PRIVACY_MODE_STORAGE_KEY = 'codexviewer:privacy-mode';
const SNAP_THRESHOLD = 24;
const COLLAPSED_HEIGHT = 496;
const EXPANDED_HEIGHT = 650;

let isSnapping = false;
let currentWindowHeight = COLLAPSED_HEIGHT;
let refreshTimer = null;
let sourcePathRaw = 'Local Codex session logs';

async function isAutostartEnabled() {
  return invoke('plugin:autostart|is_enabled');
}

async function enableAutostart() {
  return invoke('plugin:autostart|enable');
}

async function disableAutostart() {
  return invoke('plugin:autostart|disable');
}

function formatTime(date) {
  return new Intl.DateTimeFormat('zh-CN', {
    hour: 'numeric',
    minute: '2-digit',
    second: '2-digit',
    hour12: true
  }).format(date);
}

function formatReset(unixSeconds) {
  if (!unixSeconds) {
    return '--';
  }

  return new Intl.DateTimeFormat('zh-CN', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  }).format(new Date(unixSeconds * 1000));
}

function formatNumber(value) {
  if (typeof value !== 'number') {
    return '--';
  }

  return new Intl.NumberFormat('en-US').format(value);
}

function formatCompactNumber(value) {
  if (typeof value !== 'number') {
    return '--';
  }

  return new Intl.NumberFormat('en-US', {
    notation: 'compact',
    maximumFractionDigits: value >= 1000000 ? 1 : 0
  }).format(value);
}

function formatPlanName(planType) {
  if (!planType) {
    return '--';
  }

  return String(planType)
    .split(/[_-\s]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function formatRelativeTime(date) {
  const diffMs = Date.now() - date.getTime();

  if (!Number.isFinite(diffMs)) {
    return '--';
  }

  const diffMinutes = Math.max(0, Math.round(diffMs / 60000));
  if (diffMinutes < 1) {
    return 'just now';
  }

  if (diffMinutes < 60) {
    return `${diffMinutes}m ago`;
  }

  const diffHours = Math.round(diffMinutes / 60);
  if (diffHours < 24) {
    return `${diffHours}h ago`;
  }

  const diffDays = Math.round(diffHours / 24);
  return `${diffDays}d ago`;
}

function clampPercent(value) {
  return Math.max(0, Math.min(100, Math.round(Number(value) || 0)));
}

function renderBars(values) {
  dailyBars.innerHTML = '';
  const peak = Math.max(...values, 1);

  values.forEach((value, index) => {
    const bar = document.createElement('span');
    const clamped = Math.max(0, Math.min(100, Number(value) || 0));
    const normalized = peak > 0 ? clamped / peak : 0;
    const height = 6 + normalized * 22;

    bar.className = 'bar';
    bar.style.height = `${height}px`;

    if (index === values.length - 1) {
      bar.classList.add('active');
    } else if (index === values.length - 2) {
      bar.classList.add('low');
      bar.style.height = `${Math.max(height - 7, 7)}px`;
    } else if (normalized >= 0.72) {
      bar.classList.add('low');
      bar.style.opacity = '0.62';
    } else {
      bar.style.opacity = '0.92';
    }

    if (index === values.length - 1) {
      bar.classList.add('latest');
    }

    dailyBars.appendChild(bar);
  });
}

function buildWeeklyPath(values) {
  const chartWidth = 264;
  const chartHeight = 36;
  const left = 8;
  const top = 20;
  const step = chartWidth / Math.max(values.length - 1, 1);

  return values
    .map((value, index) => {
      const clamped = Math.max(0, Math.min(100, Number(value) || 0));
      const x = left + step * index;
      const y = top + (chartHeight - (clamped / 100) * chartHeight);
      return `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(' ');
}

function applyTheme(isDark) {
  document.body.classList.toggle('dark', isDark);
  themeToggle.checked = isDark;
  localStorage.setItem(THEME_STORAGE_KEY, isDark ? '1' : '0');
}

function applySnapPreference(enabled) {
  snapToggle.checked = enabled;
  localStorage.setItem(SNAP_STORAGE_KEY, enabled ? '1' : '0');
}

function applyDetailsPreference(expanded) {
  document.querySelector('.widget')?.classList.toggle('is-expanded', expanded);
  tokenBreakdown.classList.toggle('is-collapsed', !expanded);
  detailsToggle.setAttribute('aria-expanded', expanded ? 'true' : 'false');
  detailsToggleIcon.textContent = expanded ? '-' : '+';
  localStorage.setItem(DETAILS_STORAGE_KEY, expanded ? '1' : '0');
}

function getConfiguredLogDir() {
  return (localStorage.getItem(LOG_DIR_STORAGE_KEY) || '').trim();
}

function getRefreshIntervalMs() {
  const raw = Number(localStorage.getItem(REFRESH_INTERVAL_STORAGE_KEY) || '45000');
  return Number.isFinite(raw) && raw >= 0 ? raw : 45000;
}

function getSummaryMode() {
  const mode = localStorage.getItem(SUMMARY_MODE_STORAGE_KEY);
  return mode === 'last' ? 'last' : 'session';
}

function applySummaryMode(mode) {
  summaryModeSelect.value = mode;
  summaryTokensLabel.textContent = mode === 'last' ? 'Last response tokens' : 'Current session tokens';
  localStorage.setItem(SUMMARY_MODE_STORAGE_KEY, mode);
}

function restartAutoRefresh() {
  if (refreshTimer) {
    window.clearInterval(refreshTimer);
    refreshTimer = null;
  }

  const intervalMs = getRefreshIntervalMs();
  if (intervalMs > 0) {
    refreshTimer = window.setInterval(() => {
      loadSnapshot();
    }, intervalMs);
  }
}

function applyRefreshInterval(intervalMs) {
  refreshIntervalSelect.value = String(intervalMs);
  localStorage.setItem(REFRESH_INTERVAL_STORAGE_KEY, String(intervalMs));
  restartAutoRefresh();
}

function applyLogDir(logDir) {
  logDirInput.value = logDir;
  localStorage.setItem(LOG_DIR_STORAGE_KEY, logDir);
}

function isPrivacyModeEnabled() {
  return localStorage.getItem(PRIVACY_MODE_STORAGE_KEY) !== '0';
}

function maskPath(path) {
  if (!path) {
    return 'Local Codex session logs';
  }

  const segments = String(path).split(/[\\/]+/).filter(Boolean);
  if (segments.length <= 2) {
    return 'Hidden path';
  }

  const lastSegment = segments[segments.length - 1];
  const rootMatch = String(path).match(/^[A-Za-z]:\\/);
  const prefix = rootMatch ? rootMatch[0] : '';
  return `${prefix}...\\${lastSegment}`;
}

function renderSourcePath() {
  const privacyEnabled = isPrivacyModeEnabled();
  const visibleText = privacyEnabled ? maskPath(sourcePathRaw) : sourcePathRaw;

  statusDetail.textContent = visibleText;
  statusDetail.title = privacyEnabled ? 'Privacy mode is on' : sourcePathRaw;
  privacyToggle.classList.toggle('is-private', privacyEnabled);
  privacyToggle.setAttribute('aria-label', privacyEnabled ? 'Show full path' : 'Hide full path');
  privacyToggle.title = privacyEnabled ? 'Show full path' : 'Hide full path';
}

function applyPrivacyMode(enabled) {
  localStorage.setItem(PRIVACY_MODE_STORAGE_KEY, enabled ? '1' : '0');
  renderSourcePath();
}

function setStatus(mode, label, detail) {
  statusBanner.classList.toggle('is-loading', mode === 'loading');
  statusBanner.classList.toggle('is-error', mode === 'error');
  statusBadge.textContent = label;
  sourcePathRaw = detail || 'Local Codex session logs';
  renderSourcePath();
}

function resetSnapshotView() {
  primaryPercent.textContent = '--';
  secondaryPercent.textContent = '--';
  primaryMeta.textContent = 'Resets --';
  secondaryMeta.textContent = 'Resets --';
  primaryMeterFill.style.width = '0%';
  secondaryMeterFill.style.width = '0%';
  primaryRemaining.textContent = 'Remaining --';
  secondaryRemaining.textContent = 'Remaining --';
  totalTokens.textContent = '--';
  lastTokens.textContent = '--';
  contextWindow.textContent = '--';
  scannedFiles.textContent = '--';
  totalInput.textContent = '--';
  totalOutput.textContent = '--';
  totalCached.textContent = '--';
  totalReasoning.textContent = '--';
  lastInput.textContent = '--';
  lastOutput.textContent = '--';
  lastCached.textContent = '--';
  lastReasoning.textContent = '--';
  totalWindowLabel.textContent = 'Current session total';
  lastWindowLabel.textContent = 'Most recent response';
  syncTime.textContent = '--';
  dailyBars.innerHTML = '';
  weeklyPath.setAttribute('d', '');
  weeklyShadow.setAttribute('d', '');
}

async function syncWindowHeight(expanded) {
  const nextHeight = expanded ? EXPANDED_HEIGHT : COLLAPSED_HEIGHT;
  if (currentWindowHeight === nextHeight) {
    return;
  }

  const position = await appWindow.outerPosition();
  const size = await appWindow.outerSize();
  const deltaHeight = nextHeight - size.height;
  const nextY = Math.max(position.y - deltaHeight, 0);

  await appWindow.setSize(new PhysicalSize(size.width, nextHeight));
  await appWindow.setPosition(new PhysicalPosition(position.x, nextY));
  currentWindowHeight = nextHeight;
}

async function syncAutostartState() {
  try {
    autostartToggle.checked = await isAutostartEnabled();
  } catch (error) {
    autostartToggle.disabled = true;
    sourceText.textContent = 'Source: local Codex session logs | autostart unavailable';
    console.error(error);
  }
}

async function maybeSnapToEdges() {
  if (!snapToggle.checked || isSnapping) {
    return;
  }

  const monitor = await currentMonitor();
  if (!monitor) {
    return;
  }

  const position = await appWindow.outerPosition();
  const size = await appWindow.outerSize();
  const monitorLeft = monitor.position.x;
  const monitorTop = monitor.position.y;
  const monitorRight = monitor.position.x + monitor.size.width;
  const monitorBottom = monitor.position.y + monitor.size.height;

  let nextX = position.x;
  let nextY = position.y;

  if (Math.abs(position.x - monitorLeft) <= SNAP_THRESHOLD) {
    nextX = monitorLeft;
  } else if (Math.abs(monitorRight - (position.x + size.width)) <= SNAP_THRESHOLD) {
    nextX = monitorRight - size.width;
  }

  if (Math.abs(position.y - monitorTop) <= SNAP_THRESHOLD) {
    nextY = monitorTop;
  } else if (Math.abs(monitorBottom - (position.y + size.height)) <= SNAP_THRESHOLD) {
    nextY = monitorBottom - size.height;
  }

  if (nextX === position.x && nextY === position.y) {
    return;
  }

  isSnapping = true;

  try {
    await appWindow.setPosition(new PhysicalPosition(nextX, nextY));
  } finally {
    window.setTimeout(() => {
      isSnapping = false;
    }, 120);
  }
}

function renderSnapshot(snapshot) {
  const latestAt = new Date(snapshot.last_event_at * 1000);
  const weeklyD = buildWeeklyPath(snapshot.weekly_secondary_percents || []);
  const totalUsage = snapshot.total_usage || null;
  const lastUsage = snapshot.last_usage || null;
  const totalTokenValue = totalUsage?.total_tokens ?? lastUsage?.total_tokens;
  const lastTokenValue = lastUsage?.total_tokens;
  const summaryMode = getSummaryMode();
  const mainTokenValue = summaryMode === 'last' ? lastTokenValue : totalTokenValue;
  const primaryUsed = clampPercent(snapshot.primary.used_percent);
  const secondaryUsed = clampPercent(snapshot.secondary.used_percent);
  const primaryRemainingPercent = Math.max(0, 100 - primaryUsed);
  const secondaryRemainingPercent = Math.max(0, 100 - secondaryUsed);
  const refreshLabel = formatTime(new Date());
  const eventLabel = formatRelativeTime(latestAt);

  updatedAt.textContent = `Last refresh ${refreshLabel} | event ${eventLabel}`;
  planText.textContent = `Plan ${formatPlanName(snapshot.plan_type)}`;
  sourceText.textContent = `${snapshot.event_count} events from ${snapshot.scanned_files} files`;
  setStatus('ready', 'Connected', snapshot.source_label);

  primaryPercent.textContent = `${primaryUsed}%`;
  secondaryPercent.textContent = `${secondaryUsed}%`;
  primaryMeta.textContent = `Used ${primaryUsed}% | resets ${formatReset(snapshot.primary.resets_at)}`;
  secondaryMeta.textContent = `Used ${secondaryUsed}% | resets ${formatReset(snapshot.secondary.resets_at)}`;
  primaryMeterFill.style.width = `${primaryUsed}%`;
  secondaryMeterFill.style.width = `${secondaryUsed}%`;
  primaryRemaining.textContent = `Remaining ${primaryRemainingPercent}% in this ${snapshot.primary.window_minutes / 60}h window`;
  secondaryRemaining.textContent = `Remaining ${secondaryRemainingPercent}% this week`;
  primaryReset.textContent = `5 hour window ${snapshot.primary.window_minutes / 60}h`;
  secondaryReset.textContent = formatReset(snapshot.secondary.resets_at);

  totalTokens.textContent = formatNumber(mainTokenValue);
  lastTokens.textContent = formatNumber(lastTokenValue);
  contextWindow.textContent = formatCompactNumber(snapshot.model_context_window);
  scannedFiles.textContent = formatNumber(snapshot.scanned_files);
  totalInput.textContent = formatNumber(totalUsage?.input_tokens);
  totalOutput.textContent = formatNumber(totalUsage?.output_tokens);
  totalCached.textContent = formatNumber(totalUsage?.cached_input_tokens);
  totalReasoning.textContent = formatNumber(totalUsage?.reasoning_output_tokens);
  lastInput.textContent = formatNumber(lastUsage?.input_tokens);
  lastOutput.textContent = formatNumber(lastUsage?.output_tokens);
  lastCached.textContent = formatNumber(lastUsage?.cached_input_tokens);
  lastReasoning.textContent = formatNumber(lastUsage?.reasoning_output_tokens);
  totalWindowLabel.textContent = totalUsage ? `Total ${formatNumber(totalUsage.total_tokens)}` : 'Current session total';
  lastWindowLabel.textContent = lastUsage ? `Total ${formatNumber(lastUsage.total_tokens)}` : 'Most recent response';
  syncTime.textContent = `${latestAt.toLocaleString('zh-CN', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  })} (${eventLabel})`;

  renderBars(snapshot.hourly_primary_percents || []);
  weeklyPath.setAttribute('d', weeklyD);
  weeklyShadow.setAttribute('d', weeklyD);

  updatedAt.title = `Source ${snapshot.source_label}\nLast usage event ${latestAt.toLocaleString('zh-CN')}`;
}

async function loadSnapshot() {
  try {
    refreshBtn.disabled = true;
    setStatus('loading', 'Reading', getConfiguredLogDir() || 'Scanning ~/.codex/sessions');
    updatedAt.textContent = 'Reading local usage logs...';
    const sessionsDir = getConfiguredLogDir();
    const snapshot = await invoke('load_usage_snapshot', {
      sessionsDir: sessionsDir || null
    });
    renderSnapshot(snapshot);
  } catch (error) {
    resetSnapshotView();
    updatedAt.textContent = 'Read failed';
    sourceText.textContent = `Read failed: ${String(error)}`;
    planText.textContent = 'Plan --';
    setStatus('error', 'Error', String(error));
    console.error(error);
  } finally {
    refreshBtn.disabled = false;
  }
}

settingsBtn.addEventListener('click', () => {
  settingsPanel.classList.toggle('is-hidden');
});

refreshBtn.addEventListener('click', () => {
  loadSnapshot();
});

themeToggle.addEventListener('change', (event) => {
  applyTheme(event.target.checked);
});

themeToggleRow?.addEventListener('click', (event) => {
  if (event.target === themeToggle) {
    return;
  }

  event.preventDefault();
  applyTheme(!document.body.classList.contains('dark'));
});

privacyToggle?.addEventListener('click', () => {
  applyPrivacyMode(!isPrivacyModeEnabled());
});

snapToggle.addEventListener('change', (event) => {
  applySnapPreference(event.target.checked);
});

logDirInput.addEventListener('change', () => {
  applyLogDir(logDirInput.value.trim());
  loadSnapshot();
});

refreshIntervalSelect.addEventListener('change', () => {
  applyRefreshInterval(Number(refreshIntervalSelect.value));
});

summaryModeSelect.addEventListener('change', () => {
  applySummaryMode(summaryModeSelect.value);
  loadSnapshot();
});

autostartToggle.addEventListener('change', async (event) => {
  try {
    if (event.target.checked) {
      await enableAutostart();
    } else {
      await disableAutostart();
    }
  } catch (error) {
    event.target.checked = !event.target.checked;
    console.error(error);
  }
});

detailsToggle.addEventListener('click', () => {
  const isExpanded = detailsToggle.getAttribute('aria-expanded') === 'true';
  applyDetailsPreference(!isExpanded);
  syncWindowHeight(!isExpanded).catch(console.error);
});

quitBtn.addEventListener('click', async () => {
  await invoke('quit_app');
});

await appWindow.onMoved(() => {
  window.setTimeout(() => {
    maybeSnapToEdges().catch(console.error);
  }, 60);
});

applyTheme(localStorage.getItem(THEME_STORAGE_KEY) === '1');
applySnapPreference(localStorage.getItem(SNAP_STORAGE_KEY) !== '0');
applyLogDir(getConfiguredLogDir());
applySummaryMode(getSummaryMode());
applyRefreshInterval(getRefreshIntervalMs());
applyPrivacyMode(isPrivacyModeEnabled());
const detailsExpanded = localStorage.getItem(DETAILS_STORAGE_KEY) === '1';
applyDetailsPreference(detailsExpanded);
await syncWindowHeight(detailsExpanded);
await syncAutostartState();
await loadSnapshot();
