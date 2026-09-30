use crate::error::AppError;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DuplicateHeading {
    #[default]
    Error,
    First,
    Last,
}

impl From<DuplicateHeading> for journal_core::DuplicateHeading {
    fn from(value: DuplicateHeading) -> Self {
        match value {
            DuplicateHeading::Error => Self::Error,
            DuplicateHeading::First => Self::First,
            DuplicateHeading::Last => Self::Last,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryFormat {
    #[default]
    Inline,
    Block,
}

impl From<EntryFormat> for journal_core::EntryFormat {
    fn from(value: EntryFormat) -> Self {
        match value {
            EntryFormat::Inline => Self::Inline,
            EntryFormat::Block => Self::Block,
        }
    }
}

/// The Obsidian plugin whose settings define where daily notes live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteSource {
    Periodic,
    Daily,
}

impl From<NoteSource> for journal_core::obsidian::NoteSource {
    fn from(value: NoteSource) -> Self {
        match value {
            NoteSource::Periodic => Self::Periodic,
            NoteSource::Daily => Self::Daily,
        }
    }
}

impl From<journal_core::obsidian::NoteSource> for NoteSource {
    fn from(value: journal_core::obsidian::NoteSource) -> Self {
        match value {
            journal_core::obsidian::NoteSource::Periodic => Self::Periodic,
            journal_core::obsidian::NoteSource::Daily => Self::Daily,
        }
    }
}

fn note_source_unset() -> AppError {
    AppError::new(
        "note_source_unset",
        "Choose where daily notes come from (Periodic Notes or Daily notes) in Settings.",
    )
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub vault_root: PathBuf,
    /// Unset until chosen in Settings; finding a note is an error until then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_source: Option<NoteSource>,
    /// Empty appends to the end of the note; otherwise a Markdown heading such as "# Journal".
    #[serde(default)]
    pub heading: String,
    #[serde(default)]
    pub duplicate_heading: DuplicateHeading,
    #[serde(default)]
    pub entry_format: EntryFormat,
    #[serde(default)]
    pub use_yesterday_if_today_missing: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct NoteLabel {
    pub name: String,
    pub is_yesterday: bool,
}

#[derive(Debug, Serialize)]
pub struct Settings {
    pub config_path: String,
    pub use_yesterday_if_today_missing: bool,
    pub entry_format: EntryFormat,
    pub note: Option<NoteLabel>,
}

#[derive(Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsForm {
    pub vault_root: Option<String>,
    pub note_source: Option<NoteSource>,
    pub heading: String,
    pub duplicate_heading: DuplicateHeading,
    pub entry_format: EntryFormat,
    pub use_yesterday_if_today_missing: bool,
}

#[derive(Debug, Serialize)]
pub struct FormResult {
    pub config_path: String,
    pub exists: bool,
    pub form: SettingsForm,
    pub issue: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct HeadingList {
    /// File name (without extension) of the scanned note, if one was found.
    pub note: Option<String>,
    pub headings: Vec<String>,
    pub problem: Option<String>,
    /// The plugin suggested for the vault, used when no source has been chosen yet.
    pub detected: Option<NoteSource>,
    /// The resolved source and pattern, e.g. "Periodic Notes: daily/YYYY/YYYY-MM/YYYY-MM-DD".
    pub layout: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct NoteLink {
    pub name: String,
    /// Vault-relative folder, empty for the vault root.
    pub folder: String,
    /// The text to insert between `[[` and `]]`.
    pub link: String,
}

#[derive(Debug, Default, Serialize)]
pub struct NoteIndex {
    pub notes: Vec<NoteLink>,
    pub problem: Option<String>,
}

fn error(path: &Path, detail: impl std::fmt::Display) -> AppError {
    AppError::new(
        "configuration",
        format!(
            "Cannot use {}: {detail}\nUse the Settings gear to fix it, or edit this JSON file: \
             \"vault_root\" must be the absolute path of an Obsidian vault, \"note_source\" \
             \"periodic\" or \"daily\", \"heading\" empty or a Markdown heading such as \
             \"# Journal\", \"entry_format\" \"inline\" or \"block\", and \
             \"use_yesterday_if_today_missing\" a boolean. \
             Press Enter after correcting it; your draft will be retained.",
            path.display()
        ),
    )
}

fn validate_root(root: &Path) -> Result<(), String> {
    if !root.is_absolute() {
        return Err("vault_root must be a nonempty absolute path".into());
    }
    let metadata =
        fs::metadata(root).map_err(|detail| format!("cannot access vault_root: {detail}"))?;
    if !metadata.is_dir() {
        return Err("vault_root must be an existing directory".into());
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<Config, AppError> {
    let bytes = fs::read(path).map_err(|detail| error(path, detail))?;
    let mut config: Config =
        serde_json::from_slice(&bytes).map_err(|detail| error(path, detail))?;
    validate_root(&config.vault_root).map_err(|detail| error(path, detail))?;
    config.heading = journal_core::validate_heading(&config.heading)
        .map_err(|detail| error(path, detail))?
        .unwrap_or_default();
    Ok(config)
}

fn write(path: &Path, config: &Config) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(config).map_err(|detail| detail.to_string())?;
    bytes.push(b'\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|detail| detail.to_string())?;
    }
    journal_core::atomic_write_or_create(path, &bytes).map_err(|detail| detail.to_string())
}

/// Reads whatever settings are valid so the settings dialog can show them for editing.
pub fn read_form(path: &Path) -> FormResult {
    let mut result = FormResult {
        config_path: path.display().to_string(),
        exists: true,
        form: SettingsForm::default(),
        issue: None,
    };
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(detail) if detail.kind() == std::io::ErrorKind::NotFound => {
            result.exists = false;
            return result;
        }
        Err(detail) => {
            result.issue = Some(format!("Cannot read {}: {detail}", path.display()));
            return result;
        }
    };
    let mut problems = Vec::new();
    match serde_json::from_slice(&bytes) {
        Ok(serde_json::Value::Object(fields)) => {
            for (key, value) in fields {
                match (key.as_str(), value) {
                    ("vault_root", serde_json::Value::String(root)) => {
                        result.form.vault_root = Some(root).filter(|root| !root.is_empty());
                    }
                    ("use_yesterday_if_today_missing", serde_json::Value::Bool(enabled)) => {
                        result.form.use_yesterday_if_today_missing = enabled;
                    }
                    ("heading", serde_json::Value::String(heading)) => {
                        if let Err(detail) = journal_core::validate_heading(&heading) {
                            problems.push(detail);
                        }
                        result.form.heading = heading;
                    }
                    ("duplicate_heading", value @ serde_json::Value::String(_)) => {
                        match serde_json::from_value(value) {
                            Ok(policy) => result.form.duplicate_heading = policy,
                            Err(_) => problems.push(
                                "\"duplicate_heading\" must be \"error\", \"first\", or \"last\""
                                    .into(),
                            ),
                        }
                    }
                    ("entry_format", value @ serde_json::Value::String(_)) => {
                        match serde_json::from_value(value) {
                            Ok(format) => result.form.entry_format = format,
                            Err(_) => problems
                                .push("\"entry_format\" must be \"inline\" or \"block\"".into()),
                        }
                    }
                    ("note_source", value @ serde_json::Value::String(_)) => {
                        match serde_json::from_value(value) {
                            Ok(source) => result.form.note_source = Some(source),
                            Err(_) => problems
                                .push("\"note_source\" must be \"periodic\" or \"daily\"".into()),
                        }
                    }
                    (
                        "vault_root"
                        | "use_yesterday_if_today_missing"
                        | "heading"
                        | "duplicate_heading"
                        | "entry_format"
                        | "note_source",
                        _,
                    ) => {
                        problems.push(format!("\"{key}\" has the wrong type"));
                    }
                    _ => problems.push(format!("unknown setting \"{key}\" will be removed")),
                }
            }
            match &result.form.vault_root {
                Some(root) => {
                    if let Err(detail) = validate_root(Path::new(root)) {
                        problems.push(detail);
                    }
                }
                None => problems.push("vault_root is not set".into()),
            }
        }
        Ok(_) => problems.push("the file is not a JSON object".into()),
        Err(detail) => problems.push(format!("invalid JSON ({detail})")),
    }
    if !problems.is_empty() {
        result.issue = Some(format!(
            "{} has problems: {}. Choose new settings and save to replace it.",
            path.display(),
            problems.join("; ")
        ));
    }
    result
}

/// Headings in the located daily note (today's, else yesterday's) under `vault_root`, using
/// `note_source`, or the detected plugin if no source has been chosen yet.
pub fn list_headings(
    vault_root: Option<&str>,
    note_source: Option<NoteSource>,
    today: NaiveDate,
) -> HeadingList {
    let mut result = HeadingList::default();
    let Some(root) = vault_root.map(Path::new) else {
        return result;
    };
    if validate_root(root).is_err() {
        return result;
    }
    let detected = match journal_core::obsidian::detect_source(root) {
        Ok(detected) => detected.map(NoteSource::from),
        Err(detail) => {
            result.problem = Some(detail.to_string());
            return result;
        }
    };
    result.detected = detected;
    let Some(source) = note_source.or(detected) else {
        result.problem = Some(journal_core::LogError::NoDailyNotesPlugin.to_string());
        return result;
    };
    let resolved = match journal_core::obsidian::resolve_layout(root, source.into()) {
        Ok(resolved) => resolved,
        Err(detail) => {
            result.problem = Some(detail.to_string());
            return result;
        }
    };
    result.layout = Some(resolved.describe());
    let note = match journal_core::locate_daily_note(root, &resolved.layout, today) {
        Ok(Some(note)) => note,
        Ok(None) => return result,
        Err(detail) => {
            result.problem = Some(format!("Could not look for the daily note: {detail}"));
            return result;
        }
    };
    result.note = note
        .path
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned());
    let text = fs::read(&note.path)
        .map_err(|detail| detail.to_string())
        .and_then(|bytes| {
            let body = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
            String::from_utf8(body.to_vec()).map_err(|detail| detail.utf8_error().to_string())
        });
    match text {
        Ok(text) => result.headings = journal_core::list_headings(&text),
        Err(detail) => {
            result.problem = Some(format!(
                "Could not read headings from {}: {detail}",
                note.path.display()
            ))
        }
    }
    result
}

/// The vault's notes for `[[` completion, with link text in the vault's link format. Problems
/// (including configuration errors) leave the list empty or partial instead of failing.
pub fn list_notes(path: &Path, today: NaiveDate) -> NoteIndex {
    let config = match load(path) {
        Ok(config) => config,
        Err(error) => {
            return NoteIndex {
                notes: Vec::new(),
                problem: Some(error.message),
            }
        }
    };
    let scan = match journal_core::vault_index::scan_notes(&config.vault_root) {
        Ok(scan) => scan,
        Err(error) => {
            return NoteIndex {
                notes: Vec::new(),
                problem: Some(error.to_string()),
            }
        }
    };
    let format = journal_core::vault_index::link_format(&config.vault_root);
    let links =
        journal_core::vault_index::link_texts(&scan.notes, format, &config.daily_folder(today));
    NoteIndex {
        notes: scan
            .notes
            .into_iter()
            .zip(links)
            .map(|(note, link)| NoteLink {
                name: note.name,
                folder: note.folder,
                link,
            })
            .collect(),
        problem: scan.truncated.then(|| {
            format!(
                "Only the first {} notes are suggested.",
                journal_core::vault_index::MAX_NOTES
            )
        }),
    }
}

/// Validates the dialog's settings, then creates or replaces the settings file.
pub fn save_form(path: &Path, form: &SettingsForm, today: NaiveDate) -> Result<Settings, AppError> {
    let root = form
        .vault_root
        .as_deref()
        .filter(|root| !root.is_empty())
        .ok_or_else(|| AppError::new("settings", "Choose a vault folder before saving."))?;
    validate_root(Path::new(root)).map_err(|detail| {
        AppError::new(
            "settings",
            format!("Cannot use {root} as the vault folder: {detail}"),
        )
    })?;
    let source = form.note_source.ok_or_else(|| {
        AppError::new(
            "settings",
            "Choose where daily notes come from before saving.",
        )
    })?;
    journal_core::obsidian::resolve_layout(Path::new(root), source.into())
        .map_err(|detail| AppError::new("settings", detail.to_string()))?;
    let heading = journal_core::validate_heading(&form.heading)
        .map_err(|detail| AppError::new("settings", format!("Cannot use this heading: {detail}.")))?
        .unwrap_or_default();
    let config = Config {
        vault_root: PathBuf::from(root),
        note_source: Some(source),
        heading,
        duplicate_heading: form.duplicate_heading,
        entry_format: form.entry_format,
        use_yesterday_if_today_missing: form.use_yesterday_if_today_missing,
    };
    write(path, &config).map_err(|detail| {
        AppError::new(
            "settings",
            format!("Could not save settings to {}: {detail}", path.display()),
        )
    })?;
    Ok(config.settings(path, today))
}

impl Config {
    pub fn settings(&self, path: &Path, today: NaiveDate) -> Settings {
        Settings {
            config_path: path.display().to_string(),
            use_yesterday_if_today_missing: self.use_yesterday_if_today_missing,
            entry_format: self.entry_format,
            note: self.note_label(today),
        }
    }

    pub fn placement(&self) -> journal_core::Placement {
        journal_core::Placement {
            heading: Some(self.heading.clone()).filter(|heading| !heading.is_empty()),
            duplicates: self.duplicate_heading.into(),
        }
    }

    /// Where daily notes live, re-read from the vault's Obsidian settings on every call.
    pub fn layout(&self) -> Result<journal_core::NoteLayout, AppError> {
        let source = self.note_source.ok_or_else(note_source_unset)?;
        Ok(journal_core::obsidian::resolve_layout(&self.vault_root, source.into())?.layout)
    }

    /// The vault-relative folder of the note entries go to (the located note, else today's
    /// expected note), or the vault root if it cannot be determined.
    fn daily_folder(&self, today: NaiveDate) -> String {
        let Ok(layout) = self.layout() else {
            return String::new();
        };
        let date = match journal_core::locate_daily_note(&self.vault_root, &layout, today) {
            Ok(Some(note)) if note.is_yesterday => today.pred_opt().unwrap_or(today),
            _ => today,
        };
        layout
            .relative_path(date)
            .ok()
            .and_then(|path| path.rsplit_once('/').map(|(folder, _)| folder.to_owned()))
            .unwrap_or_default()
    }

    // Inspection failures are displayed as no note; saving reports the underlying error.
    fn note_label(&self, today: NaiveDate) -> Option<NoteLabel> {
        let layout = self.layout().ok()?;
        let note = journal_core::locate_daily_note(&self.vault_root, &layout, today).ok()??;
        Some(NoteLabel {
            name: note.path.file_stem()?.to_string_lossy().into_owned(),
            is_yesterday: note.is_yesterday,
        })
    }
}

pub fn set_fallback(path: &Path, enabled: bool, today: NaiveDate) -> Result<Settings, AppError> {
    let mut config = load(path)?;
    config.use_yesterday_if_today_missing = enabled;
    write(path, &config).map_err(|detail| error(path, detail))?;
    Ok(config.settings(path, today))
}

pub fn set_entry_format(
    path: &Path,
    format: EntryFormat,
    today: NaiveDate,
) -> Result<Settings, AppError> {
    let mut config = load(path)?;
    config.entry_format = format;
    write(path, &config).map_err(|detail| error(path, detail))?;
    Ok(config.settings(path, today))
}

#[cfg(test)]
pub mod test_support {
    use chrono::NaiveDate;
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    /// Makes `root` an Obsidian vault whose Periodic Notes daily notes use
    /// `daily/YYYY/YYYY-MM/YYYY-MM-DD`.
    pub fn fake_vault(root: &Path) {
        let plugin = root
            .join(".obsidian")
            .join("plugins")
            .join("periodic-notes");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(
            root.join(".obsidian").join("community-plugins.json"),
            r#"["periodic-notes"]"#,
        )
        .unwrap();
        fs::write(
            plugin.join("data.json"),
            r#"{"daily": {"enabled": true, "folder": "daily", "format": "YYYY/YYYY-MM/YYYY-MM-DD"}}"#,
        )
        .unwrap();
    }

    pub fn note_path(root: &Path, date: NaiveDate) -> PathBuf {
        journal_core::NoteLayout::new("daily", "YYYY/YYYY-MM/YYYY-MM-DD")
            .unwrap()
            .path(root, date)
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()
    }

    #[test]
    fn settings_label_the_located_note_independently_of_the_fallback() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        write_config(&path, root.path());
        let label = |settings: Settings| settings.note;
        assert_eq!(label(load(&path).unwrap().settings(&path, today())), None);
        let note = |date: NaiveDate| {
            let note = test_support::note_path(root.path(), date);
            fs::create_dir_all(note.parent().unwrap()).unwrap();
            fs::write(note, b"# Journal\n").unwrap();
        };
        note(today().pred_opt().unwrap());
        let yesterday = Some(NoteLabel {
            name: "2026-02-28".into(),
            is_yesterday: true,
        });
        assert_eq!(
            label(load(&path).unwrap().settings(&path, today())),
            yesterday
        );
        assert_eq!(
            label(set_fallback(&path, true, today()).unwrap()),
            yesterday
        );
        note(today());
        assert_eq!(
            label(set_fallback(&path, false, today()).unwrap()),
            Some(NoteLabel {
                name: "2026-03-01".into(),
                is_yesterday: false,
            })
        );
    }

    pub fn write_config(path: &Path, root: &Path) {
        if root.is_dir() {
            test_support::fake_vault(root);
        }
        fs::write(
            path,
            serde_json::to_vec(&serde_json::json!({
                "vault_root": root,
                "note_source": "periodic"
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn defaults_to_unchecked_and_persists_choice_without_changing_root() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        write_config(&path, root.path());
        assert!(!load(&path).unwrap().use_yesterday_if_today_missing);
        set_fallback(&path, true, today()).unwrap();
        let restored = load(&path).unwrap();
        assert!(restored.use_yesterday_if_today_missing);
        assert_eq!(restored.vault_root, root.path());
        set_fallback(&path, false, today()).unwrap();
        assert!(!load(&path).unwrap().use_yesterday_if_today_missing);
    }

    #[test]
    fn rejects_bad_configuration_without_overwriting_it() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        assert!(load(&path)
            .unwrap_err()
            .message
            .contains(&path.display().to_string()));
        assert!(!path.exists());
        for input in [
            r#"{"#,
            r#"{}"#,
            r#"{"vault_root": ""}"#,
            r#"{"vault_root": "relative"}"#,
            r#"{"vault_root": 42}"#,
            r#"{"vault_root": "relative", "use_yesterday_if_today_missing": "yes"}"#,
            r#"{"vault_root": "relative", "typo": true}"#,
        ] {
            fs::write(&path, input).unwrap();
            assert!(load(&path).is_err());
            assert!(set_fallback(&path, true, today()).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), input);
        }
        write_config(&path, &root.path().join("missing"));
        assert!(load(&path).is_err());
        write_config(&path, &path);
        assert!(load(&path).is_err());
    }

    #[test]
    fn preferences_reload_the_current_root_and_report_write_failures() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        write_config(&path, root.path());
        load(&path).unwrap();
        write_config(&path, other.path());
        set_fallback(&path, true, today()).unwrap();
        assert_eq!(load(&path).unwrap().vault_root, other.path());
        let original = fs::metadata(&path).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        let result = set_fallback(&path, false, today());
        fs::set_permissions(&path, original).unwrap();
        assert!(result.is_err());
        assert!(load(&path).unwrap().use_yesterday_if_today_missing);
    }

    fn form(root: Option<&Path>, enabled: bool) -> SettingsForm {
        SettingsForm {
            vault_root: root.map(|root| root.display().to_string()),
            use_yesterday_if_today_missing: enabled,
            ..Default::default()
        }
    }

    /// A savable form for a fake Periodic Notes vault at `root`.
    fn valid_form(root: &Path, enabled: bool) -> SettingsForm {
        test_support::fake_vault(root);
        SettingsForm {
            note_source: Some(NoteSource::Periodic),
            ..form(Some(root), enabled)
        }
    }

    #[test]
    fn read_form_reports_missing_and_valid_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("app").join("config.json");
        let missing = read_form(&path);
        assert!(!missing.exists);
        assert_eq!(missing.form, SettingsForm::default());
        assert_eq!(missing.issue, None);
        assert_eq!(missing.config_path, path.display().to_string());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_config(&path, root.path());
        set_fallback(&path, true, today()).unwrap();
        let valid = read_form(&path);
        assert!(valid.exists);
        assert_eq!(valid.form, valid_form(root.path(), true));
        assert_eq!(valid.issue, None);
    }

    #[test]
    fn read_form_keeps_valid_fields_and_explains_the_rest() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let dir = root.path().display().to_string();
        let missing = root.path().join("missing").display().to_string();
        let cases = [
            ("{", SettingsForm::default(), "invalid JSON"),
            ("[]", SettingsForm::default(), "not a JSON object"),
            ("{}", SettingsForm::default(), "vault_root is not set"),
            (
                r#"{"vault_root": ""}"#,
                SettingsForm::default(),
                "vault_root is not set",
            ),
            (
                r#"{"vault_root": 42, "use_yesterday_if_today_missing": true}"#,
                form(None, true),
                "\"vault_root\" has the wrong type",
            ),
            (
                &format!(r#"{{"vault_root": {dir:?}, "use_yesterday_if_today_missing": "yes"}}"#),
                form(Some(root.path()), false),
                "\"use_yesterday_if_today_missing\" has the wrong type",
            ),
            (
                &format!(r#"{{"vault_root": {dir:?}, "typo": true}}"#),
                form(Some(root.path()), false),
                "unknown setting \"typo\" will be removed",
            ),
            (
                r#"{"vault_root": "relative"}"#,
                SettingsForm {
                    vault_root: Some("relative".into()),
                    use_yesterday_if_today_missing: false,
                    ..Default::default()
                },
                "absolute path",
            ),
            (
                &format!(r#"{{"vault_root": {missing:?}}}"#),
                SettingsForm {
                    vault_root: Some(missing.clone()),
                    use_yesterday_if_today_missing: false,
                    ..Default::default()
                },
                "cannot access vault_root",
            ),
        ];
        for (input, expected, problem) in cases {
            fs::write(&path, input).unwrap();
            let result = read_form(&path);
            assert!(result.exists, "{input}");
            assert_eq!(result.form, expected, "{input}");
            let issue = result.issue.unwrap();
            assert!(issue.contains(problem), "{input}: {issue}");
            assert!(issue.contains(&path.display().to_string()));
            assert_eq!(fs::read_to_string(&path).unwrap(), input);
        }
    }

    #[test]
    fn save_form_creates_the_directory_and_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("app").join("config.json");
        let settings = save_form(&path, &valid_form(root.path(), true), today()).unwrap();
        assert!(settings.use_yesterday_if_today_missing);
        assert_eq!(settings.config_path, path.display().to_string());
        let config = load(&path).unwrap();
        assert_eq!(config.vault_root, root.path());
        assert!(config.use_yesterday_if_today_missing);
        assert!(fs::read_to_string(&path).unwrap().ends_with("}\n"));
    }

    #[test]
    fn save_form_replaces_invalid_files_and_reports_the_located_note() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        fs::write(&path, r#"{"vault_root": "relative", "typo": 1}"#).unwrap();
        let note = test_support::note_path(root.path(), today());
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(note, b"# Journal\n").unwrap();
        let settings = save_form(&path, &valid_form(root.path(), false), today()).unwrap();
        assert_eq!(
            settings.note,
            Some(NoteLabel {
                name: "2026-03-01".into(),
                is_yesterday: false,
            })
        );
        assert_eq!(load(&path).unwrap().vault_root, root.path());
        assert_eq!(read_form(&path).issue, None);
    }

    #[test]
    fn save_form_validates_before_writing() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let original = r#"{"vault_root": "relative"}"#;
        fs::write(&path, original).unwrap();
        let file = path.display().to_string();
        let missing = root.path().join("missing").display().to_string();
        for (vault_root, detail) in [
            (None, "Choose a vault folder"),
            (Some(String::new()), "Choose a vault folder"),
            (Some("relative".to_string()), "absolute path"),
            (Some(missing), "cannot access vault_root"),
            (Some(file), "existing directory"),
        ] {
            let form = SettingsForm {
                vault_root,
                use_yesterday_if_today_missing: true,
                ..Default::default()
            };
            let error = save_form(&path, &form, today()).unwrap_err();
            assert_eq!(error.code, "settings");
            assert!(error.message.contains(detail), "{}", error.message);
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
        }
    }

    #[test]
    fn save_form_reports_write_failures_without_touching_the_draft() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        write_config(&path, root.path());
        crate::draft::save(&path, "keep me").unwrap();
        let original = fs::metadata(&path).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        let result = save_form(&path, &valid_form(root.path(), true), today());
        fs::set_permissions(&path, original).unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.code, "settings");
        assert!(error.message.contains("Could not save settings"));
        assert!(!load(&path).unwrap().use_yesterday_if_today_missing);
        save_form(&path, &valid_form(root.path(), true), today()).unwrap();
        assert_eq!(crate::draft::load(&path).unwrap(), "keep me");
    }

    fn write_json(path: &Path, value: serde_json::Value) {
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    #[test]
    fn heading_settings_default_to_end_of_file_and_round_trip() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        write_config(&path, root.path());
        let config = load(&path).unwrap();
        assert_eq!(config.heading, "");
        assert_eq!(config.duplicate_heading, DuplicateHeading::Error);
        assert_eq!(config.placement(), journal_core::Placement::default());
        write_json(
            &path,
            serde_json::json!({
                "vault_root": root.path(),
                "heading": "  ## Daily Log ",
                "duplicate_heading": "last",
            }),
        );
        let config = load(&path).unwrap();
        assert_eq!(
            config.placement(),
            journal_core::Placement {
                heading: Some("## Daily Log".into()),
                duplicates: journal_core::DuplicateHeading::Last,
            }
        );
        set_fallback(&path, true, today()).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["heading"], "## Daily Log");
        assert_eq!(saved["duplicate_heading"], "last");
        assert_eq!(saved["use_yesterday_if_today_missing"], true);
    }

    #[test]
    fn invalid_heading_settings_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        for (field, value) in [
            ("heading", serde_json::json!("Journal")),
            ("heading", serde_json::json!("#Journal")),
            ("heading", serde_json::json!(1)),
            ("duplicate_heading", serde_json::json!("both")),
            ("duplicate_heading", serde_json::json!(true)),
        ] {
            let mut config = serde_json::json!({ "vault_root": root.path() });
            config[field] = value;
            write_json(&path, config);
            assert!(load(&path).is_err(), "{field}");
            let before = fs::read(&path).unwrap();
            assert!(set_fallback(&path, true, today()).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn read_form_prefills_heading_settings_and_reports_problems() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        write_json(
            &path,
            serde_json::json!({
                "vault_root": root.path(),
                "heading": "## Daily Log",
                "duplicate_heading": "first",
            }),
        );
        let valid = read_form(&path);
        assert_eq!(valid.issue, None);
        assert_eq!(valid.form.heading, "## Daily Log");
        assert_eq!(valid.form.duplicate_heading, DuplicateHeading::First);
        write_json(
            &path,
            serde_json::json!({
                "vault_root": root.path(),
                "heading": "Daily Log",
                "duplicate_heading": "both",
            }),
        );
        let invalid = read_form(&path);
        assert_eq!(invalid.form.heading, "Daily Log");
        assert_eq!(invalid.form.duplicate_heading, DuplicateHeading::Error);
        let issue = invalid.issue.unwrap();
        assert!(issue.contains("the heading must be"), "{issue}");
        assert!(issue.contains("\"duplicate_heading\" must be"), "{issue}");
        write_json(
            &path,
            serde_json::json!({ "vault_root": root.path(), "heading": 7, "duplicate_heading": 1 }),
        );
        let issue = read_form(&path).issue.unwrap();
        assert!(issue.contains("\"heading\" has the wrong type"), "{issue}");
        assert!(
            issue.contains("\"duplicate_heading\" has the wrong type"),
            "{issue}"
        );
    }

    #[test]
    fn save_form_trims_and_validates_the_heading() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let mut form = valid_form(root.path(), false);
        form.heading = "Journal".into();
        let error = save_form(&path, &form, today()).unwrap_err();
        assert_eq!(error.code, "settings");
        assert!(error.message.contains("heading"), "{}", error.message);
        assert!(!path.exists());
        form.heading = "  # Journal  ".into();
        form.duplicate_heading = DuplicateHeading::First;
        save_form(&path, &form, today()).unwrap();
        let config = load(&path).unwrap();
        assert_eq!(config.heading, "# Journal");
        assert_eq!(config.duplicate_heading, DuplicateHeading::First);
        form.heading = "   ".into();
        save_form(&path, &form, today()).unwrap();
        assert_eq!(load(&path).unwrap().placement().heading, None);
    }

    #[test]
    fn entry_format_defaults_to_inline_round_trips_and_rejects_bad_values() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        write_config(&path, root.path());
        assert_eq!(load(&path).unwrap().entry_format, EntryFormat::Inline);
        let settings = set_entry_format(&path, EntryFormat::Block, today()).unwrap();
        assert_eq!(settings.entry_format, EntryFormat::Block);
        let config = load(&path).unwrap();
        assert_eq!(config.entry_format, EntryFormat::Block);
        assert_eq!(config.vault_root, root.path());
        set_fallback(&path, true, today()).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["entry_format"], "block");
        assert_eq!(read_form(&path).form.entry_format, EntryFormat::Block);

        let mut form = valid_form(root.path(), false);
        form.entry_format = EntryFormat::Inline;
        let settings = save_form(&path, &form, today()).unwrap();
        assert_eq!(settings.entry_format, EntryFormat::Inline);
        assert_eq!(load(&path).unwrap().entry_format, EntryFormat::Inline);

        for value in [serde_json::json!("fancy"), serde_json::json!(1)] {
            let invalid = serde_json::json!({ "vault_root": root.path(), "entry_format": value });
            write_json(&path, invalid);
            assert!(load(&path).is_err(), "{value}");
            let before = fs::read(&path).unwrap();
            assert!(set_entry_format(&path, EntryFormat::Block, today()).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
            let result = read_form(&path);
            assert_eq!(result.form.entry_format, EntryFormat::Inline);
            let issue = result.issue.unwrap();
            assert!(issue.contains("\"entry_format\""), "{issue}");
        }
    }

    #[test]
    fn list_headings_scans_the_located_note() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().display().to_string();
        let empty = HeadingList::default();
        assert_eq!(list_headings(None, None, today()), empty);
        assert_eq!(list_headings(Some("relative"), None, today()), empty);
        let missing = root.path().join("missing").display().to_string();
        assert_eq!(list_headings(Some(&missing), None, today()), empty);
        let problem = list_headings(Some(&dir), None, today()).problem.unwrap();
        assert!(problem.contains("not an Obsidian vault"), "{problem}");
        test_support::fake_vault(root.path());
        let layout = Some("Periodic Notes: daily/YYYY/YYYY-MM/YYYY-MM-DD".to_string());
        assert_eq!(
            list_headings(Some(&dir), None, today()),
            HeadingList {
                detected: Some(NoteSource::Periodic),
                layout: layout.clone(),
                ..Default::default()
            }
        );
        let write = |date: NaiveDate, bytes: &[u8]| {
            let note = test_support::note_path(root.path(), date);
            fs::create_dir_all(note.parent().unwrap()).unwrap();
            fs::write(note, bytes).unwrap();
        };
        write(
            today().pred_opt().unwrap(),
            b"\xef\xbb\xbf# Yesterday\r\n## Log\r\n",
        );
        assert_eq!(
            list_headings(Some(&dir), Some(NoteSource::Periodic), today()),
            HeadingList {
                note: Some("2026-02-28".into()),
                headings: vec!["# Yesterday".into(), "## Log".into()],
                problem: None,
                detected: Some(NoteSource::Periodic),
                layout,
            }
        );
        write(today(), b"# Journal\n\xff");
        let invalid = list_headings(Some(&dir), None, today());
        assert_eq!(invalid.note.as_deref(), Some("2026-03-01"));
        assert!(invalid.headings.is_empty());
        assert!(invalid.problem.unwrap().contains("Could not read headings"));
    }

    #[test]
    fn list_headings_reports_disabled_or_missing_plugins() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().display().to_string();
        test_support::fake_vault(root.path());
        let daily = list_headings(Some(&dir), Some(NoteSource::Daily), today());
        assert_eq!(daily.detected, Some(NoteSource::Periodic));
        assert_eq!(daily.layout, None);
        assert!(daily.problem.unwrap().contains("Daily notes"));
        fs::write(root.path().join(".obsidian/community-plugins.json"), "[]").unwrap();
        let none = list_headings(Some(&dir), None, today());
        assert_eq!(none.detected, None);
        assert!(none.problem.unwrap().contains("No daily-notes plugin"));
        fs::write(
            root.path().join(".obsidian/core-plugins.json"),
            r#"{"daily-notes": true}"#,
        )
        .unwrap();
        fs::write(
            root.path().join(".obsidian/daily-notes.json"),
            r#"{"folder": "Journal", "format": "YYYY-MM-DD"}"#,
        )
        .unwrap();
        let core = list_headings(Some(&dir), None, today());
        assert_eq!(core.detected, Some(NoteSource::Daily));
        assert_eq!(
            core.layout.as_deref(),
            Some("Daily notes: Journal/YYYY-MM-DD")
        );
        assert_eq!(core.problem, None);
    }

    #[test]
    fn note_source_is_required_to_find_notes_but_not_to_load() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        test_support::fake_vault(root.path());
        let note = test_support::note_path(root.path(), today());
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, b"# Journal\n").unwrap();
        write_json(&path, serde_json::json!({ "vault_root": root.path() }));
        let config = load(&path).unwrap();
        assert_eq!(config.note_source, None);
        assert_eq!(config.layout().unwrap_err().code, "note_source_unset");
        assert_eq!(config.settings(&path, today()).note, None);
        set_fallback(&path, true, today()).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(saved.get("note_source").is_none(), "{saved}");
        assert_eq!(read_form(&path).form.note_source, None);
        assert_eq!(read_form(&path).issue, None);

        let mut form = form(Some(root.path()), false);
        let error = save_form(&path, &form, today()).unwrap_err();
        assert!(error.message.contains("Choose where daily notes come from"));
        form.note_source = Some(NoteSource::Daily);
        let error = save_form(&path, &form, today()).unwrap_err();
        assert_eq!(error.code, "settings");
        assert!(error.message.contains("not enabled"), "{}", error.message);
        assert!(load(&path).unwrap().note_source.is_none());
        form.note_source = Some(NoteSource::Periodic);
        let settings = save_form(&path, &form, today()).unwrap();
        assert_eq!(settings.note.unwrap().name, "2026-03-01");
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["note_source"], "periodic");
        assert_eq!(
            read_form(&path).form.note_source,
            Some(NoteSource::Periodic)
        );

        for value in [serde_json::json!("auto"), serde_json::json!(true)] {
            write_json(
                &path,
                serde_json::json!({ "vault_root": root.path(), "note_source": value }),
            );
            assert!(load(&path).is_err(), "{value}");
            let result = read_form(&path);
            assert_eq!(result.form.note_source, None);
            assert!(result.issue.unwrap().contains("\"note_source\""));
        }
    }

    #[test]
    fn list_notes_links_relative_to_the_daily_note_folder() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let index = list_notes(&path, today());
        assert!(index.notes.is_empty());
        assert!(index.problem.unwrap().contains("Cannot use"));
        write_config(&path, root.path());
        for file in [
            "Projects/Plan.md",
            "Home.md",
            "daily/2026/2026-03/2026-03-01.md",
        ] {
            let note = root.path().join(file);
            fs::create_dir_all(note.parent().unwrap()).unwrap();
            fs::write(note, b"").unwrap();
        }
        let links = |index: NoteIndex| {
            assert_eq!(index.problem, None);
            index
                .notes
                .into_iter()
                .map(|note| (note.name, note.folder, note.link))
                .collect::<Vec<_>>()
        };
        let owned = |items: [(&str, &str, &str); 3]| {
            items
                .map(|(a, b, c)| (a.to_owned(), b.to_owned(), c.to_owned()))
                .to_vec()
        };
        assert_eq!(
            links(list_notes(&path, today())),
            owned([
                ("2026-03-01", "daily/2026/2026-03", "2026-03-01"),
                ("Home", "", "Home"),
                ("Plan", "Projects", "Plan"),
            ])
        );
        fs::write(
            root.path().join(".obsidian").join("app.json"),
            r#"{"newLinkFormat": "relative"}"#,
        )
        .unwrap();
        assert_eq!(
            links(list_notes(&path, today())),
            owned([
                ("2026-03-01", "daily/2026/2026-03", "2026-03-01"),
                ("Home", "", "../../../Home"),
                ("Plan", "Projects", "../../../Projects/Plan"),
            ])
        );
    }
}
