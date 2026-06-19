use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;

mod context_config;
mod history_sync;
mod local_router;
mod plugin_unlock;
mod relay_config;

#[derive(Clone, Serialize)]
struct TokenUsage {
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
}

impl TokenUsage {
    fn add_assign(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_output_tokens += other.reasoning_output_tokens;
        self.total_tokens += other.total_tokens;
    }
}

#[derive(Clone, Serialize)]
struct ProviderTokenSummary {
    provider: String,
    event_count: usize,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
    last_event_at: Option<i64>,
}

#[derive(Clone, Default, Serialize)]
struct TokenBucket {
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
    event_count: usize,
}

impl TokenBucket {
    fn add_usage(&mut self, usage: &TokenUsage) {
        self.input_tokens += usage.input_tokens;
        self.cached_input_tokens += usage.cached_input_tokens;
        self.output_tokens += usage.output_tokens;
        self.reasoning_output_tokens += usage.reasoning_output_tokens;
        self.total_tokens += usage.total_tokens;
        self.event_count += 1;
    }
}

#[derive(Clone, Serialize)]
struct RateLimitState {
    used_percent: f64,
    window_minutes: i64,
    resets_at: i64,
}

#[derive(Clone, Serialize)]
struct UsageSnapshot {
    generated_at: i64,
    last_event_at: i64,
    plan_type: String,
    source_label: String,
    primary: RateLimitState,
    secondary: RateLimitState,
    total_usage: Option<TokenUsage>,
    last_usage: Option<TokenUsage>,
    hourly_primary_percents: Vec<f64>,
    weekly_secondary_percents: Vec<f64>,
    hourly_token_buckets: Vec<TokenBucket>,
    weekly_token_buckets: Vec<TokenBucket>,
    usage_mode: String,
    token_24h_total: u64,
    token_24h_events: usize,
    token_current_hour_total: u64,
    token_peak_hour_total: u64,
    event_count: usize,
    scanned_files: usize,
    model_context_window: Option<u64>,
    provider_token_summaries: Vec<ProviderTokenSummary>,
}

#[derive(Clone)]
struct UsageEvent {
    timestamp: DateTime<Utc>,
    provider: String,
    plan_type: String,
    primary: RateLimitState,
    secondary: RateLimitState,
    total_usage: Option<TokenUsage>,
    last_usage: Option<TokenUsage>,
    model_context_window: Option<u64>,
}

fn parse_usage(value: &Value) -> Option<TokenUsage> {
    Some(TokenUsage {
        input_tokens: value.get("input_tokens")?.as_u64()?,
        cached_input_tokens: value.get("cached_input_tokens")?.as_u64()?,
        output_tokens: value.get("output_tokens")?.as_u64()?,
        reasoning_output_tokens: value.get("reasoning_output_tokens")?.as_u64()?,
        total_tokens: value.get("total_tokens")?.as_u64()?,
    })
}

fn parse_rate_limit(value: &Value) -> Option<RateLimitState> {
    Some(RateLimitState {
        used_percent: value.get("used_percent")?.as_f64()?,
        window_minutes: value.get("window_minutes")?.as_i64()?,
        resets_at: value.get("resets_at")?.as_i64()?,
    })
}

fn empty_rate_limit() -> RateLimitState {
    RateLimitState {
        used_percent: 0.0,
        window_minutes: 0,
        resets_at: 0,
    }
}

fn parse_event_with_provider(line: &str, provider: &str) -> Option<UsageEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    let timestamp = value.get("timestamp")?.as_str()?;
    let parsed_time = DateTime::parse_from_rfc3339(timestamp).ok()?;
    let payload = value.get("payload")?;

    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }

    let rate_limits = payload.get("rate_limits");
    let info = payload.get("info");
    let total_usage = info
        .and_then(|item| item.get("total_token_usage"))
        .and_then(parse_usage);
    let last_usage = info
        .and_then(|item| item.get("last_token_usage"))
        .and_then(parse_usage);

    if total_usage.is_none() && last_usage.is_none() {
        return None;
    }

    Some(UsageEvent {
        timestamp: parsed_time.with_timezone(&Utc),
        provider: provider.to_string(),
        plan_type: rate_limits
            .and_then(|limits| limits.get("plan_type"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        primary: rate_limits
            .and_then(|limits| limits.get("primary"))
            .and_then(parse_rate_limit)
            .unwrap_or_else(empty_rate_limit),
        secondary: rate_limits
            .and_then(|limits| limits.get("secondary"))
            .and_then(parse_rate_limit)
            .unwrap_or_else(empty_rate_limit),
        total_usage,
        last_usage,
        model_context_window: info
            .and_then(|item| item.get("model_context_window"))
            .and_then(Value::as_u64),
    })
}

fn parse_session_provider(line: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str)? != "session_meta" {
        return None;
    }
    value
        .get("payload")
        .and_then(|payload| payload.get("model_provider"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn visit_jsonl_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|error| error.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();

        if path.is_dir() {
            visit_jsonl_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }

    Ok(())
}

fn resolve_sessions_dir(custom_dir: Option<String>) -> Result<PathBuf, String> {
    match custom_dir {
        Some(dir) if !dir.trim().is_empty() => Ok(PathBuf::from(dir)),
        _ => env::var("USERPROFILE")
            .or_else(|_| env::var("HOME"))
            .map(PathBuf::from)
            .map(|home| home.join(".codex").join("sessions"))
            .map_err(|_| "Unable to locate the user home directory".to_string()),
    }
}

fn infer_codex_home_from_sessions_dir(sessions_dir: &Path) -> Option<PathBuf> {
    if sessions_dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "sessions")
    {
        return sessions_dir.parent().map(Path::to_path_buf);
    }

    None
}

fn parse_quoted_toml_value(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?.trim_start();
    let value = rest.strip_prefix('=')?.trim_start();
    let value = value.strip_prefix('"')?;
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

fn read_configured_providers(codex_home: Option<&Path>) -> Vec<String> {
    let mut providers = vec!["openai".to_string()];
    let Some(codex_home) = codex_home else {
        return providers;
    };
    let contents = fs::read_to_string(codex_home.join("config.toml")).unwrap_or_default();

    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(provider) = parse_quoted_toml_value(trimmed, "model_provider") {
            if !providers.contains(&provider) {
                providers.push(provider);
            }
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("[model_providers.") else {
            continue;
        };
        let Some(provider) = rest.strip_suffix(']') else {
            continue;
        };
        let provider = provider.trim().to_string();
        if !provider.is_empty() && !providers.contains(&provider) {
            providers.push(provider);
        }
    }

    providers
}

fn load_events(custom_dir: Option<String>) -> Result<(Vec<UsageEvent>, usize, PathBuf), String> {
    let sessions_dir = resolve_sessions_dir(custom_dir)?;

    if !sessions_dir.exists() {
        return Err(format!(
            "Could not find Codex session logs at {}",
            sessions_dir.display()
        ));
    }

    let mut files = Vec::new();
    visit_jsonl_files(&sessions_dir, &mut files)?;
    files.sort();

    let mut events = Vec::new();

    for file_path in &files {
        let file = match File::open(file_path) {
            Ok(file) => file,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);

        let mut lines = reader.lines().map_while(Result::ok);
        let provider = lines
            .next()
            .and_then(|line| parse_session_provider(&line))
            .unwrap_or_else(|| "unknown".to_string());

        for line in lines {
            if let Some(event) = parse_event_with_provider(&line, &provider) {
                events.push(event);
            }
        }
    }

    events.sort_by_key(|event| event.timestamp.timestamp());

    Ok((events, files.len(), sessions_dir))
}

fn latest_event_before(events: &[UsageEvent], target: DateTime<Utc>) -> Option<&UsageEvent> {
    events.iter().rev().find(|event| event.timestamp <= target)
}

fn build_hourly_series(events: &[UsageEvent], now: DateTime<Utc>) -> Vec<f64> {
    (0..24)
        .map(|index| {
            let slot_end = now - Duration::hours((23 - index) as i64);
            latest_event_before(events, slot_end)
                .map(|event| event.primary.used_percent)
                .unwrap_or(0.0)
        })
        .collect()
}

fn build_weekly_series(events: &[UsageEvent], now: DateTime<Utc>) -> Vec<f64> {
    let today = now.date_naive();

    (0..7)
        .map(|index| {
            let day = today - Duration::days((6 - index) as i64);
            let day_end = day.and_hms_opt(23, 59, 59).unwrap().and_utc();
            latest_event_before(events, day_end)
                .map(|event| event.secondary.used_percent)
                .unwrap_or(0.0)
        })
        .collect()
}

fn event_token_usage(event: &UsageEvent) -> Option<&TokenUsage> {
    event.last_usage.as_ref().or(event.total_usage.as_ref())
}

fn build_hourly_token_buckets(events: &[UsageEvent], now: DateTime<Utc>) -> Vec<TokenBucket> {
    (0..24)
        .map(|index| {
            let slot_end = now - Duration::hours((23 - index) as i64);
            let slot_start = slot_end - Duration::hours(1);
            let mut bucket = TokenBucket::default();
            for event in events {
                if event.timestamp > slot_start && event.timestamp <= slot_end {
                    if let Some(usage) = event_token_usage(event) {
                        bucket.add_usage(usage);
                    }
                }
            }
            bucket
        })
        .collect()
}

fn build_weekly_token_buckets(events: &[UsageEvent], now: DateTime<Utc>) -> Vec<TokenBucket> {
    let today = now.date_naive();

    (0..7)
        .map(|index| {
            let day = today - Duration::days((6 - index) as i64);
            let day_start = day.and_hms_opt(0, 0, 0).unwrap().and_utc();
            let day_end = day.and_hms_opt(23, 59, 59).unwrap().and_utc();
            let mut bucket = TokenBucket::default();
            for event in events {
                if event.timestamp >= day_start && event.timestamp <= day_end {
                    if let Some(usage) = event_token_usage(event) {
                        bucket.add_usage(usage);
                    }
                }
            }
            bucket
        })
        .collect()
}

fn is_relay_usage_provider(provider: &str) -> bool {
    !provider.eq_ignore_ascii_case("openai") && provider != "unknown"
}

fn build_provider_token_summaries(
    events: &[UsageEvent],
    configured_providers: &[String],
) -> Vec<ProviderTokenSummary> {
    let mut totals: BTreeMap<String, (TokenUsage, usize, Option<i64>)> = BTreeMap::new();
    for provider in configured_providers {
        totals.entry(provider.clone()).or_insert((
            TokenUsage {
                input_tokens: 0,
                cached_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: 0,
            },
            0,
            None,
        ));
    }

    for event in events {
        let usage = event.last_usage.as_ref().or(event.total_usage.as_ref());
        let Some(usage) = usage else {
            continue;
        };
        let entry = totals.entry(event.provider.clone()).or_insert((
            TokenUsage {
                input_tokens: 0,
                cached_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: 0,
            },
            0,
            None,
        ));
        entry.0.add_assign(usage);
        entry.1 += 1;
        entry.2 = Some(entry.2.unwrap_or(i64::MIN).max(event.timestamp.timestamp()));
    }

    let mut summaries: Vec<ProviderTokenSummary> = totals
        .into_iter()
        .map(
            |(provider, (usage, event_count, last_event_at))| ProviderTokenSummary {
                provider,
                event_count,
                input_tokens: usage.input_tokens,
                cached_input_tokens: usage.cached_input_tokens,
                output_tokens: usage.output_tokens,
                reasoning_output_tokens: usage.reasoning_output_tokens,
                total_tokens: usage.total_tokens,
                last_event_at,
            },
        )
        .collect();
    summaries.sort_by(|left, right| {
        right
            .total_tokens
            .cmp(&left.total_tokens)
            .then_with(|| left.provider.cmp(&right.provider))
    });
    summaries
}

fn build_snapshot(custom_dir: Option<String>) -> Result<UsageSnapshot, String> {
    let (events, scanned_files, sessions_dir) = load_events(custom_dir)?;
    let codex_home = infer_codex_home_from_sessions_dir(&sessions_dir);
    let configured_providers = read_configured_providers(codex_home.as_deref());
    let latest = events
        .last()
        .cloned()
        .ok_or_else(|| "No Codex usage events were found".to_string())?;
    let now = Utc::now();
    let usage_mode = if is_relay_usage_provider(&latest.provider) {
        "token_usage"
    } else {
        "rate_limit"
    };
    let hourly_token_buckets = build_hourly_token_buckets(&events, now);
    let weekly_token_buckets = build_weekly_token_buckets(&events, now);
    let token_24h_total = hourly_token_buckets
        .iter()
        .map(|bucket| bucket.total_tokens)
        .sum();
    let token_24h_events = hourly_token_buckets
        .iter()
        .map(|bucket| bucket.event_count)
        .sum();
    let token_current_hour_total = hourly_token_buckets
        .last()
        .map(|bucket| bucket.total_tokens)
        .unwrap_or(0);
    let token_peak_hour_total = hourly_token_buckets
        .iter()
        .map(|bucket| bucket.total_tokens)
        .max()
        .unwrap_or(0);

    Ok(UsageSnapshot {
        generated_at: now.timestamp(),
        last_event_at: latest.timestamp.timestamp(),
        plan_type: latest.plan_type,
        source_label: sessions_dir.display().to_string(),
        primary: latest.primary,
        secondary: latest.secondary,
        total_usage: latest.total_usage,
        last_usage: latest.last_usage,
        hourly_primary_percents: build_hourly_series(&events, now),
        weekly_secondary_percents: build_weekly_series(&events, now),
        hourly_token_buckets,
        weekly_token_buckets,
        usage_mode: usage_mode.to_string(),
        token_24h_total,
        token_24h_events,
        token_current_hour_total,
        token_peak_hour_total,
        event_count: events.len(),
        scanned_files,
        model_context_window: latest.model_context_window,
        provider_token_summaries: build_provider_token_summaries(&events, &configured_providers),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_count_event_with_usage_fields() {
        let line = r#"{"timestamp":"2026-04-24T12:34:56.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1200,"cached_input_tokens":800,"output_tokens":240,"reasoning_output_tokens":32,"total_tokens":1440},"last_token_usage":{"input_tokens":300,"cached_input_tokens":120,"output_tokens":90,"reasoning_output_tokens":12,"total_tokens":390},"model_context_window":258400},"rate_limits":{"plan_type":"plus","primary":{"used_percent":47.0,"window_minutes":300,"resets_at":1777000000},"secondary":{"used_percent":24.0,"window_minutes":10080,"resets_at":1777600000}}}}"#;

        let event = parse_event_with_provider(line, "openai").expect("event should parse");

        assert_eq!(event.plan_type, "plus");
        assert_eq!(event.primary.window_minutes, 300);
        assert_eq!(event.secondary.window_minutes, 10080);
        assert_eq!(event.total_usage.as_ref().unwrap().total_tokens, 1440);
        assert_eq!(event.last_usage.as_ref().unwrap().total_tokens, 390);
        assert_eq!(event.model_context_window, Some(258400));
    }

    #[test]
    fn ignores_non_token_count_events() {
        let line = r#"{"timestamp":"2026-04-24T12:34:56.000Z","type":"event_msg","payload":{"type":"agent_message","message":"hello"}}"#;
        assert!(parse_event_with_provider(line, "openai").is_none());
    }

    fn test_usage(total_tokens: u64) -> TokenUsage {
        TokenUsage {
            input_tokens: total_tokens / 2,
            cached_input_tokens: 0,
            output_tokens: total_tokens / 2,
            reasoning_output_tokens: 0,
            total_tokens,
        }
    }

    fn test_event(timestamp: DateTime<Utc>, provider: &str, total_tokens: u64) -> UsageEvent {
        UsageEvent {
            timestamp,
            provider: provider.to_string(),
            plan_type: "unknown".to_string(),
            primary: empty_rate_limit(),
            secondary: empty_rate_limit(),
            total_usage: Some(test_usage(total_tokens)),
            last_usage: Some(test_usage(total_tokens)),
            model_context_window: None,
        }
    }

    #[test]
    fn builds_hourly_token_buckets_for_last_24_hours() {
        let now = DateTime::parse_from_rfc3339("2026-06-13T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let events = vec![
            test_event(now - Duration::minutes(20), "moapi", 100),
            test_event(
                now - Duration::hours(1) - Duration::minutes(20),
                "moapi",
                250,
            ),
            test_event(now - Duration::hours(25), "moapi", 999),
        ];

        let buckets = build_hourly_token_buckets(&events, now);

        assert_eq!(buckets.len(), 24);
        assert_eq!(buckets[23].total_tokens, 100);
        assert_eq!(buckets[23].event_count, 1);
        assert_eq!(buckets[22].total_tokens, 250);
        assert_eq!(
            buckets
                .iter()
                .map(|bucket| bucket.total_tokens)
                .sum::<u64>(),
            350
        );
    }

    #[test]
    fn relay_provider_uses_token_usage_mode() {
        assert!(!is_relay_usage_provider("openai"));
        assert!(is_relay_usage_provider("moapi"));
        assert!(is_relay_usage_provider("songsongcard"));
        assert!(!is_relay_usage_provider("unknown"));
    }
}

#[tauri::command]
fn load_usage_snapshot(sessions_dir: Option<String>) -> Result<UsageSnapshot, String> {
    build_snapshot(sessions_dir)
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn load_relay_settings() -> Result<relay_config::RelaySettings, String> {
    relay_config::load_relay_settings_from_default()
}

#[tauri::command]
fn load_relay_provider_settings(
    provider_id: String,
) -> Result<relay_config::RelaySettings, String> {
    relay_config::load_relay_settings_for_provider_default(provider_id)
}

#[tauri::command]
fn save_relay_settings(
    settings: relay_config::RelaySettings,
) -> Result<relay_config::RelaySettings, String> {
    relay_config::save_relay_settings_to_default(settings)
}

#[tauri::command]
fn apply_relay_config(
    settings: relay_config::RelaySettings,
) -> Result<relay_config::RelayApplyResult, String> {
    let result = relay_config::apply_relay_config_to_default(settings.clone())?;
    local_router::ensure_router_for_settings(&settings)?;
    Ok(result)
}

#[tauri::command]
fn test_relay_provider(
    settings: relay_config::RelaySettings,
) -> Result<relay_config::RelaySelfTestResult, String> {
    relay_config::test_relay_provider(settings)
}

#[tauri::command]
fn clear_relay_config() -> Result<relay_config::RelayApplyResult, String> {
    relay_config::clear_relay_config_from_default()
}

#[tauri::command]
fn relay_status() -> Result<relay_config::RelayStatus, String> {
    relay_config::relay_status_from_default()
}

#[tauri::command]
fn list_relay_providers() -> Result<Vec<relay_config::RelayProviderOption>, String> {
    relay_config::list_relay_providers_from_default()
}

#[tauri::command]
fn restart_codex_app() -> relay_config::RestartResult {
    relay_config::restart_codex_app()
}

#[tauri::command]
fn apply_relay_config_and_restart(
    settings: relay_config::RelaySettings,
) -> Result<relay_config::ApplyAndRestartResult, String> {
    let saved = relay_config::save_relay_settings_to_default(settings)?;
    let apply = relay_config::apply_relay_config_to_default(saved.clone())?;
    local_router::ensure_router_for_settings(&saved)?;
    let restart = relay_config::restart_codex_app();
    Ok(relay_config::ApplyAndRestartResult { apply, restart })
}

#[tauri::command]
fn history_sync_status() -> Result<history_sync::HistorySyncStatus, String> {
    history_sync::history_sync_status_default()
}

#[tauri::command]
fn sync_history_to_provider(
    provider: Option<String>,
) -> Result<history_sync::HistorySyncResult, String> {
    history_sync::sync_history_to_provider_default(provider)
}

#[tauri::command]
fn list_context_entries() -> Result<context_config::ContextEntries, String> {
    context_config::list_context_entries_from_default()
}

#[tauri::command]
fn upsert_context_entry(
    input: context_config::ContextEntryInput,
) -> Result<context_config::ContextApplyResult, String> {
    context_config::upsert_context_entry_default(input)
}

#[tauri::command]
fn toggle_context_entry(
    input: context_config::ContextToggleInput,
) -> Result<context_config::ContextApplyResult, String> {
    context_config::toggle_context_entry_default(input)
}

#[tauri::command]
fn delete_context_entry(
    input: context_config::ContextDeleteInput,
) -> Result<context_config::ContextApplyResult, String> {
    context_config::delete_context_entry_default(input)
}

#[tauri::command]
fn unlock_codex_plugins(
    request: plugin_unlock::UnlockRequest,
) -> Result<plugin_unlock::UnlockResult, String> {
    plugin_unlock::unlock_plugins_default(request)
}

fn restore_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_skip_taskbar(false);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.set_skip_taskbar(false);
                let _ = window.minimize();
            }
        })
        .setup(|app| {
            let show_item =
                MenuItem::with_id(app, "show", "Show Codex Toolkit", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let mut tray_builder = TrayIconBuilder::with_id("main-tray")
                .menu(&tray_menu)
                .tooltip("Codex Toolkit")
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => restore_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        restore_main_window(tray.app_handle());
                    }
                });

            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }

            let _tray = tray_builder.build(app)?;

            match relay_config::load_relay_settings_from_default() {
                Ok(settings) => {
                    if let Err(error) = local_router::ensure_router_for_settings(&settings) {
                        local_router::log_router_startup_error(&format!(
                            "router startup failed: {error}"
                        ));
                    }
                }
                Err(error) => {
                    local_router::log_router_startup_error(&format!(
                        "router settings load failed: {error}"
                    ));
                }
            }

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_skip_taskbar(false);

                if env::args().any(|arg| arg == "--autostart") {
                    let _ = window.hide();
                } else {
                    let _ = window.unminimize();
                    let _ = window.center();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_usage_snapshot,
            quit_app,
            load_relay_settings,
            load_relay_provider_settings,
            save_relay_settings,
            apply_relay_config,
            test_relay_provider,
            clear_relay_config,
            relay_status,
            list_relay_providers,
            restart_codex_app,
            apply_relay_config_and_restart,
            history_sync_status,
            sync_history_to_provider,
            list_context_entries,
            upsert_context_entry,
            toggle_context_entry,
            delete_context_entry,
            unlock_codex_plugins
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
