use crate::error::AppError;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub vault_root: PathBuf,
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

fn error(path: &Path, detail: impl std::fmt::Display) -> AppError {
    AppError::new(
        "configuration",
        format!(
            "Cannot use {}: {detail}\nCreate or edit this JSON file with an absolute \
             \"vault_root\" folder and a boolean \"use_yesterday_if_today_missing\". \
             Press Enter after correcting it; your draft will be retained.",
            path.display()
        ),
    )
}

pub fn load(path: &Path) -> Result<Config, AppError> {
    let bytes = fs::read(path).map_err(|detail| error(path, detail))?;
    let config: Config = serde_json::from_slice(&bytes).map_err(|detail| error(path, detail))?;
    if !config.vault_root.is_absolute() {
        return Err(error(path, "vault_root must be a nonempty absolute path"));
    }
    let metadata = fs::metadata(&config.vault_root)
        .map_err(|detail| error(path, format!("cannot access vault_root: {detail}")))?;
    if !metadata.is_dir() {
        return Err(error(path, "vault_root must be an existing directory"));
    }
    Ok(config)
}

impl Config {
    pub fn settings(&self, path: &Path, today: NaiveDate) -> Settings {
        Settings {
            config_path: path.display().to_string(),
            use_yesterday_if_today_missing: self.use_yesterday_if_today_missing,
            note: self.note_label(today),
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
    let mut bytes = serde_json::to_vec_pretty(&config).map_err(|detail| error(path, detail))?;
    bytes.push(b'\n');
    journal_core::atomic_write(path, &bytes).map_err(|detail| error(path, detail))?;
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
}
