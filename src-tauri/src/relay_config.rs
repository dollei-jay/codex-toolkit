use chrono::Utc;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Duration as StdDuration, Instant},
};
use toml_edit::{value, DocumentMut, Item};

const DEFAULT_RELAY_PROVIDER: &str = "moapi";
const LEGACY_RELAY_PROVIDER: &str = "CodexViewerRelay";
pub const LOCAL_ROUTER_PORT: u16 = 15721;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelaySettings {
    pub enabled: bool,
    #[serde(default = "default_provider_id")]
    pub provider_id: String,
    #[serde(default = "default_route_mode")]
    pub route_mode: String,
    pub base_url: String,
    pub api_key: String,
    pub test_model: Option<String>,
    #[serde(default = "default_local_port")]
    pub local_port: u16,
    #[serde(default)]
    pub upstream_model: Option<String>,
    #[serde(default = "default_upstream_wire_api")]
    pub upstream_wire_api: String,
}

impl Default for RelaySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider_id: default_provider_id(),
            route_mode: default_route_mode(),
            base_url: String::new(),
            api_key: String::new(),
            test_model: None,
            local_port: default_local_port(),
            upstream_model: None,
            upstream_wire_api: default_upstream_wire_api(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayStatus {
    pub configured: bool,
    pub route: String,
    pub config_path: String,
    pub provider_id: String,
    pub route_mode: String,
    pub base_url: Option<String>,
    pub upstream_base_url: Option<String>,
    pub upstream_wire_api: String,
    pub has_base_url: bool,
    pub has_api_key: bool,
    pub masked_api_key: Option<String>,
    pub codex_running: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayProviderOption {
    pub provider_id: String,
    pub is_current: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayApplyResult {
    pub config_path: String,
    pub backup_path: Option<String>,
    pub configured: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartResult {
    pub killed: bool,
    pub started: bool,
    pub app_path: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyAndRestartResult {
    pub apply: RelayApplyResult,
    pub restart: RestartResult,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelaySelfTestResult {
    pub provider_id: String,
    pub route_mode: String,
    pub upstream_wire_api: String,
    pub upstream_base_url: String,
    pub upstream_model: Option<String>,
    pub ok: bool,
    pub checks: Vec<RelaySelfTestCheck>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelaySelfTestCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
    pub latency_ms: Option<u128>,
}

pub fn load_relay_settings_from_default() -> Result<RelaySettings, String> {
    let saved = load_relay_settings_from_path(&relay_settings_path()?)?;
    Ok(load_relay_settings_from_home(&codex_home_dir()?, saved))
}

pub fn load_relay_settings_for_provider_default(
    provider_id: String,
) -> Result<RelaySettings, String> {
    let saved = load_relay_settings_from_path(&relay_settings_path()?)?;
    load_relay_settings_for_provider_from_home(&codex_home_dir()?, saved, &provider_id)
}

pub fn save_relay_settings_to_default(settings: RelaySettings) -> Result<RelaySettings, String> {
    let saved = save_relay_settings_to_path(&relay_settings_path()?, settings)?;
    save_relay_profile_to_default(&saved)?;
    Ok(saved)
}

pub fn apply_relay_config_to_default(settings: RelaySettings) -> Result<RelayApplyResult, String> {
    apply_relay_config_to_home(&codex_home_dir()?, settings)
}

pub fn test_relay_provider(settings: RelaySettings) -> Result<RelaySelfTestResult, String> {
    let mut settings = settings;
    normalize_settings(&mut settings);
    validate_relay_settings(&settings)?;

    let mut checks = Vec::new();
    if settings.route_mode == "local_router" {
        checks.push(test_local_router_health(settings.local_port));
        checks.push(test_chat_completion(&settings, false));
        checks.push(test_chat_completion(&settings, true));
    } else {
        checks.push(test_responses_endpoint(&settings));
    }

    let ok = checks.iter().all(|check| check.ok);
    Ok(RelaySelfTestResult {
        provider_id: settings.provider_id,
        route_mode: settings.route_mode,
        upstream_wire_api: settings.upstream_wire_api,
        upstream_base_url: settings.base_url,
        upstream_model: settings.upstream_model,
        ok,
        checks,
    })
}

pub fn clear_relay_config_from_default() -> Result<RelayApplyResult, String> {
    let settings = load_relay_settings_from_default().unwrap_or_default();
    clear_relay_config_from_home(&codex_home_dir()?, &settings)
}

pub fn relay_status_from_default() -> Result<RelayStatus, String> {
    let home = codex_home_dir()?;
    let settings = load_relay_settings_from_home(
        &home,
        load_relay_settings_from_path(&relay_settings_path()?).unwrap_or_default(),
    );
    Ok(relay_status_from_home(&home, &settings))
}

pub fn list_relay_providers_from_default() -> Result<Vec<RelayProviderOption>, String> {
    let home = codex_home_dir()?;
    let saved = load_relay_settings_from_path(&relay_settings_path()?).unwrap_or_default();
    Ok(list_relay_providers_from_home(&home, &saved))
}

pub fn restart_codex_app() -> RestartResult {
    let app_path = codex_process_path();
    let was_running = codex_running();
    let killed = if was_running {
        stop_codex_processes()
    } else {
        false
    };

    if killed {
        thread::sleep(StdDuration::from_millis(1200));
    }

    let started = app_path
        .as_ref()
        .map(|path| spawn_hidden(path).is_ok())
        .unwrap_or(false);

    let message = if started {
        "Codex App restarted.".to_string()
    } else if was_running {
        "Codex was closed, but the app path was unavailable. Please start Codex manually."
            .to_string()
    } else {
        "Codex App is not running and no launch path was found. Please start Codex manually."
            .to_string()
    };

    RestartResult {
        killed,
        started,
        app_path,
        message,
    }
}

fn home_dir() -> Result<PathBuf, String> {
    env::var("USERPROFILE")
        .or_else(|_| env::var("HOME"))
        .map(PathBuf::from)
        .map_err(|_| "Unable to locate the user home directory".to_string())
}

fn codex_home_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".codex"))
}

fn relay_settings_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".codexviewer").join("relay-settings.json"))
}

fn relay_profiles_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".codexviewer").join("relay-providers"))
}

fn relay_profile_path(provider_id: &str) -> Result<PathBuf, String> {
    Ok(relay_profiles_dir()?.join(format!("{}.json", normalize_provider_id(provider_id))))
}

fn default_provider_id() -> String {
    DEFAULT_RELAY_PROVIDER.to_string()
}

fn default_route_mode() -> String {
    "direct".to_string()
}

fn default_local_port() -> u16 {
    LOCAL_ROUTER_PORT
}

fn default_upstream_wire_api() -> String {
    "responses".to_string()
}

fn load_relay_settings_from_path(path: &Path) -> Result<RelaySettings, String> {
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(contents.trim_start_matches('\u{feff}'))
            .map_err(|_| "Relay settings are invalid".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(RelaySettings::default()),
        Err(error) => Err(error.to_string()),
    }
}

fn save_relay_profile_to_default(settings: &RelaySettings) -> Result<(), String> {
    if settings.provider_id.eq_ignore_ascii_case("openai") {
        return Ok(());
    }
    let path = relay_profile_path(&settings.provider_id)?;
    let _ = save_relay_settings_to_path(&path, settings.clone())?;
    Ok(())
}

fn load_relay_profile(provider_id: &str) -> Option<RelaySettings> {
    let path = relay_profile_path(provider_id).ok()?;
    load_relay_settings_from_path(&path).ok()
}

fn load_relay_settings_from_home(home: &Path, saved: RelaySettings) -> RelaySettings {
    let config_path = home.join("config.toml");
    let contents = fs::read_to_string(&config_path).unwrap_or_default();
    settings_from_active_config(&contents, &saved).unwrap_or(saved)
}

fn load_relay_settings_for_provider_from_home(
    home: &Path,
    saved: RelaySettings,
    provider_id: &str,
) -> Result<RelaySettings, String> {
    let profile = load_relay_profile(provider_id);
    load_relay_settings_for_provider_from_home_with_profile(home, saved, provider_id, profile)
}

fn load_relay_settings_for_provider_from_home_with_profile(
    home: &Path,
    saved: RelaySettings,
    provider_id: &str,
    profile: Option<RelaySettings>,
) -> Result<RelaySettings, String> {
    let provider_id = normalize_provider_id(provider_id);
    if provider_id.eq_ignore_ascii_case("openai") {
        return Ok(RelaySettings {
            enabled: false,
            provider_id,
            ..RelaySettings::default()
        });
    }
    let saved = profile.unwrap_or(saved);

    let contents = fs::read_to_string(home.join("config.toml")).unwrap_or_default();
    if contents.trim().is_empty() {
        return Ok(RelaySettings {
            enabled: true,
            provider_id,
            ..saved
        });
    }

    let doc = parse_toml(&contents)?;
    let provider = doc
        .get("model_providers")
        .and_then(|item| item.as_table())
        .and_then(|providers| providers.get(provider_id.as_str()))
        .and_then(|item| item.as_table());
    if provider.is_none() {
        return Ok(RelaySettings {
            enabled: true,
            provider_id,
            ..saved
        });
    }
    let base_url = provider
        .and_then(|table| provider_value(&doc, table, "base_url"))
        .unwrap_or_default();
    let api_key = provider
        .and_then(|table| {
            provider_value(&doc, table, "experimental_bearer_token")
                .or_else(|| provider_value(&doc, table, "api_key"))
        })
        .unwrap_or_default();
    let is_local_router =
        base_url.starts_with("http://127.0.0.1:") || base_url.starts_with("http://localhost:");

    let mut loaded = RelaySettings {
        enabled: true,
        provider_id,
        route_mode: if is_local_router {
            "local_router".to_string()
        } else {
            "direct".to_string()
        },
        base_url: if is_local_router && !saved.base_url.trim().is_empty() {
            saved.base_url
        } else {
            base_url
        },
        api_key: if is_local_router && !saved.api_key.trim().is_empty() {
            saved.api_key
        } else {
            api_key
        },
        test_model: saved.test_model,
        local_port: saved.local_port,
        upstream_model: saved.upstream_model,
        upstream_wire_api: saved.upstream_wire_api,
    };
    normalize_settings(&mut loaded);
    Ok(loaded)
}

fn settings_from_active_config(contents: &str, saved: &RelaySettings) -> Option<RelaySettings> {
    let doc = parse_toml(contents).ok()?;
    let provider_id = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let provider = doc
        .get("model_providers")?
        .as_table()?
        .get(provider_id)?
        .as_table()?;
    let base_url = provider_value(&doc, provider, "base_url")?;
    let api_key = provider_value(&doc, provider, "experimental_bearer_token")
        .or_else(|| provider_value(&doc, provider, "api_key"))
        .unwrap_or_default();

    let is_local_router =
        base_url.starts_with("http://127.0.0.1:") || base_url.starts_with("http://localhost:");

    let mut loaded = RelaySettings {
        enabled: true,
        provider_id: provider_id.to_string(),
        route_mode: if is_local_router {
            "local_router".to_string()
        } else {
            saved.route_mode.clone()
        },
        base_url: if is_local_router && !saved.base_url.trim().is_empty() {
            saved.base_url.clone()
        } else {
            base_url.to_string()
        },
        api_key: if is_local_router && !saved.api_key.trim().is_empty() {
            saved.api_key.clone()
        } else {
            api_key.to_string()
        },
        test_model: saved.test_model.clone(),
        local_port: saved.local_port,
        upstream_model: saved.upstream_model.clone(),
        upstream_wire_api: saved.upstream_wire_api.clone(),
    };
    normalize_settings(&mut loaded);
    Some(loaded)
}

fn save_relay_settings_to_path(
    path: &Path,
    mut settings: RelaySettings,
) -> Result<RelaySettings, String> {
    normalize_settings(&mut settings);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())?;
    Ok(settings)
}

fn normalize_settings(settings: &mut RelaySettings) {
    settings.provider_id = normalize_provider_id(&settings.provider_id);
    settings.route_mode = normalize_route_mode(&settings.route_mode);
    settings.base_url = settings.base_url.trim().to_string();
    settings.api_key = settings.api_key.trim().to_string();
    if settings.local_port == 0 {
        settings.local_port = default_local_port();
    }
    settings.test_model = settings
        .test_model
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    settings.upstream_model = settings
        .upstream_model
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    settings.upstream_wire_api = normalize_upstream_wire_api(&settings.upstream_wire_api);
    if settings.route_mode == "local_router" && settings.upstream_wire_api == "responses" {
        settings.upstream_wire_api = "chat_completions".to_string();
    }
    if settings.route_mode == "local_router" && settings.upstream_model.is_none() {
        settings.upstream_model = Some("deepseek-chat".to_string());
    }
}

fn validate_relay_settings(settings: &RelaySettings) -> Result<(), String> {
    if !settings.enabled {
        return Err("Relay is disabled.".to_string());
    }
    if !is_valid_provider_id(&settings.provider_id) {
        return Err(
            "Provider ID can only contain letters, numbers, underscore and hyphen.".to_string(),
        );
    }
    if settings.base_url.trim().is_empty() {
        return Err("API Base URL cannot be empty.".to_string());
    }
    if !settings.base_url.starts_with("http://") && !settings.base_url.starts_with("https://") {
        return Err("API Base URL must start with http:// or https://.".to_string());
    }
    if settings.api_key.trim().is_empty() {
        return Err("API Key cannot be empty.".to_string());
    }
    if settings.route_mode == "local_router" && settings.local_port == 0 {
        return Err("Local router port cannot be empty.".to_string());
    }
    if settings.route_mode == "local_router" && settings.upstream_wire_api != "chat_completions" {
        return Err("Local router currently supports chat_completions upstreams only.".to_string());
    }
    Ok(())
}

fn self_test_check(
    name: &str,
    ok: bool,
    message: impl Into<String>,
    latency_ms: Option<u128>,
) -> RelaySelfTestCheck {
    RelaySelfTestCheck {
        name: name.to_string(),
        ok,
        message: message.into(),
        latency_ms,
    }
}

fn test_local_router_health(port: u16) -> RelaySelfTestCheck {
    let client = match Client::builder().timeout(Duration::from_secs(8)).build() {
        Ok(client) => client,
        Err(error) => {
            return self_test_check("local_router_health", false, error.to_string(), None)
        }
    };
    let url = format!("http://127.0.0.1:{port}/health");
    let start = Instant::now();
    match client.get(url).send() {
        Ok(response) => {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            let ok = status.is_success() && text.contains("codex-toolkit-local-router");
            self_test_check(
                "local_router_health",
                ok,
                if ok {
                    "Local router is reachable.".to_string()
                } else {
                    format!("Unexpected router response: HTTP {status}")
                },
                Some(start.elapsed().as_millis()),
            )
        }
        Err(error) => self_test_check(
            "local_router_health",
            false,
            format!("Local router is not reachable: {error}"),
            Some(start.elapsed().as_millis()),
        ),
    }
}

fn test_chat_completion(settings: &RelaySettings, stream: bool) -> RelaySelfTestCheck {
    let client = match Client::builder().timeout(Duration::from_secs(30)).build() {
        Ok(client) => client,
        Err(error) => return self_test_check("chat_completion", false, error.to_string(), None),
    };
    let url = format!(
        "{}/chat/completions",
        settings.base_url.trim().trim_end_matches('/')
    );
    let model = settings
        .upstream_model
        .as_deref()
        .or(settings.test_model.as_deref())
        .unwrap_or("deepseek-chat");
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Reply with ok."}],
        "max_tokens": 16,
        "stream": stream
    });
    let start = Instant::now();
    let name = if stream {
        "upstream_chat_stream"
    } else {
        "upstream_chat_non_stream"
    };
    match client
        .post(url)
        .bearer_auth(settings.api_key.trim())
        .json(&body)
        .send()
    {
        Ok(response) => {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            let ok = status.is_success()
                && if stream {
                    text.contains("data:") && (text.contains("[DONE]") || text.contains("choices"))
                } else {
                    text.contains("choices")
                };
            self_test_check(
                name,
                ok,
                summarize_self_test_response(status.as_u16(), &text, ok),
                Some(start.elapsed().as_millis()),
            )
        }
        Err(error) => self_test_check(
            name,
            false,
            format!("Upstream request failed: {error}"),
            Some(start.elapsed().as_millis()),
        ),
    }
}

fn test_responses_endpoint(settings: &RelaySettings) -> RelaySelfTestCheck {
    let client = match Client::builder().timeout(Duration::from_secs(30)).build() {
        Ok(client) => client,
        Err(error) => return self_test_check("upstream_responses", false, error.to_string(), None),
    };
    let url = format!(
        "{}/responses",
        settings.base_url.trim().trim_end_matches('/')
    );
    let model = settings.test_model.as_deref().unwrap_or("gpt-5.5");
    let body = serde_json::json!({
        "model": model,
        "input": "Reply with ok.",
        "max_output_tokens": 16
    });
    let start = Instant::now();
    match client
        .post(url)
        .bearer_auth(settings.api_key.trim())
        .json(&body)
        .send()
    {
        Ok(response) => {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            let ok = status.is_success() && text.contains("response");
            self_test_check(
                "upstream_responses",
                ok,
                summarize_self_test_response(status.as_u16(), &text, ok),
                Some(start.elapsed().as_millis()),
            )
        }
        Err(error) => self_test_check(
            "upstream_responses",
            false,
            format!("Upstream request failed: {error}"),
            Some(start.elapsed().as_millis()),
        ),
    }
}

fn summarize_self_test_response(status: u16, body: &str, ok: bool) -> String {
    if ok {
        return format!("HTTP {status} OK");
    }
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    if let Some(message) = parsed
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
    {
        return format!("HTTP {status}: {message}");
    }
    let snippet = body.chars().take(240).collect::<String>();
    format!("HTTP {status}: {snippet}")
}

fn apply_relay_config_to_home(
    home: &Path,
    mut settings: RelaySettings,
) -> Result<RelayApplyResult, String> {
    normalize_settings(&mut settings);
    validate_relay_settings(&settings)?;
    fs::create_dir_all(home).map_err(|error| error.to_string())?;

    let config_path = home.join("config.toml");
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let backup_path = backup_config_if_exists(&config_path)?;
    let updated = upsert_relay_provider(&existing, &settings)?;
    fs::write(&config_path, updated).map_err(|error| error.to_string())?;
    let status = relay_status_from_home(home, &settings);

    Ok(RelayApplyResult {
        config_path: config_path.to_string_lossy().to_string(),
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
        message: "Relay configuration applied. Restart Codex to use it.".to_string(),
    })
}

fn clear_relay_config_from_home(
    home: &Path,
    settings: &RelaySettings,
) -> Result<RelayApplyResult, String> {
    fs::create_dir_all(home).map_err(|error| error.to_string())?;
    let config_path = home.join("config.toml");
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let backup_path = backup_config_if_exists(&config_path)?;
    let updated = remove_relay_provider(&existing, &normalize_provider_id(&settings.provider_id))?;
    fs::write(&config_path, updated).map_err(|error| error.to_string())?;
    let status = relay_status_from_home(home, settings);

    Ok(RelayApplyResult {
        config_path: config_path.to_string_lossy().to_string(),
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
        message: "Official endpoint restored. Restart Codex to use it.".to_string(),
    })
}

fn relay_status_from_home(home: &Path, settings: &RelaySettings) -> RelayStatus {
    let config_path = home.join("config.toml");
    let contents = fs::read_to_string(&config_path).unwrap_or_default();
    let configured_provider_id = configured_provider_id(&contents, settings);
    let provider = relay_provider_table(&contents, &configured_provider_id);
    let doc = parse_toml(&contents).ok();
    let base_url = doc.as_ref().and_then(|doc| {
        provider
            .as_ref()
            .and_then(|provider| provider_value(doc, provider, "base_url"))
    });
    let api_key = doc.as_ref().and_then(|doc| {
        provider.as_ref().and_then(|provider| {
            provider_value(doc, provider, "experimental_bearer_token")
                .or_else(|| provider_value(doc, provider, "api_key"))
        })
    });
    let has_base_url = base_url.is_some();
    let has_api_key = api_key.is_some();
    let configured = !configured_provider_id.is_empty() && has_base_url && has_api_key;

    RelayStatus {
        configured,
        route: if configured { "relay" } else { "official" }.to_string(),
        config_path: config_path.to_string_lossy().to_string(),
        provider_id: if configured_provider_id.is_empty() {
            normalize_provider_id(&settings.provider_id)
        } else {
            configured_provider_id
        },
        route_mode: normalize_route_mode(&settings.route_mode),
        upstream_base_url: if normalize_route_mode(&settings.route_mode) == "local_router" {
            Some(settings.base_url.clone()).filter(|value| !value.is_empty())
        } else {
            None
        },
        upstream_wire_api: normalize_upstream_wire_api(&settings.upstream_wire_api),
        base_url,
        has_base_url,
        has_api_key,
        masked_api_key: api_key.as_deref().and_then(mask_api_key),
        codex_running: codex_running(),
    }
}

fn list_relay_providers_from_home(home: &Path, saved: &RelaySettings) -> Vec<RelayProviderOption> {
    let contents = fs::read_to_string(home.join("config.toml")).unwrap_or_default();
    let doc = parse_toml(&contents).ok();
    let current_provider = doc
        .as_ref()
        .and_then(|doc| doc.get("model_provider"))
        .and_then(|item| item.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| "openai".to_string());
    let mut providers = vec!["openai".to_string()];
    if !current_provider.trim().is_empty() {
        providers.push(current_provider.clone());
    }

    let saved_provider = normalize_provider_id(&saved.provider_id);
    if !saved_provider.is_empty() {
        providers.push(saved_provider);
    }

    if let Some(model_providers) = doc
        .as_ref()
        .and_then(|doc| doc.get("model_providers"))
        .and_then(|item| item.as_table())
    {
        for provider in model_providers.iter().map(|(name, _)| name.to_string()) {
            if !provider.trim().is_empty() {
                providers.push(provider);
            }
        }
    }

    providers.sort();
    providers.dedup();
    providers
        .into_iter()
        .map(|provider_id| RelayProviderOption {
            is_current: provider_id == current_provider,
            provider_id,
        })
        .collect()
}

fn provider_value(doc: &DocumentMut, provider: &toml_edit::Table, key: &str) -> Option<String> {
    provider
        .get(key)
        .or_else(|| doc.get(key))
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_toml(contents: &str) -> Result<DocumentMut, String> {
    contents
        .parse::<DocumentMut>()
        .map_err(|_| "Codex config.toml is invalid.".to_string())
}

fn upsert_relay_provider(contents: &str, settings: &RelaySettings) -> Result<String, String> {
    let provider_id = normalize_provider_id(&settings.provider_id);
    let route_mode = normalize_route_mode(&settings.route_mode);
    let base_url = if route_mode == "local_router" {
        format!("http://127.0.0.1:{}/v1", settings.local_port)
    } else {
        settings.base_url.clone()
    };
    let api_key = if route_mode == "local_router" {
        "codex-toolkit-local-router".to_string()
    } else {
        settings.api_key.clone()
    };
    let mut doc = if contents.trim().is_empty() {
        DocumentMut::new()
    } else {
        parse_toml(contents)?
    };

    doc["model_provider"] = value(provider_id.as_str());
    if settings.test_model.is_some() && doc.get("model").is_none() {
        doc["model"] = value(settings.test_model.as_ref().unwrap().as_str());
    }

    if !doc.as_table().contains_key("model_providers") || !doc["model_providers"].is_table() {
        doc["model_providers"] = toml_edit::table();
    }

    doc["model_providers"][provider_id.as_str()] = toml_edit::table();
    doc["model_providers"][provider_id.as_str()]["name"] = value(provider_id.as_str());
    doc["model_providers"][provider_id.as_str()]["wire_api"] = value("responses");
    doc["model_providers"][provider_id.as_str()]["requires_openai_auth"] = value(true);
    doc["model_providers"][provider_id.as_str()]["base_url"] = value(base_url.as_str());
    doc["model_providers"][provider_id.as_str()]["experimental_bearer_token"] =
        value(api_key.as_str());

    Ok(ensure_trailing_newline(doc.to_string()))
}

fn remove_relay_provider(contents: &str, provider_id: &str) -> Result<String, String> {
    let mut doc = if contents.trim().is_empty() {
        DocumentMut::new()
    } else {
        parse_toml(contents)?
    };

    let current_provider = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    let owned_providers = [provider_id, DEFAULT_RELAY_PROVIDER, LEGACY_RELAY_PROVIDER];
    if current_provider
        .as_deref()
        .map(|provider| owned_providers.contains(&provider))
        .unwrap_or(false)
    {
        doc.as_table_mut().remove("model_provider");
    }

    if let Some(model_providers) = doc.get_mut("model_providers").and_then(Item::as_table_mut) {
        for provider in owned_providers {
            model_providers.remove(provider);
        }
        if model_providers.is_empty() {
            doc.as_table_mut().remove("model_providers");
        }
    }

    Ok(ensure_trailing_newline(doc.to_string()))
}

fn relay_provider_table(contents: &str, provider_id: &str) -> Option<toml_edit::Table> {
    let doc = parse_toml(contents).ok()?;
    doc.get("model_providers")?
        .as_table()?
        .get(provider_id)?
        .as_table()
        .cloned()
}

fn configured_provider_id(contents: &str, settings: &RelaySettings) -> String {
    let settings_provider_id = normalize_provider_id(&settings.provider_id);
    let doc = match parse_toml(contents) {
        Ok(doc) => doc,
        Err(_) => return String::new(),
    };
    let root_provider = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    match root_provider.as_deref() {
        Some(provider)
            if provider == settings_provider_id
                || provider == DEFAULT_RELAY_PROVIDER
                || provider == LEGACY_RELAY_PROVIDER =>
        {
            provider.to_string()
        }
        Some(provider)
            if doc
                .get("model_providers")
                .and_then(|item| item.as_table())
                .and_then(|providers| providers.get(provider))
                .and_then(|item| item.as_table())
                .and_then(|table| table.get("base_url"))
                .and_then(|item| item.as_str())
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false) =>
        {
            provider.to_string()
        }
        _ => String::new(),
    }
}

fn normalize_provider_id(provider_id: &str) -> String {
    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        default_provider_id()
    } else {
        provider_id.to_string()
    }
}

fn normalize_route_mode(route_mode: &str) -> String {
    match route_mode.trim() {
        "local_router" => "local_router".to_string(),
        _ => "direct".to_string(),
    }
}

fn normalize_upstream_wire_api(upstream_wire_api: &str) -> String {
    match upstream_wire_api.trim() {
        "chat_completions" | "chat" | "openai_chat" => "chat_completions".to_string(),
        _ => "responses".to_string(),
    }
}

fn is_valid_provider_id(provider_id: &str) -> bool {
    !provider_id.is_empty()
        && provider_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

fn backup_config_if_exists(config_path: &Path) -> Result<Option<PathBuf>, String> {
    if !config_path.exists() {
        return Ok(None);
    }
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let backup_path =
        config_path.with_file_name(format!("config.toml.codexviewer-backup-{timestamp}"));
    fs::copy(config_path, &backup_path).map_err(|error| error.to_string())?;
    Ok(Some(backup_path))
}

fn ensure_trailing_newline(mut contents: String) -> String {
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents
}

fn mask_api_key(api_key: &str) -> Option<String> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return None;
    }
    let head: String = api_key.chars().take(2).collect();
    let tail: String = api_key
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    Some(format!("{head}...{tail}"))
}

#[cfg(target_os = "windows")]
fn codex_running() -> bool {
    powershell_output(
        "Get-Process | Where-Object { $_.ProcessName -eq 'Codex' -or $_.ProcessName -eq 'OpenAI Codex' } | Select-Object -First 1 -ExpandProperty ProcessName",
    )
    .map(|output| !output.trim().is_empty())
    .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn codex_running() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn codex_process_path() -> Option<String> {
    powershell_output(
        "Get-Process | Where-Object { $_.ProcessName -eq 'Codex' -or $_.ProcessName -eq 'OpenAI Codex' } | Select-Object -First 1 -ExpandProperty Path",
    )
    .ok()
    .map(|output| output.trim().to_string())
    .filter(|output| !output.is_empty())
}

#[cfg(not(target_os = "windows"))]
fn codex_process_path() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn stop_codex_processes() -> bool {
    powershell_output(
        "Get-Process | Where-Object { $_.ProcessName -eq 'Codex' -or $_.ProcessName -eq 'OpenAI Codex' } | Stop-Process -Force",
    )
    .is_ok()
}

#[cfg(not(target_os = "windows"))]
fn stop_codex_processes() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn spawn_hidden(path: &str) -> Result<(), String> {
    Command::new(path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "windows"))]
fn spawn_hidden(path: &str) -> Result<(), String> {
    Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn powershell_output(script: &str) -> Result<String, String> {
    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("PowerShell command failed.".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relay_settings() -> RelaySettings {
        RelaySettings {
            enabled: true,
            provider_id: default_provider_id(),
            route_mode: default_route_mode(),
            base_url: "https://relay.example.test/v1".to_string(),
            api_key: "sk-secret-value".to_string(),
            test_model: Some("gpt-5-mini".to_string()),
            local_port: default_local_port(),
            upstream_model: None,
            upstream_wire_api: default_upstream_wire_api(),
        }
    }

    #[test]
    fn apply_relay_config_creates_config_toml() {
        let temp = tempfile::tempdir().unwrap();
        let result = apply_relay_config_to_home(temp.path(), relay_settings()).unwrap();
        let config = fs::read_to_string(temp.path().join("config.toml")).unwrap();

        assert!(result.configured);
        assert!(result.backup_path.is_none());
        assert!(config.contains(r#"model_provider = "moapi""#));
        assert!(config.contains("[model_providers.moapi]"));
        assert!(config.contains(r#"base_url = "https://relay.example.test/v1""#));
        assert!(config.contains(r#"experimental_bearer_token = "sk-secret-value""#));
    }

    #[test]
    fn apply_relay_config_backs_up_existing_config() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("config.toml"), r#"model = "gpt-5""#).unwrap();

        let result = apply_relay_config_to_home(temp.path(), relay_settings()).unwrap();

        assert!(result.backup_path.is_some());
        assert!(PathBuf::from(result.backup_path.unwrap()).exists());
    }

    #[test]
    fn apply_relay_config_preserves_unrelated_config() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("config.toml"),
            r#"[features]
goals = true
"#,
        )
        .unwrap();

        apply_relay_config_to_home(temp.path(), relay_settings()).unwrap();
        let config = fs::read_to_string(temp.path().join("config.toml")).unwrap();

        assert!(config.contains("[features]"));
        assert!(config.contains("goals = true"));
        assert!(config.contains("[model_providers.moapi]"));
    }

    #[test]
    fn clear_relay_config_removes_provider() {
        let temp = tempfile::tempdir().unwrap();
        apply_relay_config_to_home(temp.path(), relay_settings()).unwrap();

        let result = clear_relay_config_from_home(temp.path(), &relay_settings()).unwrap();
        let config = fs::read_to_string(temp.path().join("config.toml")).unwrap();

        assert!(!result.configured);
        assert!(!config.contains("moapi"));
    }

    #[test]
    fn apply_relay_config_rejects_empty_base_url() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = relay_settings();
        settings.base_url = " ".to_string();

        let error = apply_relay_config_to_home(temp.path(), settings).unwrap_err();

        assert!(error.contains("Base URL"));
        assert!(!error.contains("sk-secret-value"));
    }

    #[test]
    fn apply_relay_config_rejects_empty_api_key_without_leaking_secret() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = relay_settings();
        settings.api_key = " ".to_string();

        let error = apply_relay_config_to_home(temp.path(), settings).unwrap_err();

        assert!(error.contains("API Key"));
        assert!(!error.contains("sk-secret-value"));
    }

    #[test]
    fn settings_roundtrip_persists_relay_settings() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("relay-settings.json");

        save_relay_settings_to_path(&path, relay_settings()).unwrap();
        let loaded = load_relay_settings_from_path(&path).unwrap();

        assert!(loaded.enabled);
        assert_eq!(loaded.base_url, "https://relay.example.test/v1");
        assert_eq!(loaded.api_key, "sk-secret-value");
        assert_eq!(loaded.provider_id, "moapi");
    }

    #[test]
    fn local_router_settings_default_to_chat_completions_upstream() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("relay-settings.json");
        let mut settings = relay_settings();
        settings.route_mode = "local_router".to_string();
        settings.upstream_wire_api = "responses".to_string();
        settings.upstream_model = Some("deepseek-chat".to_string());

        save_relay_settings_to_path(&path, settings).unwrap();
        let loaded = load_relay_settings_from_path(&path).unwrap();

        assert_eq!(loaded.upstream_wire_api, "chat_completions");
    }

    #[test]
    fn load_relay_settings_prefers_active_config_provider() {
        let temp = tempfile::tempdir().unwrap();
        let saved = RelaySettings {
            enabled: false,
            provider_id: "old_provider".to_string(),
            route_mode: default_route_mode(),
            base_url: "https://old.example.test/v1".to_string(),
            api_key: "sk-old-value".to_string(),
            test_model: Some("gpt-5-mini".to_string()),
            local_port: default_local_port(),
            upstream_model: None,
            upstream_wire_api: default_upstream_wire_api(),
        };
        fs::write(
            temp.path().join("config.toml"),
            r#"model_provider = "custom_provider"

[model_providers.custom_provider]
name = "custom_provider"
wire_api = "responses"
base_url = "https://active.example.test/v1"
experimental_bearer_token = "sk-active-value"
"#,
        )
        .unwrap();

        let loaded = load_relay_settings_from_home(temp.path(), saved);

        assert!(loaded.enabled);
        assert_eq!(loaded.provider_id, "custom_provider");
        assert_eq!(loaded.base_url, "https://active.example.test/v1");
        assert_eq!(loaded.api_key, "sk-active-value");
        assert_eq!(loaded.test_model, Some("gpt-5-mini".to_string()));
    }

    #[test]
    fn load_relay_settings_falls_back_to_root_provider_fields() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("config.toml"),
            r#"model_provider = "moapi"
base_url = "https://root.example.test/v1"
api_key = "sk-root-value"

[model_providers.moapi]
name = "moapi"
wire_api = "responses"
"#,
        )
        .unwrap();

        let loaded = load_relay_settings_from_home(temp.path(), RelaySettings::default());
        let status = relay_status_from_home(temp.path(), &loaded);

        assert!(loaded.enabled);
        assert_eq!(loaded.provider_id, "moapi");
        assert_eq!(loaded.base_url, "https://root.example.test/v1");
        assert_eq!(loaded.api_key, "sk-root-value");
        assert!(status.configured);
        assert_eq!(status.route, "relay");
        assert_eq!(
            status.base_url,
            Some("https://root.example.test/v1".to_string())
        );
        assert_eq!(status.masked_api_key, Some("sk...alue".to_string()));
    }

    #[test]
    fn apply_relay_config_uses_custom_provider_id() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = relay_settings();
        settings.provider_id = "custom_provider".to_string();

        apply_relay_config_to_home(temp.path(), settings).unwrap();
        let config = fs::read_to_string(temp.path().join("config.toml")).unwrap();

        assert!(config.contains(r#"model_provider = "custom_provider""#));
        assert!(config.contains("[model_providers.custom_provider]"));
        assert!(config.contains(r#"name = "custom_provider""#));
    }

    #[test]
    fn local_router_config_points_codex_to_localhost() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = relay_settings();
        settings.provider_id = "deepseek".to_string();
        settings.route_mode = "local_router".to_string();
        settings.base_url = "https://api.deepseek.com/v1".to_string();
        settings.api_key = "sk-deepseek-secret".to_string();
        settings.upstream_model = Some("deepseek-chat".to_string());

        apply_relay_config_to_home(temp.path(), settings).unwrap();
        let config = fs::read_to_string(temp.path().join("config.toml")).unwrap();

        assert!(config.contains(r#"model_provider = "deepseek""#));
        assert!(config.contains(r#"base_url = "http://127.0.0.1:15721/v1""#));
        assert!(config.contains(r#"experimental_bearer_token = "codex-toolkit-local-router""#));
        assert!(!config.contains("sk-deepseek-secret"));
    }

    #[test]
    fn provider_settings_detect_local_router_route_mode() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("config.toml"),
            r#"model_provider = "deepseek"

[model_providers.deepseek]
name = "deepseek"
wire_api = "responses"
base_url = "http://127.0.0.1:15721/v1"
experimental_bearer_token = "codex-toolkit-local-router"
"#,
        )
        .unwrap();
        let saved = RelaySettings {
            route_mode: "direct".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-deepseek-secret".to_string(),
            upstream_model: Some("deepseek-chat".to_string()),
            ..RelaySettings::default()
        };

        let loaded =
            load_relay_settings_for_provider_from_home(temp.path(), saved, "deepseek").unwrap();

        assert_eq!(loaded.route_mode, "local_router");
        assert_eq!(loaded.base_url, "http://127.0.0.1:15721/v1");
        assert_eq!(loaded.api_key, "codex-toolkit-local-router");
    }

    #[test]
    fn provider_settings_detect_direct_route_mode() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("config.toml"),
            r#"model_provider = "moapi"

[model_providers.moapi]
name = "moapi"
wire_api = "responses"
base_url = "https://relay.example.test/v1"
experimental_bearer_token = "sk-relay"
"#,
        )
        .unwrap();
        let saved = RelaySettings {
            route_mode: "local_router".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-deepseek-secret".to_string(),
            upstream_model: Some("deepseek-chat".to_string()),
            ..RelaySettings::default()
        };

        let loaded =
            load_relay_settings_for_provider_from_home(temp.path(), saved, "moapi").unwrap();

        assert_eq!(loaded.route_mode, "direct");
        assert_eq!(loaded.base_url, "https://relay.example.test/v1");
        assert_eq!(loaded.api_key, "sk-relay");
    }

    #[test]
    fn provider_settings_use_provider_profile_for_local_router_upstream() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("config.toml"),
            r#"model_provider = "gui"

[model_providers.gui]
name = "gui"
wire_api = "responses"
base_url = "http://127.0.0.1:15721/v1"
experimental_bearer_token = "codex-toolkit-local-router"
"#,
        )
        .unwrap();
        let fallback = RelaySettings {
            provider_id: "moapi".to_string(),
            route_mode: "direct".to_string(),
            base_url: "https://moapi.example.test/v1".to_string(),
            api_key: "sk-moapi".to_string(),
            upstream_model: Some("moapi-model".to_string()),
            ..RelaySettings::default()
        };
        let profile = RelaySettings {
            enabled: true,
            provider_id: "gui".to_string(),
            route_mode: "local_router".to_string(),
            base_url: "https://gui.example.test/v1".to_string(),
            api_key: "sk-gui".to_string(),
            upstream_model: Some("deepseek-ai/DeepSeek-V3.2".to_string()),
            ..RelaySettings::default()
        };
        let loaded = load_relay_settings_for_provider_from_home_with_profile(
            temp.path(),
            fallback,
            "gui",
            Some(profile),
        )
        .unwrap();

        assert_eq!(loaded.route_mode, "local_router");
        assert_eq!(loaded.base_url, "https://gui.example.test/v1");
        assert_eq!(loaded.api_key, "sk-gui");
        assert_eq!(loaded.upstream_wire_api, "chat_completions");
        assert_eq!(
            loaded.upstream_model,
            Some("deepseek-ai/DeepSeek-V3.2".to_string())
        );
    }

    #[test]
    fn relay_status_detects_active_custom_provider_from_config() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("config.toml"),
            r#"model_provider = "custom_provider"

[model_providers.custom_provider]
name = "custom_provider"
wire_api = "responses"
base_url = "https://active.example.test/v1"
experimental_bearer_token = "sk-active-value"
"#,
        )
        .unwrap();

        let settings = load_relay_settings_from_home(temp.path(), RelaySettings::default());
        let status = relay_status_from_home(temp.path(), &settings);

        assert!(status.configured);
        assert_eq!(status.route, "relay");
        assert_eq!(status.provider_id, "custom_provider");
        assert_eq!(
            status.base_url,
            Some("https://active.example.test/v1".to_string())
        );
        assert!(status.has_api_key);
        assert_eq!(status.masked_api_key, Some("sk...alue".to_string()));
    }
}
