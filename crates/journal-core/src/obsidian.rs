//! Reads the daily-note folder and format from a vault's Obsidian plugin settings.

use crate::{LogError, NoteLayout};
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Which Obsidian plugin defines the daily notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteSource {
    /// The Periodic Notes community plugin.
    Periodic,
    /// The core Daily notes plugin.
    Daily,
}

impl NoteSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Periodic => "Periodic Notes",
            Self::Daily => "Daily notes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLayout {
    pub source: NoteSource,
    pub layout: NoteLayout,
}

impl ResolvedLayout {
    /// E.g. `Periodic Notes: daily/YYYY/YYYY-MM/YYYY-MM-DD`.
    pub fn describe(&self) -> String {
        format!("{}: {}", self.source.label(), self.layout.describe())
    }
}

const PERIODIC_ID: &str = "periodic-notes";
const DAILY_ID: &str = "daily-notes";

struct DailyConfig {
    folder: String,
    format: String,
}

fn config_dir(root: &Path) -> Result<PathBuf, LogError> {
    let dir = root.join(".obsidian");
    match fs::metadata(&dir) {
        Ok(metadata) if metadata.is_dir() => Ok(dir),
        Ok(_) => Err(LogError::NotAVault {
            root: root.to_owned(),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(LogError::NotAVault {
            root: root.to_owned(),
        }),
        Err(error) => Err(LogError::PluginSettings {
            path: dir,
            detail: error.to_string(),
        }),
    }
}

fn invalid(path: &Path, detail: impl Into<String>) -> LogError {
    LogError::PluginSettings {
        path: path.to_owned(),
        detail: detail.into(),
    }
}

/// Parsed JSON, or `None` if the file does not exist.
fn read_json(path: &Path) -> Result<Option<Value>, LogError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(invalid(path, error.to_string())),
    };
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
    serde_json::from_slice(bytes)
        .map(Some)
        .map_err(|error| invalid(path, format!("invalid JSON ({error})")))
}

fn optional_string(path: &Path, config: &Value, key: &str) -> Result<String, LogError> {
    match config.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(invalid(path, format!("\"{key}\" must be text"))),
    }
}

fn daily_config(path: &Path, config: &Value) -> Result<DailyConfig, LogError> {
    Ok(DailyConfig {
        folder: optional_string(path, config, "folder")?,
        format: optional_string(path, config, "format")?,
    })
}

/// Periodic Notes' daily settings if the plugin is enabled with daily notes turned on.
fn periodic(dir: &Path) -> Result<Option<DailyConfig>, LogError> {
    let list = dir.join("community-plugins.json");
    let enabled = match read_json(&list)? {
        None => false,
        Some(Value::Array(ids)) => ids.iter().any(|id| id.as_str() == Some(PERIODIC_ID)),
        Some(_) => return Err(invalid(&list, "expected a list of plugin IDs")),
    };
    if !enabled {
        return Ok(None);
    }
    let path = dir.join("plugins").join(PERIODIC_ID).join("data.json");
    let Some(settings) = read_json(&path)? else {
        return Ok(None);
    };
    if !settings.is_object() {
        return Err(invalid(&path, "expected a JSON object"));
    }
    // Version 1.0 keeps settings in calendar sets; 0.x has a top-level "daily" object.
    let daily = match settings.get("calendarSets") {
        Some(Value::Array(sets)) => {
            let active = settings.get("activeCalendarSet").and_then(Value::as_str);
            let set = sets
                .iter()
                .find(|set| set.get("id").and_then(Value::as_str) == active)
                .ok_or_else(|| invalid(&path, "the active calendar set was not found"))?;
            set.get("day")
        }
        Some(_) => return Err(invalid(&path, "\"calendarSets\" must be a list")),
        None => settings.get("daily"),
    };
    let Some(daily) = daily.filter(|daily| !daily.is_null()) else {
        return Ok(None);
    };
    if !daily.is_object() {
        return Err(invalid(
            &path,
            "the daily note settings must be a JSON object",
        ));
    }
    match daily.get("enabled") {
        Some(Value::Bool(true)) => daily_config(&path, daily).map(Some),
        None | Some(Value::Bool(false)) | Some(Value::Null) => Ok(None),
        Some(_) => Err(invalid(&path, "\"enabled\" must be true or false")),
    }
}

/// Core Daily notes settings if the plugin is enabled.
fn core_daily(dir: &Path) -> Result<Option<DailyConfig>, LogError> {
    let list = dir.join("core-plugins.json");
    let enabled = match read_json(&list)? {
        None => false,
        Some(Value::Object(plugins)) => match plugins.get(DAILY_ID) {
            None | Some(Value::Bool(false)) => false,
            Some(Value::Bool(true)) => true,
            Some(_) => return Err(invalid(&list, "\"daily-notes\" must be true or false")),
        },
        Some(Value::Array(ids)) => ids.iter().any(|id| id.as_str() == Some(DAILY_ID)),
        Some(_) => return Err(invalid(&list, "expected a JSON object or list")),
    };
    if !enabled {
        return Ok(None);
    }
    let path = dir.join("daily-notes.json");
    match read_json(&path)? {
        None => Ok(Some(DailyConfig {
            folder: String::new(),
            format: String::new(),
        })),
        Some(settings @ Value::Object(_)) => daily_config(&path, &settings).map(Some),
        Some(_) => Err(invalid(&path, "expected a JSON object")),
    }
}

fn read(dir: &Path, source: NoteSource) -> Result<Option<DailyConfig>, LogError> {
    match source {
        NoteSource::Periodic => periodic(dir),
        NoteSource::Daily => core_daily(dir),
    }
}

/// The plugin to suggest for a vault: Periodic Notes if its daily notes are on (it takes
/// over daily-note commands), else core Daily notes if enabled, else `None`.
pub fn detect_source(root: &Path) -> Result<Option<NoteSource>, LogError> {
    let dir = config_dir(root)?;
    for source in [NoteSource::Periodic, NoteSource::Daily] {
        if read(&dir, source)?.is_some() {
            return Ok(Some(source));
        }
    }
    Ok(None)
}

/// The daily-note layout defined by `source`, which must be enabled in the vault.
pub fn resolve_layout(root: &Path, source: NoteSource) -> Result<ResolvedLayout, LogError> {
    let dir = config_dir(root)?;
    let config = read(&dir, source)?.ok_or(LogError::PluginDisabled {
        plugin: match source {
            NoteSource::Periodic => "Periodic Notes (with daily notes turned on)",
            NoteSource::Daily => "The Daily notes core plugin",
        },
    })?;
    Ok(ResolvedLayout {
        source,
        layout: NoteLayout::new(&config.folder, &config.format)?,
    })
}
