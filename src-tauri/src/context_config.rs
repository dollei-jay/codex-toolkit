use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item, Table};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntry {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub toml_body: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntries {
    pub mcp_servers: Vec<ContextEntry>,
    pub skills: Vec<ContextEntry>,
    pub plugins: Vec<ContextEntry>,
    pub config_path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntryInput {
    pub kind: String,
    pub id: String,
    pub toml_body: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextToggleInput {
    pub kind: String,
    pub id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDeleteInput {
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextApplyResult {
    pub entries: ContextEntries,
    pub backup_path: Option<String>,
    pub message: String,
}

pub fn list_context_entries_from_default() -> Result<ContextEntries, String> {
    let home = codex_home_dir()?;
    let config_path = home.join("config.toml");
    let contents = fs::read_to_string(&config_path).unwrap_or_default();
    let doc = parse_config_or_empty(&contents)?;
    Ok(entries_from_doc(&doc, &config_path))
}

pub fn upsert_context_entry_default(
    input: ContextEntryInput,
) -> Result<ContextApplyResult, String> {
    let home = codex_home_dir()?;
    edit_context_config(&home, |doc| {
        upsert_context_entry(doc, &input.kind, &input.id, &input.toml_body)
    })
}

pub fn toggle_context_entry_default(
    input: ContextToggleInput,
) -> Result<ContextApplyResult, String> {
    let home = codex_home_dir()?;
    edit_context_config(&home, |doc| {
        toggle_context_entry(doc, &input.kind, &input.id, input.enabled)
    })
}

pub fn delete_context_entry_default(
    input: ContextDeleteInput,
) -> Result<ContextApplyResult, String> {
    let home = codex_home_dir()?;
    edit_context_config(&home, |doc| {
        delete_context_entry(doc, &input.kind, &input.id)
    })
}

fn edit_context_config<F>(home: &Path, edit: F) -> Result<ContextApplyResult, String>
where
    F: FnOnce(&mut DocumentMut) -> Result<String, String>,
{
    fs::create_dir_all(home).map_err(|error| error.to_string())?;
    let config_path = home.join("config.toml");
    let contents = fs::read_to_string(&config_path).unwrap_or_default();
    let mut doc = parse_config_or_empty(&contents)?;
    let message = edit(&mut doc)?;
    let backup_path = backup_config_if_exists(&config_path)?;
    fs::write(&config_path, ensure_trailing_newline(doc.to_string()))
        .map_err(|error| error.to_string())?;
    Ok(ContextApplyResult {
        entries: entries_from_doc(&doc, &config_path),
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        message,
    })
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

fn parse_config_or_empty(contents: &str) -> Result<DocumentMut, String> {
    if contents.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        contents
            .parse::<DocumentMut>()
            .map_err(|_| "Codex config.toml is invalid.".to_string())
    }
}

fn entries_from_doc(doc: &DocumentMut, config_path: &Path) -> ContextEntries {
    ContextEntries {
        mcp_servers: list_entries_for_table(doc, "mcp_servers"),
        skills: list_entries_for_table(doc, "skills"),
        plugins: list_entries_for_table(doc, "plugins"),
        config_path: config_path.to_string_lossy().to_string(),
    }
}

fn list_entries_for_table(doc: &DocumentMut, table_name: &str) -> Vec<ContextEntry> {
    let Some(table) = doc.get(table_name).and_then(Item::as_table) else {
        return Vec::new();
    };
    table
        .iter()
        .filter_map(|(id, item)| {
            let table = item.as_table()?;
            let body = table_body_to_string(table);
            Some(ContextEntry {
                id: id.to_string(),
                kind: context_kind_name(table_name).to_string(),
                title: id.to_string(),
                summary: context_entry_summary(&body),
                toml_body: body,
                enabled: context_entry_enabled(table),
            })
        })
        .collect()
}

fn upsert_context_entry(
    doc: &mut DocumentMut,
    kind: &str,
    id: &str,
    toml_body: &str,
) -> Result<String, String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("Entry ID cannot be empty.".to_string());
    }
    validate_entry_id(id)?;
    let table_name = context_table_name(kind)?;
    let body_doc = parse_config_or_empty(toml_body)?;
    if !doc.as_table().contains_key(table_name) {
        doc[table_name] = toml_edit::table();
    }
    if doc[table_name].as_table().is_none() {
        return Err(format!("{table_name} must be a TOML table."));
    }
    doc[table_name][id] = Item::Table(body_doc.as_table().clone());
    Ok("Tool/plugin entry saved.".to_string())
}

fn toggle_context_entry(
    doc: &mut DocumentMut,
    kind: &str,
    id: &str,
    enabled: bool,
) -> Result<String, String> {
    let table_name = context_table_name(kind)?;
    let id = id.trim();
    let table = doc
        .get_mut(table_name)
        .and_then(Item::as_table_mut)
        .and_then(|items| items.get_mut(id))
        .and_then(Item::as_table_mut)
        .ok_or_else(|| "Entry was not found.".to_string())?;
    table["enabled"] = toml_edit::value(enabled);
    table.remove("disabled");
    Ok(if enabled {
        "Tool/plugin entry enabled.".to_string()
    } else {
        "Tool/plugin entry disabled.".to_string()
    })
}

fn delete_context_entry(doc: &mut DocumentMut, kind: &str, id: &str) -> Result<String, String> {
    let table_name = context_table_name(kind)?;
    if let Some(table) = doc.get_mut(table_name).and_then(Item::as_table_mut) {
        table.remove(id.trim());
        if table.is_empty() {
            doc.as_table_mut().remove(table_name);
        }
    }
    Ok("Tool/plugin entry deleted.".to_string())
}

fn context_table_name(kind: &str) -> Result<&'static str, String> {
    match kind {
        "mcp" | "mcpServer" | "mcpServers" | "mcp_servers" => Ok("mcp_servers"),
        "skill" | "skills" => Ok("skills"),
        "plugin" | "plugins" => Ok("plugins"),
        _ => Err("Unknown entry type.".to_string()),
    }
}

fn context_kind_name(table: &str) -> &'static str {
    match table {
        "mcp_servers" => "mcp",
        "skills" => "skill",
        "plugins" => "plugin",
        _ => "unknown",
    }
}

fn context_entry_enabled(table: &Table) -> bool {
    if table
        .get("enabled")
        .and_then(|value| value.as_bool())
        .is_some_and(|enabled| !enabled)
    {
        return false;
    }
    if table
        .get("disabled")
        .and_then(|value| value.as_bool())
        .is_some_and(|disabled| disabled)
    {
        return false;
    }
    true
}

fn context_entry_summary(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("")
        .chars()
        .take(96)
        .collect()
}

fn table_body_to_string(table: &Table) -> String {
    let mut doc = DocumentMut::new();
    for (key, value) in table.iter() {
        doc[key] = value.clone();
    }
    normalize_optional_toml(doc.to_string())
}

fn normalize_optional_toml(contents: String) -> String {
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        ensure_trailing_newline(trimmed.to_string())
    }
}

fn validate_entry_id(id: &str) -> Result<(), String> {
    if id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.'))
    {
        Ok(())
    } else {
        Err("Entry ID can only contain letters, numbers, dot, underscore and hyphen.".to_string())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_plugin_entries_and_enabled_state() {
        let config = r#"[plugins.demo]
path = "C:/demo"
enabled = false
"#;
        let doc = parse_config_or_empty(config).unwrap();
        let entries = entries_from_doc(&doc, Path::new("config.toml"));

        assert_eq!(entries.plugins.len(), 1);
        assert_eq!(entries.plugins[0].id, "demo");
        assert!(!entries.plugins[0].enabled);
        assert!(entries.plugins[0].toml_body.contains("path = \"C:/demo\""));
    }

    #[test]
    fn upserts_and_toggles_plugin_entry() {
        let mut doc = DocumentMut::new();
        upsert_context_entry(&mut doc, "plugin", "demo", "path = \"C:/demo\"\n").unwrap();
        toggle_context_entry(&mut doc, "plugin", "demo", false).unwrap();
        let contents = doc.to_string();

        assert!(contents.contains("[plugins.demo]"));
        assert!(contents.contains("path = \"C:/demo\""));
        assert!(contents.contains("enabled = false"));
    }

    #[test]
    fn delete_removes_empty_context_table() {
        let mut doc = parse_config_or_empty("[skills.demo]\npath = \"x\"\n").unwrap();
        delete_context_entry(&mut doc, "skill", "demo").unwrap();

        assert!(!doc.to_string().contains("skills"));
    }
}
