use chrono::Utc;
use filetime::{set_file_mtime, FileTime};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const DEFAULT_PROVIDER: &str = "openai";
const BACKUP_NAMESPACE: &str = "codex-toolkit-history-sync";
const DB_FILE_BASENAME: &str = "state_5.sqlite";
const SESSION_DIRS: [&str; 2] = ["sessions", "archived_sessions"];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySyncStatus {
    pub codex_home: String,
    pub current_provider: String,
    pub current_provider_implicit: bool,
    pub rollout_counts: ScopeCounts,
    pub sqlite_counts: Option<ScopeCounts>,
    pub provider_summaries: Vec<ProviderHistorySummary>,
    pub sqlite_error: Option<String>,
    pub backup_root: String,
    pub backup_count: usize,
    pub pending_rollout_files: usize,
    pub pending_sqlite_rows: usize,
    pub encrypted_content_warning: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySyncResult {
    pub codex_home: String,
    pub target_provider: String,
    pub backup_dir: String,
    pub changed_session_files: usize,
    pub sqlite_rows_updated: usize,
    pub sqlite_present: bool,
    pub encrypted_content_warning: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHistorySummary {
    pub provider: String,
    pub is_current: bool,
    pub rollout_sessions: usize,
    pub rollout_archived_sessions: usize,
    pub sqlite_sessions: usize,
    pub sqlite_archived_sessions: usize,
    pub total_rollout: usize,
    pub total_sqlite: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCounts {
    pub sessions: BTreeMap<String, usize>,
    pub archived_sessions: BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
struct SessionChange {
    path: PathBuf,
    thread_id: Option<String>,
    cwd: Option<String>,
    original_first_line: String,
    original_separator: String,
    updated_first_line: String,
    original_mtime: FileTime,
    directory: String,
    original_provider: String,
}

#[derive(Clone, Debug)]
struct FirstLineRecord {
    first_line: String,
    separator: String,
    offset: u64,
}

pub fn history_sync_status_default() -> Result<HistorySyncStatus, String> {
    let codex_home = codex_home_dir()?;
    let current = read_current_provider(&codex_home)?;
    let configured_providers = read_configured_providers(&codex_home)?;
    let scan = collect_session_changes(&codex_home, &current.provider)?;
    let (sqlite_counts, sqlite_error, pending_sqlite_rows) =
        read_sqlite_status(&codex_home, &current.provider)?;
    let backup_root = backup_root(&codex_home);
    let provider_summaries = build_provider_summaries(
        &scan.provider_counts,
        sqlite_counts.as_ref(),
        &current.provider,
        &configured_providers,
    );

    Ok(HistorySyncStatus {
        codex_home: codex_home.to_string_lossy().to_string(),
        current_provider: current.provider.clone(),
        current_provider_implicit: current.implicit,
        rollout_counts: scan.provider_counts,
        sqlite_counts,
        provider_summaries,
        sqlite_error,
        backup_root: backup_root.to_string_lossy().to_string(),
        backup_count: managed_backup_count(&backup_root)?,
        pending_rollout_files: scan.changes.len(),
        pending_sqlite_rows,
        encrypted_content_warning: build_encrypted_warning(
            &scan.encrypted_counts,
            &current.provider,
        ),
    })
}

pub fn sync_history_to_provider_default(
    provider: Option<String>,
) -> Result<HistorySyncResult, String> {
    let codex_home = codex_home_dir()?;
    let current = read_current_provider(&codex_home)?;
    let target_provider = provider
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or(current.provider);

    if !is_valid_provider_id(&target_provider) {
        return Err(
            "Provider ID can only contain letters, numbers, underscore and hyphen.".to_string(),
        );
    }

    let lock_path = acquire_lock(&codex_home)?;
    let result = sync_history_locked(&codex_home, &target_provider);
    let _ = fs::remove_file(lock_path);
    result
}

fn sync_history_locked(
    codex_home: &Path,
    target_provider: &str,
) -> Result<HistorySyncResult, String> {
    let scan = collect_session_changes(codex_home, target_provider)?;
    assert_sqlite_writable(codex_home)?;
    let backup_dir = create_backup(codex_home, target_provider, &scan.changes)?;
    let encrypted_content_warning =
        build_encrypted_warning(&scan.encrypted_counts, target_provider);

    let mut applied_changes = Vec::new();
    match apply_session_changes(&scan.changes, &mut applied_changes)
        .and_then(|changed_files| {
            update_sqlite_threads(codex_home, target_provider, &scan).and_then(
                |(sqlite_rows_updated, sqlite_present)| {
                    sync_global_state_workspace_roots(codex_home, &scan.workspace_roots)
                        .map(|_| (changed_files, sqlite_rows_updated, sqlite_present))
                },
            )
        }) {
        Ok((changed_files, sqlite_rows_updated, sqlite_present)) => Ok(HistorySyncResult {
            codex_home: codex_home.to_string_lossy().to_string(),
            target_provider: target_provider.to_string(),
            backup_dir: backup_dir.to_string_lossy().to_string(),
            changed_session_files: changed_files,
            sqlite_rows_updated,
            sqlite_present,
            encrypted_content_warning,
            message: format!(
                "History synced to {target_provider}. Updated {changed_files} session file(s) and {sqlite_rows_updated} SQLite row(s)."
            ),
        }),
        Err(error) => {
            let _ = restore_session_changes(&applied_changes);
            Err(error)
        }
    }
}

struct CurrentProvider {
    provider: String,
    implicit: bool,
}

struct SessionScan {
    changes: Vec<SessionChange>,
    provider_counts: ScopeCounts,
    encrypted_counts: ScopeCounts,
    thread_cwd_by_id: BTreeMap<String, String>,
    workspace_roots: Vec<String>,
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

fn backup_root(codex_home: &Path) -> PathBuf {
    codex_home
        .join("backups_state")
        .join("toolkit-history-sync")
}

fn read_current_provider(codex_home: &Path) -> Result<CurrentProvider, String> {
    let config_path = codex_home.join("config.toml");
    let contents = fs::read_to_string(config_path).unwrap_or_default();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            break;
        }
        if let Some(value) = parse_quoted_toml_value(trimmed, "model_provider") {
            return Ok(CurrentProvider {
                provider: value,
                implicit: false,
            });
        }
    }

    Ok(CurrentProvider {
        provider: DEFAULT_PROVIDER.to_string(),
        implicit: true,
    })
}

fn read_configured_providers(codex_home: &Path) -> Result<Vec<String>, String> {
    let config_path = codex_home.join("config.toml");
    let contents = fs::read_to_string(config_path).unwrap_or_default();
    let mut providers = Vec::new();
    push_unique_provider(&mut providers, DEFAULT_PROVIDER.to_string());
    for line in contents.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("[model_providers.") else {
            continue;
        };
        let Some(provider) = rest.strip_suffix(']') else {
            continue;
        };
        let provider = provider.trim();
        if !provider.is_empty() {
            push_unique_provider(&mut providers, provider.to_string());
        }
    }
    Ok(providers)
}

fn push_unique_provider(providers: &mut Vec<String>, provider: String) {
    if providers.iter().any(|existing| existing == &provider) {
        return;
    }
    providers.push(provider);
}

fn parse_quoted_toml_value(line: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} =");
    if !line.starts_with(&prefix) {
        return None;
    }
    let value = line[prefix.len()..].trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return None;
    }
    Some(value[1..value.len() - 1].to_string())
}

fn collect_session_changes(
    codex_home: &Path,
    target_provider: &str,
) -> Result<SessionScan, String> {
    let mut changes = Vec::new();
    let mut provider_counts = ScopeCounts::default();
    let mut encrypted_counts = ScopeCounts::default();
    let mut thread_cwd_by_id = BTreeMap::new();
    let mut workspace_roots = Vec::new();

    for directory in SESSION_DIRS {
        let root = codex_home.join(directory);
        if !root.exists() {
            continue;
        }

        let mut files = Vec::new();
        visit_rollout_files(&root, &mut files)?;
        for path in files {
            let record = read_first_line_record(&path)?;
            let Some(mut parsed) = parse_session_meta(&record.first_line) else {
                continue;
            };
            let payload = parsed
                .get_mut("payload")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "Session metadata payload is invalid.".to_string())?;
            let current_provider = payload
                .get("model_provider")
                .and_then(Value::as_str)
                .unwrap_or("(missing)")
                .to_string();
            let thread_id = payload
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(to_desktop_workspace_path);
            if let (Some(thread_id), Some(cwd)) = (&thread_id, &cwd) {
                thread_cwd_by_id.insert(thread_id.clone(), cwd.clone());
                push_unique_path(&mut workspace_roots, cwd.clone());
            }
            increment_scope_count(&mut provider_counts, directory, &current_provider);

            if file_contains_text(&path, "\"encrypted_content\"", record.offset)? {
                increment_scope_count(&mut encrypted_counts, directory, &current_provider);
            }

            if current_provider != target_provider {
                let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
                payload.insert("model_provider".to_string(), json!(target_provider));
                changes.push(SessionChange {
                    path,
                    thread_id,
                    cwd,
                    original_first_line: record.first_line,
                    original_separator: record.separator,
                    updated_first_line: serde_json::to_string(&parsed)
                        .map_err(|error| error.to_string())?,
                    original_mtime: FileTime::from_last_modification_time(&metadata),
                    directory: directory.to_string(),
                    original_provider: current_provider,
                });
            }
        }
    }

    Ok(SessionScan {
        changes,
        provider_counts,
        encrypted_counts,
        thread_cwd_by_id,
        workspace_roots,
    })
}

fn visit_rollout_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            visit_rollout_files(&path, files)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
            .unwrap_or(false)
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(())
}

fn read_first_line_record(path: &Path) -> Result<FirstLineRecord, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let bytes = reader
        .read_until(b'\n', &mut line)
        .map_err(|error| error.to_string())?;
    if bytes == 0 {
        return Err(format!("Session file is empty: {}", path.display()));
    }

    let separator = if line.ends_with(b"\r\n") {
        line.truncate(line.len() - 2);
        "\r\n"
    } else if line.ends_with(b"\n") {
        line.truncate(line.len() - 1);
        "\n"
    } else {
        ""
    };

    Ok(FirstLineRecord {
        first_line: String::from_utf8(line).map_err(|error| error.to_string())?,
        separator: separator.to_string(),
        offset: bytes as u64,
    })
}

fn parse_session_meta(line: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str) == Some("session_meta") {
        Some(value)
    } else {
        None
    }
}

fn file_contains_text(path: &Path, needle: &str, start: u64) -> Result<bool, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|error| error.to_string())?;
    Ok(contents.contains(needle))
}

fn increment_scope_count(counts: &mut ScopeCounts, directory: &str, provider: &str) {
    let bucket = if directory == "archived_sessions" {
        &mut counts.archived_sessions
    } else {
        &mut counts.sessions
    };
    *bucket.entry(provider.to_string()).or_insert(0) += 1;
}

fn to_desktop_workspace_path(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(rest) = trimmed.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{}", rest.replace('/', r"\"));
    }
    if let Some(rest) = trimmed.strip_prefix(r"\\?\") {
        return rest.replace('/', r"\");
    }
    value.to_string()
}

fn normalize_comparable_path(value: &str) -> String {
    let mut normalized = to_desktop_workspace_path(value).trim().replace('/', r"\");
    while normalized.ends_with('\\') && normalized.len() > 3 {
        normalized.pop();
    }
    normalized.to_lowercase()
}

fn push_unique_path(paths: &mut Vec<String>, path: String) {
    let comparable = normalize_comparable_path(&path);
    if paths
        .iter()
        .any(|existing| normalize_comparable_path(existing) == comparable)
    {
        return;
    }
    paths.push(path);
}

fn build_provider_summaries(
    rollout_counts: &ScopeCounts,
    sqlite_counts: Option<&ScopeCounts>,
    current_provider: &str,
    configured_providers: &[String],
) -> Vec<ProviderHistorySummary> {
    let mut providers = BTreeMap::<String, ()>::new();
    for provider in configured_providers {
        providers.insert(provider.clone(), ());
    }
    collect_provider_keys(&mut providers, rollout_counts);
    if let Some(sqlite_counts) = sqlite_counts {
        collect_provider_keys(&mut providers, sqlite_counts);
    }

    let mut summaries: Vec<ProviderHistorySummary> = providers
        .keys()
        .map(|provider| {
            let rollout_sessions = *rollout_counts.sessions.get(provider).unwrap_or(&0);
            let rollout_archived_sessions =
                *rollout_counts.archived_sessions.get(provider).unwrap_or(&0);
            let sqlite_sessions = sqlite_counts
                .and_then(|counts| counts.sessions.get(provider))
                .copied()
                .unwrap_or(0);
            let sqlite_archived_sessions = sqlite_counts
                .and_then(|counts| counts.archived_sessions.get(provider))
                .copied()
                .unwrap_or(0);
            let total_rollout = rollout_sessions + rollout_archived_sessions;
            let total_sqlite = sqlite_sessions + sqlite_archived_sessions;
            ProviderHistorySummary {
                provider: provider.clone(),
                is_current: provider == current_provider,
                rollout_sessions,
                rollout_archived_sessions,
                sqlite_sessions,
                sqlite_archived_sessions,
                total_rollout,
                total_sqlite,
                total: total_rollout + total_sqlite,
            }
        })
        .collect();

    summaries.sort_by(|left, right| {
        right
            .is_current
            .cmp(&left.is_current)
            .then_with(|| right.total.cmp(&left.total))
            .then_with(|| left.provider.cmp(&right.provider))
    });
    summaries
}

fn collect_provider_keys(providers: &mut BTreeMap<String, ()>, counts: &ScopeCounts) {
    for provider in counts
        .sessions
        .keys()
        .chain(counts.archived_sessions.keys())
    {
        providers.insert(provider.clone(), ());
    }
}

fn read_sqlite_status(
    codex_home: &Path,
    target_provider: &str,
) -> Result<(Option<ScopeCounts>, Option<String>, usize), String> {
    let db_path = codex_home.join(DB_FILE_BASENAME);
    if !db_path.exists() {
        return Ok((None, None, 0));
    }

    let connection = match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(connection) => connection,
        Err(error) => return Ok((None, Some(format_sqlite_error(error)), 0)),
    };

    let counts = read_sqlite_counts_from_connection(&connection)?;
    let pending = connection
        .query_row(
            "SELECT COUNT(*) FROM threads WHERE COALESCE(model_provider, '') <> ?1",
            [target_provider],
            |row| row.get::<_, usize>(0),
        )
        .map_err(format_sqlite_error)?;

    Ok((Some(counts), None, pending))
}

fn read_sqlite_counts_from_connection(connection: &Connection) -> Result<ScopeCounts, String> {
    let mut statement = connection
        .prepare(
            "SELECT CASE WHEN model_provider IS NULL OR model_provider = '' THEN '(missing)' ELSE model_provider END, archived, COUNT(*) FROM threads GROUP BY model_provider, archived",
        )
        .map_err(format_sqlite_error)?;
    let mut rows = statement.query([]).map_err(format_sqlite_error)?;
    let mut counts = ScopeCounts::default();
    while let Some(row) = rows.next().map_err(format_sqlite_error)? {
        let provider: String = row.get(0).map_err(format_sqlite_error)?;
        let archived: i64 = row.get(1).map_err(format_sqlite_error)?;
        let count: usize = row.get(2).map_err(format_sqlite_error)?;
        let bucket = if archived != 0 {
            &mut counts.archived_sessions
        } else {
            &mut counts.sessions
        };
        bucket.insert(provider, count);
    }
    Ok(counts)
}

fn assert_sqlite_writable(codex_home: &Path) -> Result<(), String> {
    let db_path = codex_home.join(DB_FILE_BASENAME);
    if !db_path.exists() {
        return Ok(());
    }
    let connection = Connection::open(&db_path).map_err(format_sqlite_error)?;
    connection
        .execute_batch("PRAGMA busy_timeout = 5000; BEGIN IMMEDIATE; ROLLBACK;")
        .map_err(|error| {
            format!(
                "Unable to update session provider metadata because state_5.sqlite is currently in use or unreadable. Original error: {}",
                error
            )
        })
}

fn update_sqlite_threads(
    codex_home: &Path,
    target_provider: &str,
    scan: &SessionScan,
) -> Result<(usize, bool), String> {
    let db_path = codex_home.join(DB_FILE_BASENAME);
    if !db_path.exists() {
        return Ok((0, false));
    }

    let mut connection = Connection::open(&db_path).map_err(format_sqlite_error)?;
    connection
        .execute_batch("PRAGMA busy_timeout = 5000;")
        .map_err(format_sqlite_error)?;
    let transaction = connection.transaction().map_err(format_sqlite_error)?;
    let changed = transaction
        .execute(
            "UPDATE threads SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1",
            [target_provider],
        )
        .map_err(format_sqlite_error)?;
    let mut total_changed = changed;
    if table_has_column(&transaction, "threads", "cwd")? {
        let mut statement = transaction
            .prepare("UPDATE threads SET cwd = ?1 WHERE id = ?2 AND COALESCE(cwd, '') <> ?1")
            .map_err(format_sqlite_error)?;
        for (thread_id, cwd) in &scan.thread_cwd_by_id {
            total_changed += statement
                .execute([cwd.as_str(), thread_id.as_str()])
                .map_err(format_sqlite_error)?;
        }
    }
    if table_has_column(&transaction, "threads", "has_user_event")? {
        let mut statement = transaction
            .prepare(
                "UPDATE threads SET has_user_event = 1 WHERE id = ?1 AND COALESCE(has_user_event, 0) <> 1",
            )
            .map_err(format_sqlite_error)?;
        for thread_id in scan.thread_cwd_by_id.keys() {
            total_changed += statement
                .execute([thread_id.as_str()])
                .map_err(format_sqlite_error)?;
        }
    }
    transaction.commit().map_err(format_sqlite_error)?;
    Ok((total_changed, true))
}

fn table_has_column(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!(
            "PRAGMA table_info({})",
            quote_identifier(table_name)
        ))
        .map_err(format_sqlite_error)?;
    let mut rows = statement.query([]).map_err(format_sqlite_error)?;
    while let Some(row) = rows.next().map_err(format_sqlite_error)? {
        let name: String = row.get(1).map_err(format_sqlite_error)?;
        if name == column_name {
            return Ok(true);
        }
    }
    Ok(false)
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn create_backup(
    codex_home: &Path,
    target_provider: &str,
    changes: &[SessionChange],
) -> Result<PathBuf, String> {
    let backup_dir =
        backup_root(codex_home).join(Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string());
    fs::create_dir_all(backup_dir.join("db")).map_err(|error| error.to_string())?;

    for suffix in ["", "-wal", "-shm"] {
        let file_name = format!("{DB_FILE_BASENAME}{suffix}");
        let source = codex_home.join(&file_name);
        if source.exists() {
            fs::copy(&source, backup_dir.join("db").join(&file_name))
                .map_err(|error| error.to_string())?;
        }
    }

    copy_if_present(
        &codex_home.join("config.toml"),
        &backup_dir.join("config.toml"),
    )?;
    copy_if_present(
        &codex_home.join(".codex-global-state.json"),
        &backup_dir.join(".codex-global-state.json"),
    )?;
    copy_if_present(
        &codex_home.join(".codex-global-state.backup.json"),
        &backup_dir.join(".codex-global-state.backup.json"),
    )?;

    let files: Vec<Value> = changes
        .iter()
        .map(|change| {
            json!({
                "path": change.path,
                "threadId": change.thread_id,
                "cwd": change.cwd,
                "directory": change.directory,
                "originalProvider": change.original_provider,
                "originalFirstLine": change.original_first_line,
                "originalSeparator": change.original_separator,
                "originalMtimeUnixSeconds": change.original_mtime.unix_seconds(),
                "originalMtimeNanoseconds": change.original_mtime.nanoseconds()
            })
        })
        .collect();

    let metadata = json!({
        "version": 1,
        "namespace": BACKUP_NAMESPACE,
        "codexHome": codex_home,
        "targetProvider": target_provider,
        "createdAt": Utc::now().to_rfc3339(),
        "changedSessionFiles": changes.len()
    });
    let manifest = json!({
        "version": 1,
        "namespace": BACKUP_NAMESPACE,
        "codexHome": codex_home,
        "targetProvider": target_provider,
        "files": files
    });

    fs::write(
        backup_dir.join("metadata.json"),
        serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        backup_dir.join("session-meta-backup.json"),
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    Ok(backup_dir)
}

fn copy_if_present(source: &Path, destination: &Path) -> Result<(), String> {
    if source.exists() {
        fs::copy(source, destination).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn sync_global_state_workspace_roots(codex_home: &Path, roots: &[String]) -> Result<usize, String> {
    let path = codex_home.join(".codex-global-state.json");
    if !path.exists() {
        return Ok(0);
    }

    let original = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut state: Value = serde_json::from_str(&original).map_err(|error| error.to_string())?;
    let mut changed = 0;
    changed += merge_state_path_array(&mut state, "electron-saved-workspace-roots", roots);
    changed += merge_state_path_array(&mut state, "project-order", roots);

    if changed > 0 {
        let next = format!(
            "{}\n",
            serde_json::to_string_pretty(&state).map_err(|error| error.to_string())?
        );
        fs::write(&path, &next).map_err(|error| error.to_string())?;
        fs::write(codex_home.join(".codex-global-state.backup.json"), next)
            .map_err(|error| error.to_string())?;
    }

    Ok(changed)
}

fn merge_state_path_array(state: &mut Value, key: &str, roots: &[String]) -> usize {
    let Some(object) = state.as_object_mut() else {
        return 0;
    };
    let entry = object
        .entry(key.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !entry.is_array() {
        *entry = Value::Array(path_values_from_roots(roots));
        return roots.len();
    }

    let array = entry.as_array_mut().expect("array checked above");
    let mut changed = 0;
    for root in roots {
        if !array_has_path(array, root) {
            array.push(Value::String(root.clone()));
            changed += 1;
        }
    }
    changed
}

fn path_values_from_roots(roots: &[String]) -> Vec<Value> {
    roots.iter().cloned().map(Value::String).collect()
}

fn array_has_path(array: &[Value], path: &str) -> bool {
    let target = normalize_comparable_path(path);
    array
        .iter()
        .filter_map(Value::as_str)
        .any(|value| normalize_comparable_path(value) == target)
}

fn apply_session_changes(
    changes: &[SessionChange],
    applied_changes: &mut Vec<SessionChange>,
) -> Result<usize, String> {
    for change in changes {
        rewrite_first_line(
            &change.path,
            &change.updated_first_line,
            &change.original_separator,
        )?;
        set_file_mtime(&change.path, change.original_mtime).map_err(|error| error.to_string())?;
        applied_changes.push(change.clone());
    }
    Ok(applied_changes.len())
}

fn restore_session_changes(changes: &[SessionChange]) -> Result<(), String> {
    for change in changes.iter().rev() {
        rewrite_first_line(
            &change.path,
            &change.original_first_line,
            &change.original_separator,
        )?;
        set_file_mtime(&change.path, change.original_mtime).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn rewrite_first_line(path: &Path, first_line: &str, separator: &str) -> Result<(), String> {
    let current = read_first_line_record(path)?;
    let tmp_path = path.with_extension(format!(
        "{}.toolkit-history-sync.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("tmp")
    ));
    let mut input = fs::File::open(path).map_err(|error| error.to_string())?;
    input
        .seek(SeekFrom::Start(current.offset))
        .map_err(|error| error.to_string())?;
    let mut output = fs::File::create(&tmp_path).map_err(|error| error.to_string())?;
    output
        .write_all(first_line.as_bytes())
        .map_err(|error| error.to_string())?;
    output
        .write_all(separator.as_bytes())
        .map_err(|error| error.to_string())?;
    std::io::copy(&mut input, &mut output).map_err(|error| error.to_string())?;
    drop(output);
    fs::rename(&tmp_path, path).map_err(|error| {
        let _ = fs::remove_file(&tmp_path);
        error.to_string()
    })
}

fn managed_backup_count(root: &Path) -> Result<usize, String> {
    if !root.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.path().is_dir() {
            continue;
        }
        let metadata_path = entry.path().join("metadata.json");
        let Ok(contents) = fs::read_to_string(metadata_path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&contents) else {
            continue;
        };
        if value.get("namespace").and_then(Value::as_str) == Some(BACKUP_NAMESPACE) {
            count += 1;
        }
    }
    Ok(count)
}

fn acquire_lock(codex_home: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(codex_home).map_err(|error| error.to_string())?;
    let lock_path = codex_home.join(".codex-toolkit-history-sync.lock");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(mut file) => {
            writeln!(file, "{}", std::process::id()).map_err(|error| error.to_string())?;
            Ok(lock_path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(
            "History sync is already running. If it is not, remove .codex-toolkit-history-sync.lock from your Codex home."
                .to_string(),
        ),
        Err(error) => Err(error.to_string()),
    }
}

fn build_encrypted_warning(
    encrypted_counts: &ScopeCounts,
    target_provider: &str,
) -> Option<String> {
    let risky: Vec<String> = encrypted_counts
        .sessions
        .iter()
        .chain(encrypted_counts.archived_sessions.iter())
        .filter(|(provider, count)| **count > 0 && provider.as_str() != target_provider)
        .map(|(provider, _)| provider.clone())
        .collect();

    if risky.is_empty() {
        return None;
    }

    Some(format!(
        "Some histories contain encrypted_content from provider(s) {}. Sync can restore list visibility, but continuing those sessions may still fail.",
        risky.join(", ")
    ))
}

fn is_valid_provider_id(provider_id: &str) -> bool {
    !provider_id.is_empty()
        && provider_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

fn format_sqlite_error(error: rusqlite::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_root_provider() {
        let value = parse_quoted_toml_value(r#"model_provider = "moapi""#, "model_provider");
        assert_eq!(value.as_deref(), Some("moapi"));
    }

    #[test]
    fn parses_session_meta() {
        let line = r#"{"type":"session_meta","payload":{"id":"abc","model_provider":"openai"}}"#;
        let value = parse_session_meta(line).unwrap();
        assert_eq!(value["payload"]["model_provider"].as_str(), Some("openai"));
    }
}
