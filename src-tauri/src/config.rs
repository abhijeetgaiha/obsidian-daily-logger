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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub vault_root: PathBuf,
    /// Empty appends to the end of the note; otherwise a Markdown heading such as "# Journal".
    #[serde(default)]
    pub heading: String,
    #[serde(default)]
    pub duplicate_heading: DuplicateHeading,
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
    pub note: Option<NoteLabel>,
}

#[derive(Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsForm {
    pub vault_root: Option<String>,
    pub heading: String,
    pub duplicate_heading: DuplicateHeading,
    pub use_yesterday_if_today_missing: bool,
}

#[derive(Debug, Serialize)]
pub struct FormResult {
    pub config_path: String,
    pub exists: bool,
    pub form: SettingsForm,
    pub issue: Option<String>,
}

fn error(path: &Path, detail: impl std::fmt::Display) -> AppError {
    AppError::new(
        "configuration",
        format!(
            "Cannot use {}: {detail}\nUse the Settings gear to fix it, or edit this JSON file: \
             \"vault_root\" must be an absolute folder, \"heading\" empty or a Markdown \
             heading such as \"# Journal\", and \"use_yesterday_if_today_missing\" a boolean. \
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
                    (
                        "vault_root"
                        | "use_yesterday_if_today_missing"
                        | "heading"
                        | "duplicate_heading",
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

/// Validates the dialog's settings, then creates or replaces the settings file.
pub fn save_form(path: &Path, form: &SettingsForm, today: NaiveDate) -> Result<Settings, AppError> {
    let root = form
        .vault_root
        .as_deref()
        .filter(|root| !root.is_empty())
        .ok_or_else(|| AppError::new("settings", "Choose a journal folder before saving."))?;
    validate_root(Path::new(root)).map_err(|detail| {
        AppError::new(
            "settings",
            format!("Cannot use {root} as the journal folder: {detail}"),
        )
    })?;
    let heading = journal_core::validate_heading(&form.heading)
        .map_err(|detail| AppError::new("settings", format!("Cannot use this heading: {detail}.")))?
        .unwrap_or_default();
    let config = Config {
        vault_root: PathBuf::from(root),
        heading,
        duplicate_heading: form.duplicate_heading,
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
            note: self.note_label(today),
        }
    }

    pub fn placement(&self) -> journal_core::Placement {
        journal_core::Placement {
            heading: Some(self.heading.clone()).filter(|heading| !heading.is_empty()),
            duplicates: self.duplicate_heading.into(),
        }
    }

    // Inspection failures are displayed as no note; saving reports the underlying error.
    fn note_label(&self, today: NaiveDate) -> Option<NoteLabel> {
        let note = journal_core::locate_daily_note(&self.vault_root, today).ok()??;
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
            let note = journal_core::daily_note_path(root.path(), date);
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
        fs::write(
            path,
            serde_json::to_vec(&serde_json::json!({
                "vault_root": root
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
        assert_eq!(valid.form, form(Some(root.path()), true));
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
        let settings = save_form(&path, &form(Some(root.path()), true), today()).unwrap();
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
        let note = journal_core::daily_note_path(root.path(), today());
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(note, b"# Journal\n").unwrap();
        let settings = save_form(&path, &form(Some(root.path()), false), today()).unwrap();
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
            (None, "Choose a journal folder"),
            (Some(String::new()), "Choose a journal folder"),
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
        let result = save_form(&path, &form(Some(root.path()), true), today());
        fs::set_permissions(&path, original).unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.code, "settings");
        assert!(error.message.contains("Could not save settings"));
        assert!(!load(&path).unwrap().use_yesterday_if_today_missing);
        save_form(&path, &form(Some(root.path()), true), today()).unwrap();
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
        let mut form = form(Some(root.path()), false);
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
}
