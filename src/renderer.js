import { invoke } from './vendor/@tauri-apps/api/core.js';
import { getVersion } from './vendor/@tauri-apps/api/app.js';
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
const languageButtons = document.querySelectorAll('[data-language]');
const logDirInput = document.getElementById('log-dir-input');
const refreshIntervalSelect = document.getElementById('refresh-interval-select');
const summaryModeSelect = document.getElementById('summary-mode-select');
const relayEnabledToggle = document.getElementById('relay-enabled-toggle');
const relayRouteStatus = document.getElementById('relay-route-status');
const relayProviderIdInput = document.getElementById('relay-provider-id-input');
const relayRouteModeSelect = document.getElementById('relay-route-mode-select');
const relayBaseUrlInput = document.getElementById('relay-base-url-input');
const relayApiKeyInput = document.getElementById('relay-api-key-input');
const relayKeyVisibility = document.getElementById('relay-key-visibility');
const relayTestModelInput = document.getElementById('relay-test-model-input');
const relayUpstreamModelInput = document.getElementById('relay-upstream-model-input');
const relayStatusRoute = document.getElementById('relay-status-route');
const relayStatusConfig = document.getElementById('relay-status-config');
const relayStatusCodex = document.getElementById('relay-status-codex');
const relayLastApply = document.getElementById('relay-last-apply');
const relayActionStatus = document.getElementById('relay-action-status');
const relayApplyBtn = document.getElementById('relay-apply-btn');
const relayRestartBtn = document.getElementById('relay-restart-btn');
const relayClearBtn = document.getElementById('relay-clear-btn');
const relayTestBtn = document.getElementById('relay-test-btn');
const pluginUnlockBtn = document.getElementById('plugin-unlock-btn');
const relayProviderList = document.getElementById('relay-provider-list');
const historySyncProvider = document.getElementById('history-sync-provider');
const historyProvider = document.getElementById('history-provider');
const historyRolloutPending = document.getElementById('history-rollout-pending');
const historySqlitePending = document.getElementById('history-sqlite-pending');
const historyBackups = document.getElementById('history-backups');
const historyProviderList = document.getElementById('history-provider-list');
const historyActionStatus = document.getElementById('history-action-status');
const historyStatusBtn = document.getElementById('history-status-btn');
const historySyncBtn = document.getElementById('history-sync-btn');
const contextConfigPath = document.getElementById('context-config-path');
const contextRefreshBtn = document.getElementById('context-refresh-btn');
const contextTabs = document.querySelectorAll('[data-context-kind]');
const contextEntryList = document.getElementById('context-entry-list');
const contextEditorTitle = document.getElementById('context-editor-title');
const contextNewBtn = document.getElementById('context-new-btn');
const contextEntryIdInput = document.getElementById('context-entry-id-input');
const contextEntryBodyInput = document.getElementById('context-entry-body-input');
const contextDeleteBtn = document.getElementById('context-delete-btn');
const contextSaveBtn = document.getElementById('context-save-btn');
const contextActionStatus = document.getElementById('context-action-status');
const quitBtn = document.getElementById('quit-btn');
const detailsToggle = document.getElementById('details-toggle');
const detailsToggleIcon = document.getElementById('details-toggle-icon');
const tokenBreakdown = document.getElementById('token-breakdown');

const primaryPercent = document.getElementById('primary-percent');
const secondaryPercent = document.getElementById('secondary-percent');
const primaryStatLabel = document.querySelector('.stat-card-primary > span');
const secondaryStatLabel = document.querySelector('.stat-card-secondary > span');
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
const providerTokenList = document.getElementById('provider-token-list');
const sourceText = document.getElementById('source-text');
const planText = document.getElementById('plan-text');
const appVersion = document.getElementById('app-version');

const SNAP_STORAGE_KEY = 'codexviewer:snap-enabled';
const THEME_STORAGE_KEY = 'codexviewer:dark-mode';
const DETAILS_STORAGE_KEY = 'codexviewer:details-open';
const LOG_DIR_STORAGE_KEY = 'codexviewer:log-dir';
const REFRESH_INTERVAL_STORAGE_KEY = 'codexviewer:refresh-interval';
const SUMMARY_MODE_STORAGE_KEY = 'codexviewer:summary-mode';
const PRIVACY_MODE_STORAGE_KEY = 'codexviewer:privacy-mode';
const LANGUAGE_STORAGE_KEY = 'codexviewer:language';
const RELAY_LAST_APPLY_STORAGE_KEY = 'codexviewer:relay-last-apply';
const DEFAULT_CODEX_MODEL = 'gpt-5.5';
const SNAP_THRESHOLD = 24;
const COLLAPSED_HEIGHT = 496;
const EXPANDED_HEIGHT = 760;

let isSnapping = false;
let currentWindowHeight = COLLAPSED_HEIGHT;
let refreshTimer = null;
let sourcePathRaw = 'Local Codex session logs';
let currentRelayStatus = null;
let currentSnapshot = null;
let initialSnapshotRequested = false;
let currentLanguage = 'en';
let currentContextKind = 'plugin';
let currentContextEntries = null;
let selectedRelayProvider = null;
let selectedContextEntry = null;

async function loadAppVersion() {
  if (!appVersion) {
    return;
  }
  try {
    appVersion.textContent = `v${await getVersion()}`;
  } catch (error) {
    appVersion.textContent = 'v1.1.2';
  }
}

const I18N = {
  en: {
    waiting: 'Waiting',
    refresh: 'Refresh',
    settings: 'Settings',
    togglePrivacy: 'Toggle privacy mode',
    showFullPath: 'Show full path',
    hideFullPath: 'Hide full path',
    localLogs: 'Local Codex session logs',
    hiddenPath: 'Hidden path',
    languageTitle: 'Language',
    languageHint: 'Switch interface language.',
    autostartTitle: 'Autostart',
    autostartHint: 'Launch the widget after sign-in.',
    snapTitle: 'Snap to edges',
    snapHint: 'Snap when the widget is close to a screen edge.',
    logDirLabel: 'Codex session log directory',
    refreshLabel: 'Auto refresh',
    refresh15: '15 seconds',
    refresh30: '30 seconds',
    refresh45: '45 seconds',
    refresh60: '60 seconds',
    refreshManual: 'Manual only',
    summaryModeLabel: 'Main token card',
    summarySession: 'Current session',
    summaryLast: 'Last response',
    relayTitle: 'Relay manager',
    officialEndpoint: 'Official endpoint',
    relayNotConfigured: 'Relay not configured',
    relayActive: 'Relay active',
    providerIdLabel: 'Provider ID',
    routeModeLabel: 'Route mode',
    routeModeDirect: 'Direct Responses',
    routeModeLocalRouter: 'Local router',
    baseUrlLabel: 'API Base URL',
    apiKeyLabel: 'API Key',
    show: 'Show',
    hide: 'Hide',
    testModelLabel: 'Codex model',
    upstreamModelLabel: 'Upstream model',
    route: 'Route',
    config: 'Config',
    codex: 'Codex',
    lastApply: 'Last apply',
    save: 'Save',
    applyToCodex: 'Apply to Codex',
    applyRestart: 'Apply & Restart Codex',
    restoreOfficial: 'Restore official',
    testProvider: 'Test provider',
    testingProvider: 'Testing provider...',
    providerTestPassed: 'Provider test passed',
    providerTestFailed: 'Provider test failed',
    historySyncTitle: 'History sync',
    targetProvider: 'Provider',
    rolloutPending: 'Pending files',
    sqlitePending: 'Pending DB rows',
    backups: 'Backups',
    refreshStatus: 'Refresh status',
    syncHistory: 'Sync history',
    currentProviderBadge: 'Current',
    savedProviders: 'Saved providers',
    selectProviderHint: 'Click to load',
    providerSelected: (provider) => `${provider} loaded into relay settings.`,
    officialProviderSelected: 'openai loaded. Apply restore official if you want to switch back.',
    noProviderHistory: 'No provider history found',
    allProviderHistory: 'All provider records',
    historyRecordColumns: 'Total / files / DB',
    historyProviderCounts: (total, files, rows) => `${total} total | ${files} files | ${rows} DB`,
    contextManagerTitle: 'Tools & plugins',
    pluginsTab: 'Plugins',
    newContextEntry: 'New entry',
    editContextEntry: (id) => `Edit ${id}`,
    newEntry: 'New',
    entryIdLabel: 'Entry ID',
    tomlBodyLabel: 'TOML body',
    saveEntry: 'Save entry',
    deleteEntry: 'Delete',
    editEntry: 'Edit',
    noContextEntries: 'No entries in this section',
    loadingContextEntries: 'Reading tools and plugins...',
    contextEntriesLoaded: (total) => `${total} tool/plugin entr${total === 1 ? 'y' : 'ies'} loaded`,
    savingContextEntry: 'Saving entry...',
    togglingContextEntry: 'Updating entry...',
    deletingContextEntry: 'Deleting entry...',
    contextEntryIdRequired: 'Entry ID cannot be empty.',
    contextBodyRequired: 'TOML body cannot be empty.',
    ready: 'Ready',
    settingsNote: 'Source is local Codex session logs. This is not the OpenAI API Usage endpoint and not a remote ChatGPT Plus dashboard.',
    sourcePrefix: 'Source',
    planPrefix: 'Plan',
    trend24: '24 Hour Trend',
    trend7: '7 Day Trend',
    fiveHourWindow: '5 Hour Window',
    weeklyWindow: 'Weekly Window',
    currentSessionTokens: 'Current session tokens',
    lastResponseTokens: 'Last response tokens',
    lastUsageEvent: 'Last usage event',
    contextWindow: 'Context window',
    scannedFiles: 'Scanned files',
    detailedTokenUsage: 'Detailed token usage',
    sessionTotals: 'Session Totals',
    currentSessionTotal: 'Current session total',
    mostRecentResponse: 'Most recent response',
    lastResponse: 'Last Response',
    providerTokenUsage: 'Provider Tokens',
    localLogStats: 'Local log stats',
    noProviderTokens: 'No provider token records',
    providerTokenLine: (total, events) => `${total} tokens | ${events} events`,
    input: 'Input',
    output: 'Output',
    cached: 'Cached',
    reasoning: 'Reasoning',
    remaining: 'Remaining',
    used: 'Used',
    resets: 'resets',
    inThisWindow: 'in this',
    thisWeek: 'this week',
    total: 'Total',
    dayNight: 'Day/Night',
    quit: 'Quit',
    connected: 'Connected',
    reading: 'Reading',
    readFailed: 'Read failed',
    scanningLogs: 'Scanning ~/.codex/sessions',
    readingLogs: 'Reading local usage logs...',
    justNow: 'just now',
    minutesAgo: (value) => `${value}m ago`,
    hoursAgo: (value) => `${value}h ago`,
    daysAgo: (value) => `${value}d ago`,
    lastRefresh: (refresh, event) => `Last refresh ${refresh} | event ${event}`,
    eventsFromFiles: (events, files) => `${events} events from ${files} files`,
    tokensFromEvents: (route, events, files) => `${route} tokens | ${events} events from ${files} files`,
    tokenLogs: (route, source) => `${route} token logs | ${source}`,
    relayTokenLogs: (route, baseUrl, source) => `${route} token logs via ${baseUrl} | ${source}`,
    sessionTokens: (route) => `${route} session tokens`,
    responseTokens: (route) => `${route} response tokens`,
    remainingWindow: (remaining, hours) => `Remaining ${remaining}% in this ${hours}h window`,
    remainingWeek: (remaining) => `Remaining ${remaining}% this week`,
    saving: 'Saving...',
    relaySaved: 'Relay settings saved.',
    applyingRelay: 'Applying relay config...',
    applyingRestart: 'Applying and restarting Codex...',
    restoringOfficial: 'Restoring official endpoint...',
    unlockPlugins: 'Unlock plugins for API mode',
    launchUnlockPlugins: 'Launch Codex with plugin unlock',
    unlockingPlugins: 'Launching Codex with plugin unlock...',
    pluginsUnlocked: 'Codex launched and plugin unlock injected. Open Plugins in Codex to verify.',
    loadingHistoryStatus: 'Reading history status...',
    syncingHistory: 'Syncing history...',
    historyStatusLoaded: (rollouts, rows) => `${rollouts} rollout file(s), ${rows} SQLite row(s) pending`,
    historySynced: (files, rows) => `Synced ${files} session file(s), ${rows} SQLite row(s).`,
    configured: 'Configured',
    notApplied: 'Not applied',
    running: 'Running',
    notRunning: 'Not running',
    relayDisabledError: 'Relay is disabled.',
    emptyBaseUrlError: 'API Base URL cannot be empty.',
    invalidBaseUrlError: 'API Base URL must start with http:// or https://.',
    emptyApiKeyError: 'API Key cannot be empty.',
    invalidProviderIdError: 'Provider ID can only contain letters, numbers, underscore and hyphen.',
    relayConfiguredFallback: 'configured relay',
    official: 'Official',
    relay: 'Relay',
    localRouter: 'Local router'
  },
  zh: {
    waiting: '等待',
    refresh: '刷新',
    settings: '设置',
    togglePrivacy: '切换隐私模式',
    showFullPath: '显示完整路径',
    hideFullPath: '隐藏完整路径',
    localLogs: '本地 Codex 会话日志',
    hiddenPath: '路径已隐藏',
    languageTitle: '语言',
    languageHint: '切换界面语言。',
    autostartTitle: '开机启动',
    autostartHint: '登录系统后自动启动小组件。',
    snapTitle: '贴边吸附',
    snapHint: '窗口靠近屏幕边缘时自动吸附。',
    logDirLabel: 'Codex 会话日志目录',
    refreshLabel: '自动刷新',
    refresh15: '15 秒',
    refresh30: '30 秒',
    refresh45: '45 秒',
    refresh60: '60 秒',
    refreshManual: '仅手动',
    summaryModeLabel: '主 token 卡片',
    summarySession: '当前会话',
    summaryLast: '最近回复',
    relayTitle: '中转站管理',
    officialEndpoint: '官方端点',
    relayNotConfigured: '中转站未配置',
    relayActive: '中转站已启用',
    providerIdLabel: 'Provider ID',
    routeModeLabel: '路由模式',
    routeModeDirect: '直连 Responses',
    routeModeLocalRouter: '本地路由器',
    baseUrlLabel: 'API Base URL',
    apiKeyLabel: 'API Key',
    show: '显示',
    hide: '隐藏',
    testModelLabel: 'Codex 模型',
    upstreamModelLabel: '上游模型',
    route: '路由',
    config: '配置',
    codex: 'Codex',
    lastApply: '最后应用',
    save: '保存',
    applyToCodex: '应用到 Codex',
    applyRestart: '应用并重启 Codex',
    restoreOfficial: '恢复官方端点',
    testProvider: '测试 Provider',
    testingProvider: '正在测试 Provider...',
    providerTestPassed: 'Provider 测试通过',
    providerTestFailed: 'Provider 测试失败',
    historySyncTitle: '历史同步',
    targetProvider: 'Provider',
    rolloutPending: '待同步文件',
    sqlitePending: '待同步数据库行',
    backups: '备份',
    refreshStatus: '刷新状态',
    syncHistory: '同步历史',
    currentProviderBadge: '当前',
    savedProviders: '已有 Provider',
    selectProviderHint: '点击回显',
    providerSelected: (provider) => `已将 ${provider} 回显到中转站配置。`,
    officialProviderSelected: '已回显 openai。需要切回官方时点击恢复官方端点。',
    noProviderHistory: '没有找到 provider 历史',
    allProviderHistory: '所有 Provider 记录',
    historyRecordColumns: '总数 / 文件 / 数据库',
    historyProviderCounts: (total, files, rows) => `总数 ${total} | 文件 ${files} | 数据库 ${rows}`,
    ready: '就绪',
    settingsNote: '来源是本地 Codex 会话日志，不是 OpenAI API Usage 接口，也不是远程 ChatGPT Plus 仪表盘。',
    sourcePrefix: '来源',
    planPrefix: '套餐',
    trend24: '24 小时趋势',
    trend7: '7 天趋势',
    fiveHourWindow: '5 小时窗口',
    weeklyWindow: '每周窗口',
    currentSessionTokens: '当前会话 token',
    lastResponseTokens: '最近回复 token',
    lastUsageEvent: '最近使用事件',
    contextWindow: '上下文窗口',
    scannedFiles: '扫描文件',
    detailedTokenUsage: '详细 token 用量',
    sessionTotals: '会话总计',
    currentSessionTotal: '当前会话总计',
    mostRecentResponse: '最近一次回复',
    lastResponse: '最近回复',
    providerTokenUsage: 'Provider Tokens',
    localLogStats: '本地日志统计',
    noProviderTokens: '没有 provider token 记录',
    providerTokenLine: (total, events) => `${total} tokens | ${events} 条记录`,
    input: '输入',
    output: '输出',
    cached: '缓存',
    reasoning: '推理',
    remaining: '剩余',
    used: '已用',
    resets: '重置',
    inThisWindow: '在当前',
    thisWeek: '本周',
    total: '总计',
    dayNight: '日间/夜间',
    quit: '退出',
    connected: '已连接',
    reading: '读取中',
    readFailed: '读取失败',
    scanningLogs: '扫描 ~/.codex/sessions',
    readingLogs: '正在读取本地用量日志...',
    justNow: '刚刚',
    minutesAgo: (value) => `${value} 分钟前`,
    hoursAgo: (value) => `${value} 小时前`,
    daysAgo: (value) => `${value} 天前`,
    lastRefresh: (refresh, event) => `刷新 ${refresh} | 事件 ${event}`,
    eventsFromFiles: (events, files) => `${events} 个事件，来自 ${files} 个文件`,
    tokensFromEvents: (route, events, files) => `${route} token | ${events} 个事件，来自 ${files} 个文件`,
    tokenLogs: (route, source) => `${route} token 日志 | ${source}`,
    relayTokenLogs: (route, baseUrl, source) => `${route} token 日志，经由 ${baseUrl} | ${source}`,
    sessionTokens: (route) => `${route} 会话 token`,
    responseTokens: (route) => `${route} 回复 token`,
    remainingWindow: (remaining, hours) => `剩余 ${remaining}%，当前 ${hours} 小时窗口`,
    remainingWeek: (remaining) => `本周剩余 ${remaining}%`,
    saving: '保存中...',
    relaySaved: '中转站设置已保存。',
    applyingRelay: '正在应用中转站配置...',
    applyingRestart: '正在应用并重启 Codex...',
    restoringOfficial: '正在恢复官方端点...',
    loadingHistoryStatus: '正在读取历史状态...',
    syncingHistory: '正在同步历史...',
    historyStatusLoaded: (rollouts, rows) => `${rollouts} 个 rollout 文件，${rows} 行 SQLite 待同步`,
    historySynced: (files, rows) => `已同步 ${files} 个会话文件，${rows} 行 SQLite。`,
    configured: '已配置',
    notApplied: '未应用',
    running: '运行中',
    notRunning: '未运行',
    relayDisabledError: '中转站未启用。',
    emptyBaseUrlError: 'API Base URL 不能为空。',
    invalidBaseUrlError: 'API Base URL 必须以 http:// 或 https:// 开头。',
    emptyApiKeyError: 'API Key 不能为空。',
    invalidProviderIdError: 'Provider ID 只能包含英文、数字、下划线和连字符。',
    relayConfiguredFallback: '已配置中转站',
    official: '官方',
    relay: '中转站',
    localRouter: '本地路由器'
  }
};

function t(key, ...args) {
  const value = I18N[currentLanguage]?.[key] ?? I18N.en[key] ?? key;
  return typeof value === 'function' ? value(...args) : value;
}

function getInitialLanguage() {
  const saved = localStorage.getItem(LANGUAGE_STORAGE_KEY);
  if (saved === 'en' || saved === 'zh') {
    return saved;
  }
  return navigator.language?.toLowerCase().startsWith('zh') ? 'zh' : 'en';
}

function applyLanguage(language) {
  currentLanguage = language === 'zh' ? 'zh' : 'en';
  document.documentElement.lang = currentLanguage === 'zh' ? 'zh-CN' : 'en';
  localStorage.setItem(LANGUAGE_STORAGE_KEY, currentLanguage);

  document.querySelectorAll('[data-i18n]').forEach((element) => {
    element.textContent = t(element.dataset.i18n);
  });

  languageButtons.forEach((button) => {
    const isActive = button.dataset.language === currentLanguage;
    button.classList.toggle('is-active', isActive);
    button.setAttribute('aria-pressed', isActive ? 'true' : 'false');
  });

  refreshBtn.setAttribute('aria-label', t('refresh'));
  settingsBtn.setAttribute('aria-label', t('settings'));
  logDirInput.placeholder = 'Default: ~/.codex/sessions';
  privacyToggle.setAttribute('aria-label', t('togglePrivacy'));
  privacyToggle.title = t('togglePrivacy');
  relayActionStatus.textContent = relayActionStatus.textContent === 'Ready' ? t('ready') : relayActionStatus.textContent;
  renderSummaryTokenLabel();
  renderSourcePath();
  syncRelayEnabledState();
  if (currentRelayStatus) {
    renderRelayStatus(currentRelayStatus);
  }
  renderContextEntries();
  if (!selectedContextEntry) {
    contextEditorTitle.textContent = t('newContextEntry');
  }
}

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
  return new Intl.DateTimeFormat(currentLanguage === 'zh' ? 'zh-CN' : 'en-US', {
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

  return new Intl.DateTimeFormat(currentLanguage === 'zh' ? 'zh-CN' : 'en-US', {
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
    return t('justNow');
  }

  if (diffMinutes < 60) {
    return t('minutesAgo', diffMinutes);
  }

  const diffHours = Math.round(diffMinutes / 60);
  if (diffHours < 24) {
    return t('hoursAgo', diffHours);
  }

  const diffDays = Math.round(diffHours / 24);
  return t('daysAgo', diffDays);
}

function clampPercent(value) {
  return Math.max(0, Math.min(100, Math.round(Number(value) || 0)));
}

function bucketTotal(bucket) {
  return Number(firstDefined(bucket?.total_tokens, bucket?.totalTokens, 0)) || 0;
}

function bucketEvents(bucket) {
  return Number(firstDefined(bucket?.event_count, bucket?.eventCount, 0)) || 0;
}

function renderBars(values, options = {}) {
  dailyBars.innerHTML = '';
  const rawValues = options.mode === 'tokens' ? values.map(bucketTotal) : values;
  const peak = Math.max(...rawValues, 1);

  rawValues.forEach((value, index) => {
    const bar = document.createElement('span');
    const clamped = options.mode === 'tokens'
      ? Math.max(0, Number(value) || 0)
      : Math.max(0, Math.min(100, Number(value) || 0));
    const normalized = peak > 0 ? clamped / peak : 0;
    const height = 6 + normalized * 22;

    bar.className = 'bar';
    bar.style.height = `${height}px`;
    if (options.mode === 'tokens') {
      const events = bucketEvents(values[index]);
      bar.title = `${formatNumber(clamped)} tokens | ${events} events`;
    } else {
      bar.title = `${Math.round(clamped)}%`;
    }

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
  detailsToggleIcon.textContent = expanded ? '▲' : '▼';
  localStorage.setItem(DETAILS_STORAGE_KEY, expanded ? '1' : '0');
}

function buildTokenWeeklyPath(buckets) {
  const values = buckets.map(bucketTotal);
  const peak = Math.max(...values, 1);
  return buildWeeklyPath(values.map((value) => (value / peak) * 100));
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
  renderSummaryTokenLabel();
  localStorage.setItem(SUMMARY_MODE_STORAGE_KEY, mode);
}

function getUsageRouteLabel() {
  return currentRelayStatus?.route === 'relay' ? t('relay') : t('official');
}

function getUsageSourceDetail(snapshot) {
  const routeLabel = getUsageRouteLabel();
  const source = snapshot?.source_label || 'Local Codex session logs';
  if (currentRelayStatus?.route === 'relay') {
    const baseUrl = currentRelayStatus.baseUrl || currentRelayStatus.base_url || t('relayConfiguredFallback');
    return t('relayTokenLogs', routeLabel, baseUrl, source);
  }
  return t('tokenLogs', routeLabel, source);
}

function renderSummaryTokenLabel() {
  const mode = getSummaryMode();
  const routeLabel = getUsageRouteLabel();
  summaryTokensLabel.textContent = mode === 'last' ? t('responseTokens', routeLabel) : t('sessionTokens', routeLabel);
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

function requestInitialSnapshot() {
  if (initialSnapshotRequested) {
    return;
  }
  initialSnapshotRequested = true;
  updatedAt.textContent = t('readingLogs');
  setStatus('loading', t('reading'), getConfiguredLogDir() || t('scanningLogs'));
  loadSnapshot();
}

function getRelayFormSettings() {
  return {
    enabled: relayEnabledToggle.checked,
    providerId: relayProviderIdInput.value.trim() || 'moapi',
    routeMode: relayRouteModeSelect.value || 'direct',
    baseUrl: relayBaseUrlInput.value.trim(),
    apiKey: relayApiKeyInput.value.trim(),
    testModel: relayTestModelInput.value.trim() || DEFAULT_CODEX_MODEL,
    localPort: 15721,
    upstreamModel: relayUpstreamModelInput.value.trim() || null,
    upstreamWireApi: relayRouteModeSelect.value === 'local_router' ? 'chat_completions' : 'responses'
  };
}

function setRelayFormSettings(settings) {
  relayEnabledToggle.checked = Boolean(settings?.enabled);
  relayProviderIdInput.value = settings?.providerId || 'moapi';
  relayRouteModeSelect.value = settings?.routeMode || 'direct';
  relayBaseUrlInput.value = settings?.baseUrl || '';
  relayApiKeyInput.value = settings?.apiKey || '';
  relayTestModelInput.value = settings?.testModel || DEFAULT_CODEX_MODEL;
  relayUpstreamModelInput.value = settings?.upstreamModel || '';
  syncRelayEnabledState();
}

function syncRelayEnabledState() {
  const enabled = relayEnabledToggle.checked;
  relayProviderIdInput.disabled = !enabled;
  relayRouteModeSelect.disabled = !enabled;
  relayBaseUrlInput.disabled = !enabled;
  relayApiKeyInput.disabled = !enabled;
  relayTestModelInput.disabled = !enabled;
  relayUpstreamModelInput.disabled = !enabled || relayRouteModeSelect.value !== 'local_router';
  if (!enabled) {
    relayRouteStatus.textContent = t('officialEndpoint');
  } else if (relayRouteModeSelect.value === 'local_router') {
    relayRouteStatus.textContent = `127.0.0.1:15721 responses -> chat_completions -> ${relayBaseUrlInput.value.trim() || t('relayNotConfigured')}`;
  } else {
    relayRouteStatus.textContent = relayBaseUrlInput.value.trim() || t('relayNotConfigured');
  }
}

function setRelayBusy(isBusy) {
  [relayApplyBtn, relayRestartBtn, relayClearBtn, relayTestBtn, pluginUnlockBtn].forEach((button) => {
    if (button) {
      button.disabled = isBusy;
    }
  });
}

function setRelayActionStatus(message, isError = false) {
  relayActionStatus.textContent = message;
  relayActionStatus.classList.toggle('is-error', isError);
}

function formatRelaySelfTest(result) {
  const checks = result?.checks || [];
  const lines = checks.map((check) => {
    const icon = check.ok ? 'OK' : 'FAIL';
    const latency = check.latencyMs == null ? '' : ` ${check.latencyMs}ms`;
    return `${icon} ${check.name}${latency}: ${check.message}`;
  });
  return [
    `${result?.ok ? t('providerTestPassed') : t('providerTestFailed')}: ${result?.providerId || '--'} | ${result?.upstreamWireApi || '--'} | ${result?.upstreamModel || '--'}`,
    ...lines
  ].join('\n');
}

function setHistoryBusy(isBusy) {
  [historyStatusBtn, historySyncBtn].forEach((button) => {
    button.disabled = isBusy;
  });
}

function setHistoryActionStatus(message, isError = false) {
  historyActionStatus.textContent = message;
  historyActionStatus.classList.toggle('is-error', isError);
}

function renderRelayStatus(status) {
  currentRelayStatus = status || null;
  syncRelayFormFromStatus(status);
  relayStatusRoute.textContent = status?.route === 'relay' ? t('relay') : t('official');
  if (status?.routeMode === 'local_router') {
    relayStatusRoute.textContent = t('localRouter');
  }
  relayStatusConfig.textContent = status?.configured ? t('configured') : t('notApplied');
  relayStatusConfig.title = status?.configPath || '';
  relayStatusCodex.textContent = status?.codexRunning ? t('running') : t('notRunning');
  relayRouteStatus.textContent =
    status?.route === 'relay'
      ? status?.routeMode === 'local_router'
        ? `127.0.0.1:15721 responses -> ${status?.upstreamWireApi || 'chat_completions'} -> ${status?.upstreamBaseUrl || t('relayActive')}`
        : status?.baseUrl || t('relayActive')
      : t('officialEndpoint');
  renderSummaryTokenLabel();
  if (currentSnapshot) {
    const routeLabel = getUsageRouteLabel();
    const sourceDetail = getUsageSourceDetail(currentSnapshot);
    planText.textContent = `${routeLabel} | ${t('planPrefix')} ${formatPlanName(currentSnapshot.plan_type)}`;
    sourceText.textContent = t(
      'tokensFromEvents',
      routeLabel,
      snapshotField(currentSnapshot, 'event_count', 'eventCount', 0),
      snapshotField(currentSnapshot, 'scanned_files', 'scannedFiles', 0)
    );
    setStatus('ready', routeLabel, sourceDetail);
  }
}

function syncRelayFormFromStatus(status) {
  if (status?.route !== 'relay') {
    return;
  }

  relayEnabledToggle.checked = true;
  relayProviderIdInput.value = status.providerId || relayProviderIdInput.value || 'moapi';
  relayRouteModeSelect.value = status.routeMode || relayRouteModeSelect.value || 'direct';
  if (status.upstreamBaseUrl) {
    relayBaseUrlInput.value = status.upstreamBaseUrl;
  } else if (status.baseUrl) {
    relayBaseUrlInput.value = status.baseUrl;
  }
  syncRelayEnabledState();
}

function updateRelayLastApply() {
  const raw = localStorage.getItem(RELAY_LAST_APPLY_STORAGE_KEY);
  relayLastApply.textContent = raw ? new Date(Number(raw)).toLocaleString('zh-CN') : '--';
}

function validateRelayForm(settings) {
  if (!settings.enabled) {
    throw new Error(t('relayDisabledError'));
  }
  if (!/^[A-Za-z0-9_-]+$/.test(settings.providerId)) {
    throw new Error(t('invalidProviderIdError'));
  }
  if (!settings.baseUrl) {
    throw new Error(t('emptyBaseUrlError'));
  }
  if (!/^https?:\/\//i.test(settings.baseUrl)) {
    throw new Error(t('invalidBaseUrlError'));
  }
  if (!settings.apiKey) {
    throw new Error(t('emptyApiKeyError'));
  }
}

async function refreshRelayStatus() {
  try {
    const status = await invoke('relay_status');
    renderRelayStatus(status);
    loadRelayProviders();
    loadHistoryStatus();
  } catch (error) {
    setRelayActionStatus(String(error), true);
  }
}

function renderHistoryStatus(status) {
  const provider = status?.currentProvider || '--';
  const pendingRollouts = status?.pendingRolloutFiles ?? 0;
  const pendingRows = status?.pendingSqliteRows ?? 0;
  historySyncProvider.textContent = provider;
  historyProvider.textContent = status?.currentProviderImplicit ? `${provider} (default)` : provider;
  historyRolloutPending.textContent = String(pendingRollouts);
  historySqlitePending.textContent = status?.sqliteError || String(pendingRows);
  historySqlitePending.title = status?.sqliteError || '';
  historyBackups.textContent = String(status?.backupCount ?? 0);
  renderHistoryProviderList(status?.providerSummaries || []);
  const warning = status?.encryptedContentWarning;
  setHistoryActionStatus(warning || t('historyStatusLoaded', pendingRollouts, pendingRows), Boolean(warning));
}

function renderHistoryProviderList(summaries) {
  historyProviderList.innerHTML = '';
  if (!summaries.length) {
    const empty = document.createElement('span');
    empty.className = 'history-provider-empty';
    empty.textContent = t('noProviderHistory');
    historyProviderList.appendChild(empty);
    return;
  }

  summaries.forEach((summary) => {
    const provider = summary.provider || '';
    const row = document.createElement('div');
    row.className = 'history-provider-row';
    row.classList.toggle('is-current', Boolean(summary.isCurrent));

    const name = document.createElement('strong');
    name.textContent = provider || '(missing)';

    const counts = document.createElement('span');
    const totalFiles = summary.totalRollout || 0;
    const totalRows = summary.totalSqlite || 0;
    counts.textContent = t('historyProviderCounts', Math.max(totalFiles, totalRows), totalFiles, totalRows);
    counts.title = `sessions ${summary.rolloutSessions || 0}, archived ${summary.rolloutArchivedSessions || 0}, DB active ${summary.sqliteSessions || 0}, DB archived ${summary.sqliteArchivedSessions || 0}`;

    row.appendChild(name);
    if (summary.isCurrent) {
      const badge = document.createElement('em');
      badge.textContent = t('currentProviderBadge');
      row.appendChild(badge);
    }
    row.appendChild(counts);
    historyProviderList.appendChild(row);
  });
}

function renderRelayProviderList(providers) {
  relayProviderList.innerHTML = '';
  if (!providers.length) {
    const empty = document.createElement('span');
    empty.className = 'provider-chip-empty';
    empty.textContent = t('noProviderHistory');
    relayProviderList.appendChild(empty);
    return;
  }

  providers.forEach((provider) => {
    const providerId = provider.providerId || provider.provider_id || '';
    if (!providerId) {
      return;
    }
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'provider-chip';
    button.classList.toggle('is-current', Boolean(provider.isCurrent || provider.is_current));
    button.classList.toggle('is-selected', providerId === selectedRelayProvider);
    button.textContent = providerId;
    button.addEventListener('click', () => {
      selectRelayProvider(providerId);
    });
    relayProviderList.appendChild(button);
  });
}

async function loadRelayProviders() {
  try {
    const providers = await invoke('list_relay_providers');
    renderRelayProviderList(providers || []);
  } catch (error) {
    relayProviderList.innerHTML = '';
    const empty = document.createElement('span');
    empty.className = 'provider-chip-empty';
    empty.textContent = String(error);
    relayProviderList.appendChild(empty);
  }
}

async function selectRelayProvider(provider) {
  const normalizedProvider = (provider || '').trim();
  if (!normalizedProvider || normalizedProvider === '(missing)') {
    return;
  }

  selectedRelayProvider = normalizedProvider;

  if (normalizedProvider.toLowerCase() === 'openai') {
    relayEnabledToggle.checked = false;
    relayProviderIdInput.value = 'openai';
    relayRouteModeSelect.value = 'direct';
    syncRelayEnabledState();
    setRelayActionStatus(t('officialProviderSelected'));
  } else {
    try {
      const settings = await invoke('load_relay_provider_settings', { providerId: normalizedProvider });
      setRelayFormSettings(settings);
    } catch {
      relayEnabledToggle.checked = true;
      relayProviderIdInput.value = normalizedProvider;
      if (!relayTestModelInput.value.trim()) {
        relayTestModelInput.value = DEFAULT_CODEX_MODEL;
      }
      if (relayRouteModeSelect.value === 'local_router' && !relayUpstreamModelInput.value.trim()) {
        relayUpstreamModelInput.value = normalizedProvider.toLowerCase() === 'deepseek' ? 'deepseek-chat' : normalizedProvider;
      }
      syncRelayEnabledState();
    }
    setRelayActionStatus(t('providerSelected', normalizedProvider));
  }

  Array.from(relayProviderList.querySelectorAll('.provider-chip')).forEach((row) => {
    const rowProvider = row.textContent || '';
    row.classList.toggle('is-selected', rowProvider === normalizedProvider);
  });
}

async function loadHistoryStatus() {
  setHistoryBusy(true);
  setHistoryActionStatus(t('loadingHistoryStatus'));
  try {
    const status = await invoke('history_sync_status');
    renderHistoryStatus(status);
  } catch (error) {
    setHistoryActionStatus(String(error), true);
  } finally {
    setHistoryBusy(false);
  }
}

async function syncHistoryToCurrentProvider() {
  setHistoryBusy(true);
  setHistoryActionStatus(t('syncingHistory'));
  try {
    const result = await invoke('sync_history_to_provider', { provider: null });
    setHistoryActionStatus(
      result.encryptedContentWarning || t('historySynced', result.changedSessionFiles, result.sqliteRowsUpdated),
      Boolean(result.encryptedContentWarning)
    );
    await loadHistoryStatus();
  } catch (error) {
    setHistoryActionStatus(String(error), true);
  } finally {
    setHistoryBusy(false);
  }
}

function contextEntriesForKind(kind = currentContextKind) {
  if (!currentContextEntries) {
    return [];
  }
  if (kind === 'mcp') {
    return currentContextEntries.mcpServers || currentContextEntries.mcp_servers || [];
  }
  if (kind === 'skill') {
    return currentContextEntries.skills || [];
  }
  return currentContextEntries.plugins || [];
}

function allContextEntryCount(entries = currentContextEntries) {
  if (!entries) {
    return 0;
  }
  return [
    ...(entries.mcpServers || entries.mcp_servers || []),
    ...(entries.skills || []),
    ...(entries.plugins || [])
  ].length;
}

function setContextBusy(busy) {
  [
    contextRefreshBtn,
    contextNewBtn,
    contextEntryIdInput,
    contextEntryBodyInput,
    contextDeleteBtn,
    contextSaveBtn,
    ...contextTabs
  ].forEach((element) => {
    if (element) {
      element.disabled = busy;
    }
  });
}

function setContextActionStatus(message, isError = false) {
  if (!contextActionStatus) {
    return;
  }
  contextActionStatus.textContent = message || t('ready');
  contextActionStatus.classList.toggle('is-error', Boolean(isError));
}

function selectContextKind(kind) {
  if (!contextEntryList) {
    return;
  }
  currentContextKind = kind;
  selectedContextEntry = null;
  contextTabs.forEach((button) => {
    button.classList.toggle('is-active', button.dataset.contextKind === kind);
  });
  clearContextEditor();
  renderContextEntries();
}

function clearContextEditor() {
  if (!contextEditorTitle || !contextEntryIdInput || !contextEntryBodyInput || !contextDeleteBtn) {
    return;
  }
  selectedContextEntry = null;
  contextEditorTitle.textContent = t('newContextEntry');
  contextEntryIdInput.value = '';
  contextEntryIdInput.disabled = false;
  contextEntryBodyInput.value = '';
  contextDeleteBtn.disabled = true;
}

function editContextEntry(entry) {
  if (!contextEditorTitle || !contextEntryIdInput || !contextEntryBodyInput || !contextDeleteBtn) {
    return;
  }
  selectedContextEntry = entry;
  contextEditorTitle.textContent = t('editContextEntry', entry.id);
  contextEntryIdInput.value = entry.id;
  contextEntryIdInput.disabled = true;
  contextEntryBodyInput.value = entry.tomlBody || entry.toml_body || '';
  contextDeleteBtn.disabled = false;
  renderContextEntries();
}

function renderContextEntries() {
  if (!contextEntryList) {
    return;
  }
  contextEntryList.innerHTML = '';
  const entries = contextEntriesForKind();
  if (!entries.length) {
    const empty = document.createElement('div');
    empty.className = 'context-entry-empty';
    empty.textContent = t('noContextEntries');
    contextEntryList.appendChild(empty);
    return;
  }

  entries.forEach((entry) => {
    const row = document.createElement('div');
    row.className = 'context-entry-row';
    row.classList.toggle('is-selected', selectedContextEntry?.id === entry.id);

    const detail = document.createElement('div');
    const title = document.createElement('strong');
    title.textContent = entry.id;
    const summary = document.createElement('span');
    summary.textContent = entry.summary || (entry.enabled ? 'enabled' : 'disabled');
    detail.appendChild(title);
    detail.appendChild(summary);

    const actions = document.createElement('div');
    actions.className = 'context-entry-actions';

    const toggle = document.createElement('label');
    toggle.className = 'switch';
    const input = document.createElement('input');
    input.type = 'checkbox';
    input.checked = Boolean(entry.enabled);
    input.addEventListener('change', () => {
      toggleContextEntry(entry, input.checked);
    });
    const track = document.createElement('span');
    toggle.appendChild(input);
    toggle.appendChild(track);

    const edit = document.createElement('button');
    edit.className = 'mini-btn context-edit-btn';
    edit.type = 'button';
    edit.textContent = t('editEntry');
    edit.addEventListener('click', () => editContextEntry(entry));

    actions.appendChild(toggle);
    actions.appendChild(edit);
    row.appendChild(detail);
    row.appendChild(actions);
    contextEntryList.appendChild(row);
  });
}

function applyContextEntries(entries) {
  currentContextEntries = entries;
  if (contextConfigPath) {
    contextConfigPath.textContent = entries?.configPath || entries?.config_path || '~/.codex/config.toml';
    contextConfigPath.title = contextConfigPath.textContent;
  }
  renderContextEntries();
}

async function loadContextEntries() {
  setContextBusy(true);
  setContextActionStatus(t('loadingContextEntries'));
  try {
    const entries = await invoke('list_context_entries');
    applyContextEntries(entries);
    setContextActionStatus(t('contextEntriesLoaded', allContextEntryCount(entries)));
  } catch (error) {
    setContextActionStatus(String(error), true);
  } finally {
    setContextBusy(false);
    contextDeleteBtn.disabled = !selectedContextEntry;
  }
}

async function saveContextEntry() {
  const id = contextEntryIdInput.value.trim();
  const tomlBody = contextEntryBodyInput.value.trim();
  if (!id) {
    setContextActionStatus(t('contextEntryIdRequired'), true);
    return;
  }
  if (!tomlBody) {
    setContextActionStatus(t('contextBodyRequired'), true);
    return;
  }
  setContextBusy(true);
  setContextActionStatus(t('savingContextEntry'));
  try {
    const result = await invoke('upsert_context_entry', {
      input: { kind: currentContextKind, id, tomlBody }
    });
    applyContextEntries(result.entries);
    const entry = contextEntriesForKind().find((item) => item.id === id);
    if (entry) {
      editContextEntry(entry);
    }
    setContextActionStatus(result.message || t('ready'));
  } catch (error) {
    setContextActionStatus(String(error), true);
  } finally {
    setContextBusy(false);
    contextDeleteBtn.disabled = !selectedContextEntry;
  }
}

async function toggleContextEntry(entry, enabled) {
  setContextBusy(true);
  setContextActionStatus(t('togglingContextEntry'));
  try {
    const result = await invoke('toggle_context_entry', {
      input: { kind: currentContextKind, id: entry.id, enabled }
    });
    applyContextEntries(result.entries);
    setContextActionStatus(result.message || t('ready'));
  } catch (error) {
    setContextActionStatus(String(error), true);
    await loadContextEntries();
  } finally {
    setContextBusy(false);
    contextDeleteBtn.disabled = !selectedContextEntry;
  }
}

async function deleteSelectedContextEntry() {
  if (!selectedContextEntry) {
    return;
  }
  setContextBusy(true);
  setContextActionStatus(t('deletingContextEntry'));
  try {
    const result = await invoke('delete_context_entry', {
      input: { kind: currentContextKind, id: selectedContextEntry.id }
    });
    applyContextEntries(result.entries);
    clearContextEditor();
    setContextActionStatus(result.message || t('ready'));
  } catch (error) {
    setContextActionStatus(String(error), true);
  } finally {
    setContextBusy(false);
    contextDeleteBtn.disabled = !selectedContextEntry;
  }
}

async function loadRelaySettings() {
  try {
    const settings = await invoke('load_relay_settings');
    setRelayFormSettings(settings);
    await refreshRelayStatus();
    updateRelayLastApply();
  } catch (error) {
    setRelayActionStatus(String(error), true);
  }
}

async function saveRelaySettings() {
  const settings = getRelayFormSettings();
  setRelayBusy(true);
  setRelayActionStatus(t('saving'));
  try {
    const saved = await invoke('save_relay_settings', { settings });
    setRelayFormSettings(saved);
    setRelayActionStatus(t('relaySaved'));
    await refreshRelayStatus();
  } catch (error) {
    setRelayActionStatus(String(error), true);
  } finally {
    setRelayBusy(false);
  }
}

async function applyRelayConfig() {
  const settings = getRelayFormSettings();
  setRelayBusy(true);
  setRelayActionStatus(t('applyingRelay'));
  try {
    if (!settings.enabled) {
      const result = await invoke('clear_relay_config');
      setRelayActionStatus(result.message || 'Official endpoint restored.');
      await refreshRelayStatus();
      return;
    }
    validateRelayForm(settings);
    const saved = await invoke('save_relay_settings', { settings });
    setRelayFormSettings(saved);
    const result = await invoke('apply_relay_config', { settings: saved });
    localStorage.setItem(RELAY_LAST_APPLY_STORAGE_KEY, String(Date.now()));
    updateRelayLastApply();
    setRelayActionStatus(result.message || 'Relay configuration applied.');
    await refreshRelayStatus();
  } catch (error) {
    setRelayActionStatus(String(error), true);
  } finally {
    setRelayBusy(false);
  }
}

async function applyRelayConfigAndRestart() {
  const settings = getRelayFormSettings();
  setRelayBusy(true);
  setRelayActionStatus(t('applyingRestart'));
  try {
    if (!settings.enabled) {
      const clearResult = await invoke('clear_relay_config');
      const restartResult = await invoke('restart_codex_app');
      setRelayActionStatus(`${clearResult.message} ${restartResult.message}`);
      await loadRelaySettings();
      return;
    }
    validateRelayForm(settings);
    const result = await invoke('apply_relay_config_and_restart', { settings });
    localStorage.setItem(RELAY_LAST_APPLY_STORAGE_KEY, String(Date.now()));
    updateRelayLastApply();
    setRelayActionStatus(`${result.apply.message} ${result.restart.message}`);
    await loadRelaySettings();
  } catch (error) {
    setRelayActionStatus(String(error), true);
  } finally {
    setRelayBusy(false);
  }
}

async function clearRelayConfig() {
  setRelayBusy(true);
  setRelayActionStatus(t('restoringOfficial'));
  try {
    const result = await invoke('clear_relay_config');
    setRelayActionStatus(result.message || 'Official endpoint restored.');
    await refreshRelayStatus();
  } catch (error) {
    setRelayActionStatus(String(error), true);
  } finally {
    setRelayBusy(false);
  }
}

async function testRelayProvider() {
  const settings = getRelayFormSettings();
  setRelayBusy(true);
  setRelayActionStatus(t('testingProvider'));
  try {
    validateRelayForm(settings);
    const result = await invoke('test_relay_provider', { settings });
    setRelayActionStatus(formatRelaySelfTest(result), !result.ok);
  } catch (error) {
    setRelayActionStatus(String(error), true);
  } finally {
    setRelayBusy(false);
  }
}

async function unlockCodexPlugins() {
  setRelayBusy(true);
  setRelayActionStatus(t('unlockingPlugins'));
  try {
    const result = await invoke('unlock_codex_plugins', {
      request: { debugPort: 9229, restartCodex: true }
    });
    setRelayActionStatus(result.message || t('pluginsUnlocked'));
    await refreshRelayStatus();
  } catch (error) {
    setRelayActionStatus(String(error), true);
  } finally {
    setRelayBusy(false);
  }
}

function isPrivacyModeEnabled() {
  return localStorage.getItem(PRIVACY_MODE_STORAGE_KEY) !== '0';
}

function maskPath(path) {
  if (!path) {
    return t('localLogs');
  }

  const segments = String(path).split(/[\\/]+/).filter(Boolean);
  if (segments.length <= 2) {
    return t('hiddenPath');
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
  privacyToggle.setAttribute('aria-label', privacyEnabled ? t('showFullPath') : t('hideFullPath'));
  privacyToggle.title = privacyEnabled ? t('showFullPath') : t('hideFullPath');
}

function applyPrivacyMode(enabled) {
  localStorage.setItem(PRIVACY_MODE_STORAGE_KEY, enabled ? '1' : '0');
  renderSourcePath();
}

function setStatus(mode, label, detail) {
  statusBanner.classList.toggle('is-loading', mode === 'loading');
  statusBanner.classList.toggle('is-error', mode === 'error');
  statusBadge.textContent = label;
  sourcePathRaw = detail || t('localLogs');
  renderSourcePath();
}

function resetSnapshotView() {
  primaryPercent.textContent = '--';
  secondaryPercent.textContent = '--';
  primaryMeta.textContent = `${t('resets')} --`;
  secondaryMeta.textContent = `${t('resets')} --`;
  primaryMeterFill.style.width = '0%';
  secondaryMeterFill.style.width = '0%';
  primaryRemaining.textContent = `${t('remaining')} --`;
  secondaryRemaining.textContent = `${t('remaining')} --`;
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
  providerTokenList.innerHTML = '';
  totalWindowLabel.textContent = t('currentSessionTotal');
  lastWindowLabel.textContent = t('mostRecentResponse');
  syncTime.textContent = '--';
  dailyBars.innerHTML = '';
  weeklyPath.setAttribute('d', '');
  weeklyShadow.setAttribute('d', '');
}

function firstDefined(...values) {
  return values.find((value) => value !== undefined && value !== null);
}

function tokenField(usage, snakeName, camelName) {
  return usage ? firstDefined(usage[snakeName], usage[camelName]) : undefined;
}

function tokenTotal(usage) {
  return tokenField(usage, 'total_tokens', 'totalTokens');
}

function snapshotField(snapshot, snakeName, camelName, fallback) {
  return firstDefined(snapshot?.[snakeName], snapshot?.[camelName], fallback);
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
    sourceText.textContent = `${t('sourcePrefix')}: ${t('localLogs')} | autostart unavailable`;
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
  currentSnapshot = snapshot;
  const latestAt = new Date(snapshotField(snapshot, 'last_event_at', 'lastEventAt', 0) * 1000);
  const usageMode = snapshotField(snapshot, 'usage_mode', 'usageMode', 'rate_limit');
  const isTokenUsageMode = usageMode === 'token_usage';
  const hourlyTokenBuckets = snapshotField(snapshot, 'hourly_token_buckets', 'hourlyTokenBuckets', []);
  const weeklyTokenBuckets = snapshotField(snapshot, 'weekly_token_buckets', 'weeklyTokenBuckets', []);
  const weeklyD = isTokenUsageMode
    ? buildTokenWeeklyPath(weeklyTokenBuckets)
    : buildWeeklyPath(snapshotField(snapshot, 'weekly_secondary_percents', 'weeklySecondaryPercents', []));
  const totalUsage = snapshotField(snapshot, 'total_usage', 'totalUsage', null);
  const lastUsage = snapshotField(snapshot, 'last_usage', 'lastUsage', null);
  const totalTokenValue = tokenTotal(totalUsage) ?? tokenTotal(lastUsage);
  const lastTokenValue = tokenTotal(lastUsage);
  const summaryMode = getSummaryMode();
  const mainTokenValue = summaryMode === 'last' ? lastTokenValue : totalTokenValue;
  const primaryUsed = clampPercent(snapshot.primary.used_percent);
  const secondaryUsed = clampPercent(snapshot.secondary.used_percent);
  const primaryRemainingPercent = Math.max(0, 100 - primaryUsed);
  const secondaryRemainingPercent = Math.max(0, 100 - secondaryUsed);
  const refreshLabel = formatTime(new Date());
  const eventLabel = formatRelativeTime(latestAt);

  updatedAt.textContent = t('lastRefresh', refreshLabel, eventLabel);
  const routeLabel = getUsageRouteLabel();
  const sourceDetail = getUsageSourceDetail(snapshot);
  planText.textContent = `${routeLabel} | ${t('planPrefix')} ${formatPlanName(snapshot.plan_type)}`;
  sourceText.textContent = t('tokensFromEvents', routeLabel, snapshotField(snapshot, 'event_count', 'eventCount', 0), snapshotField(snapshot, 'scanned_files', 'scannedFiles', 0));
  setStatus('ready', routeLabel, sourceDetail);

  if (isTokenUsageMode) {
    const token24hTotal = snapshotField(snapshot, 'token_24h_total', 'token24hTotal', 0);
    const token24hEvents = snapshotField(snapshot, 'token_24h_events', 'token24hEvents', 0);
    const tokenCurrentHourTotal = snapshotField(snapshot, 'token_current_hour_total', 'tokenCurrentHourTotal', 0);
    const tokenPeakHourTotal = snapshotField(snapshot, 'token_peak_hour_total', 'tokenPeakHourTotal', 0);
    const weeklyTokenTotal = weeklyTokenBuckets.reduce((sum, bucket) => sum + bucketTotal(bucket), 0);
    const peakPercent = tokenPeakHourTotal > 0 ? Math.round((tokenCurrentHourTotal / tokenPeakHourTotal) * 100) : 0;
    const weekPeak = Math.max(...weeklyTokenBuckets.map(bucketTotal), 1);
    const weekPercent = Math.round((weeklyTokenBuckets.at(-1) ? bucketTotal(weeklyTokenBuckets.at(-1)) : 0) / weekPeak * 100);

    if (primaryStatLabel) {
      primaryStatLabel.textContent = '24h Tokens';
    }
    if (secondaryStatLabel) {
      secondaryStatLabel.textContent = '7d Tokens';
    }
    primaryPercent.textContent = formatCompactNumber(token24hTotal);
    secondaryPercent.textContent = formatCompactNumber(weeklyTokenTotal);
    primaryMeta.textContent = `24h | ${formatNumber(token24hEvents)} events`;
    secondaryMeta.textContent = `7d | peak ${formatCompactNumber(weekPeak)}`;
    primaryMeterFill.style.width = `${Math.max(4, Math.min(100, peakPercent))}%`;
    secondaryMeterFill.style.width = `${Math.max(4, Math.min(100, weekPercent))}%`;
    primaryRemaining.textContent = `Current hour ${formatCompactNumber(tokenCurrentHourTotal)} tokens`;
    secondaryRemaining.textContent = `Peak hour ${formatCompactNumber(tokenPeakHourTotal)} tokens`;
    primaryReset.textContent = `24h tokens ${formatNumber(token24hTotal)}`;
    secondaryReset.textContent = `7d tokens ${formatNumber(weeklyTokenTotal)}`;
  } else {
    if (primaryStatLabel) {
      primaryStatLabel.textContent = t('fiveHourWindow');
    }
    if (secondaryStatLabel) {
      secondaryStatLabel.textContent = t('weeklyWindow');
    }
    primaryPercent.textContent = `${primaryUsed}%`;
    secondaryPercent.textContent = `${secondaryUsed}%`;
    primaryMeta.textContent = `${t('used')} ${primaryUsed}% | ${t('resets')} ${formatReset(snapshot.primary.resets_at)}`;
    secondaryMeta.textContent = `${t('used')} ${secondaryUsed}% | ${t('resets')} ${formatReset(snapshot.secondary.resets_at)}`;
    primaryMeterFill.style.width = `${primaryUsed}%`;
    secondaryMeterFill.style.width = `${secondaryUsed}%`;
    primaryRemaining.textContent = t('remainingWindow', primaryRemainingPercent, snapshot.primary.window_minutes / 60);
    secondaryRemaining.textContent = t('remainingWeek', secondaryRemainingPercent);
    primaryReset.textContent = `5 hour window ${snapshot.primary.window_minutes / 60}h`;
    secondaryReset.textContent = formatReset(snapshot.secondary.resets_at);
  }

  totalTokens.textContent = formatCompactNumber(mainTokenValue);
  totalTokens.title = formatNumber(mainTokenValue);
  lastTokens.textContent = formatCompactNumber(lastTokenValue);
  lastTokens.title = formatNumber(lastTokenValue);
  contextWindow.textContent = formatCompactNumber(snapshot.model_context_window);
  scannedFiles.textContent = formatNumber(snapshotField(snapshot, 'scanned_files', 'scannedFiles'));
  totalInput.textContent = formatNumber(tokenField(totalUsage, 'input_tokens', 'inputTokens'));
  totalOutput.textContent = formatNumber(tokenField(totalUsage, 'output_tokens', 'outputTokens'));
  totalCached.textContent = formatNumber(tokenField(totalUsage, 'cached_input_tokens', 'cachedInputTokens'));
  totalReasoning.textContent = formatNumber(tokenField(totalUsage, 'reasoning_output_tokens', 'reasoningOutputTokens'));
  lastInput.textContent = formatNumber(tokenField(lastUsage, 'input_tokens', 'inputTokens'));
  lastOutput.textContent = formatNumber(tokenField(lastUsage, 'output_tokens', 'outputTokens'));
  lastCached.textContent = formatNumber(tokenField(lastUsage, 'cached_input_tokens', 'cachedInputTokens'));
  lastReasoning.textContent = formatNumber(tokenField(lastUsage, 'reasoning_output_tokens', 'reasoningOutputTokens'));
  totalWindowLabel.textContent = totalUsage ? `${t('total')} ${formatNumber(tokenTotal(totalUsage))}` : t('currentSessionTotal');
  lastWindowLabel.textContent = lastUsage ? `${t('total')} ${formatNumber(tokenTotal(lastUsage))}` : t('mostRecentResponse');
  renderProviderTokenSummaries(snapshotField(snapshot, 'provider_token_summaries', 'providerTokenSummaries', []));
  syncTime.textContent = `${latestAt.toLocaleString(currentLanguage === 'zh' ? 'zh-CN' : 'en-US', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  })} (${eventLabel})`;

  renderBars(
    isTokenUsageMode
      ? hourlyTokenBuckets
      : snapshotField(snapshot, 'hourly_primary_percents', 'hourlyPrimaryPercents', []),
    { mode: isTokenUsageMode ? 'tokens' : 'percent' }
  );
  weeklyPath.setAttribute('d', weeklyD);
  weeklyShadow.setAttribute('d', weeklyD);

  updatedAt.title = `${t('sourcePrefix')} ${sourceDetail}\n${t('lastUsageEvent')} ${latestAt.toLocaleString(currentLanguage === 'zh' ? 'zh-CN' : 'en-US')}`;
}

function renderProviderTokenSummaries(summaries) {
  if (!providerTokenList) {
    return;
  }
  providerTokenList.innerHTML = '';
  if (!summaries.length) {
    const empty = document.createElement('span');
    empty.className = 'provider-token-empty';
    empty.textContent = t('noProviderTokens');
    providerTokenList.appendChild(empty);
    return;
  }

  summaries.forEach((summary) => {
    const row = document.createElement('div');
    row.className = 'provider-token-row';

    const name = document.createElement('strong');
    name.textContent = summary.provider || 'unknown';

    const total = document.createElement('span');
    total.textContent = t(
      'providerTokenLine',
      formatNumber(firstDefined(summary.total_tokens, summary.totalTokens, 0)),
      firstDefined(summary.event_count, summary.eventCount, 0)
    );

    const parts = document.createElement('small');
    parts.textContent = `in ${formatNumber(firstDefined(summary.input_tokens, summary.inputTokens, 0))} | out ${formatNumber(firstDefined(summary.output_tokens, summary.outputTokens, 0))} | cached ${formatNumber(firstDefined(summary.cached_input_tokens, summary.cachedInputTokens, 0))} | reasoning ${formatNumber(firstDefined(summary.reasoning_output_tokens, summary.reasoningOutputTokens, 0))}`;

    row.appendChild(name);
    row.appendChild(total);
    row.appendChild(parts);
    providerTokenList.appendChild(row);
  });
}

async function loadSnapshot() {
  try {
    refreshBtn.disabled = true;
    setStatus('loading', t('reading'), getConfiguredLogDir() || t('scanningLogs'));
    updatedAt.textContent = t('readingLogs');
    const sessionsDir = getConfiguredLogDir();
    const snapshot = await invoke('load_usage_snapshot', {
      sessionsDir: sessionsDir || null
    });
    renderSnapshot(snapshot);
    await refreshRelayStatus();
  } catch (error) {
    resetSnapshotView();
    updatedAt.textContent = t('readFailed');
    sourceText.textContent = `${t('readFailed')}: ${String(error)}`;
    planText.textContent = `${t('planPrefix')} --`;
    setStatus('error', t('readFailed'), String(error));
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
window.setTimeout(requestInitialSnapshot, 0);

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

languageButtons.forEach((button) => {
  button.addEventListener('click', () => {
    applyLanguage(button.dataset.language);
    loadSnapshot();
  });
});

relayEnabledToggle.addEventListener('change', syncRelayEnabledState);

relayProviderIdInput.addEventListener('input', syncRelayEnabledState);

relayRouteModeSelect.addEventListener('change', () => {
  if (relayRouteModeSelect.value === 'local_router') {
    if (!relayBaseUrlInput.value.trim()) {
      relayBaseUrlInput.value = 'https://api.deepseek.com/v1';
    }
    if (!relayUpstreamModelInput.value.trim()) {
      relayUpstreamModelInput.value = 'deepseek-chat';
    }
    if (!relayTestModelInput.value.trim()) {
      relayTestModelInput.value = DEFAULT_CODEX_MODEL;
    }
    if (!relayProviderIdInput.value.trim() || relayProviderIdInput.value.trim() === 'moapi') {
      relayProviderIdInput.value = 'deepseek';
    }
  }
  syncRelayEnabledState();
});

relayBaseUrlInput.addEventListener('input', syncRelayEnabledState);

relayKeyVisibility.addEventListener('click', () => {
  const isPassword = relayApiKeyInput.type === 'password';
  relayApiKeyInput.type = isPassword ? 'text' : 'password';
  relayKeyVisibility.textContent = isPassword ? t('hide') : t('show');
});

relayApplyBtn.addEventListener('click', () => {
  applyRelayConfig();
});

relayRestartBtn.addEventListener('click', () => {
  applyRelayConfigAndRestart();
});

relayClearBtn.addEventListener('click', () => {
  clearRelayConfig();
});

relayTestBtn?.addEventListener('click', () => {
  testRelayProvider();
});

pluginUnlockBtn?.addEventListener('click', () => {
  unlockCodexPlugins();
});

historyStatusBtn.addEventListener('click', () => {
  loadHistoryStatus();
});

historySyncBtn.addEventListener('click', () => {
  syncHistoryToCurrentProvider();
});

contextRefreshBtn?.addEventListener('click', () => {
  loadContextEntries();
});

contextTabs.forEach((button) => {
  button.addEventListener('click', () => {
    selectContextKind(button.dataset.contextKind);
  });
});

contextNewBtn?.addEventListener('click', () => {
  clearContextEditor();
});

contextSaveBtn?.addEventListener('click', () => {
  saveContextEntry();
});

contextDeleteBtn?.addEventListener('click', () => {
  deleteSelectedContextEntry();
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
  if (isExpanded === false) {
    window.setTimeout(() => {
      tokenBreakdown.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }, 40);
  }
  syncWindowHeight(!isExpanded).catch(console.error);
});

quitBtn.addEventListener('click', async () => {
  await invoke('quit_app');
});

if (typeof appWindow.onMoved === 'function') {
  appWindow.onMoved(() => {
    window.setTimeout(() => {
      maybeSnapToEdges().catch(console.error);
    }, 60);
  }).catch(console.error);
}

applyLanguage(getInitialLanguage());
applyTheme(localStorage.getItem(THEME_STORAGE_KEY) === '1');
applySnapPreference(localStorage.getItem(SNAP_STORAGE_KEY) !== '0');
applyLogDir(getConfiguredLogDir());
applySummaryMode(getSummaryMode());
applyRefreshInterval(getRefreshIntervalMs());
applyPrivacyMode(isPrivacyModeEnabled());
const detailsExpanded = localStorage.getItem(DETAILS_STORAGE_KEY) === '1';
applyDetailsPreference(detailsExpanded);
window.setTimeout(requestInitialSnapshot, 0);
loadAppVersion();
syncWindowHeight(detailsExpanded).catch(console.error);
loadRelaySettings();
if (contextEntryList) {
  selectContextKind('plugin');
  loadContextEntries();
}
syncAutostartState();
