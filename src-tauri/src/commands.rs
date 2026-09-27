use crate::{
    config::{self, EntryFormat, FormResult, HeadingList, Settings, SettingsForm},
    draft,
    error::AppError,
};
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, TryLockError,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub struct Session {
    saved: bool,
    exiting: bool,
}

pub struct AppState {
    pub config_path: PathBuf,
    pub session: Arc<Mutex<Session>>,
    /// Set when an OS close/quit was forwarded to the webview and not yet answered.
    close_pending: AtomicBool,
}

/// How long the webview has to answer an OS close/quit before the app exits without it.
const CLOSE_FALLBACK: Duration = Duration::from_secs(3);

impl AppState {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            session: Arc::new(Mutex::new(Session::default())),
            close_pending: AtomicBool::new(false),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SavedEntry {
    pub note_path: String,
}

pub fn lock_session(session: &Mutex<Session>) -> Result<MutexGuard<'_, Session>, AppError> {
    session.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => AppError::new("busy", "An operation is already in progress."),
        TryLockError::Poisoned(_) => AppError::new(
            "state_unavailable",
            "Application state is unavailable. Close and reopen the application.",
        ),
    })
}

impl Session {
    pub fn prepare_exit(&mut self, path: &Path, text: Option<&str>) -> Result<(), AppError> {
        if self.exiting {
            return Ok(());
        }
        if self.saved {
            draft::clear(path)?;
        } else if let Some(text) = text {
            draft::save(path, text)?;
        }
        self.exiting = true;
        Ok(())
    }

    fn check_active(&self) -> Result<(), AppError> {
        if self.exiting {
            Err(AppError::new("exiting", "The application is closing."))
        } else {
            Ok(())
        }
    }

    fn check_writable(&self) -> Result<(), AppError> {
        self.check_active()?;
        if self.saved {
            Err(AppError::new(
                "already_saved",
                "This entry was already saved. Close the window.",
            ))
        } else {
            Ok(())
        }
    }

    fn save_draft(&self, path: &Path, text: &str) -> Result<(), AppError> {
        self.check_writable()?;
        draft::save(path, text)
    }

    fn save_settings(
        &self,
        path: &Path,
        form: &SettingsForm,
        today: NaiveDate,
    ) -> Result<Settings, AppError> {
        self.check_writable()?;
        config::save_form(path, form, today)
    }

    fn save<Tz: TimeZone>(
        &mut self,
        path: &Path,
        moment: &DateTime<Tz>,
        text: &str,
    ) -> Result<SavedEntry, AppError> {
        self.check_writable()?;
        let config = config::load(path)?;
        let note = journal_core::append_entry(
            &config.vault_root,
            moment,
            config.use_yesterday_if_today_missing,
            &config.placement(),
            config.entry_format.into(),
            text,
        )?;
        self.saved = true;
        Ok(SavedEntry {
            note_path: note.display().to_string(),
        })
    }
}

async fn operate<T: Send + 'static>(
    state: &AppState,
    operation: impl FnOnce(&mut Session, &Path) -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    let session = Arc::clone(&state.session);
    let path = state.config_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut session = lock_session(&session)?;
        session.check_active()?;
        operation(&mut session, &path)
    })
    .await
    .map_err(|_| worker_failed())?
}

fn worker_failed() -> AppError {
    AppError::new(
        "worker_failed",
        "The operation failed unexpectedly. Check the note before retrying.",
    )
}

#[tauri::command]
pub async fn load_draft(state: State<'_, AppState>) -> Result<String, AppError> {
    operate(&state, |_, path| draft::load(path)).await
}

#[tauri::command]
pub async fn save_draft(state: State<'_, AppState>, text: String) -> Result<(), AppError> {
    operate(&state, move |session, path| session.save_draft(path, &text)).await
}

#[tauri::command]
pub async fn load_settings(state: State<'_, AppState>) -> Result<Settings, AppError> {
    let today = Local::now().date_naive();
    operate(&state, move |_, path| {
        Ok(config::load(path)?.settings(path, today))
    })
    .await
}

#[tauri::command]
pub async fn set_fallback(state: State<'_, AppState>, enabled: bool) -> Result<Settings, AppError> {
    let today = Local::now().date_naive();
    operate(&state, move |session, path| {
        session.check_writable()?;
        config::set_fallback(path, enabled, today)
    })
    .await
}

#[tauri::command]
pub async fn set_entry_format(
    state: State<'_, AppState>,
    format: EntryFormat,
) -> Result<Settings, AppError> {
    let today = Local::now().date_naive();
    operate(&state, move |session, path| {
        session.check_writable()?;
        config::set_entry_format(path, format, today)
    })
    .await
}

#[tauri::command]
pub async fn read_settings_form(state: State<'_, AppState>) -> Result<FormResult, AppError> {
    operate(&state, |_, path| Ok(config::read_form(path))).await
}

#[tauri::command]
pub async fn list_headings(
    state: State<'_, AppState>,
    vault_root: Option<String>,
) -> Result<HeadingList, AppError> {
    let today = Local::now().date_naive();
    operate(&state, move |_, _| {
        Ok(config::list_headings(vault_root.as_deref(), today))
    })
    .await
}

#[tauri::command]
pub async fn pick_vault_folder(
    window: WebviewWindow,
    state: State<'_, AppState>,
    current: Option<String>,
) -> Result<Option<String>, AppError> {
    // The lock is released before the picker opens so it is not held while the user browses.
    lock_session(&state.session)?.check_writable()?;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = window
            .dialog()
            .file()
            .set_parent(&window)
            .set_title("Choose journal folder");
        if let Some(directory) = current.map(PathBuf::from).filter(|path| path.is_dir()) {
            dialog = dialog.set_directory(directory);
        }
        dialog.blocking_pick_folder()
    })
    .await
    .map_err(|_| worker_failed())?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    picked
        .into_path()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .map(Some)
        .ok_or_else(|| {
            AppError::new(
                "settings",
                "The chosen folder's path cannot be stored in settings. Choose another folder.",
            )
        })
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    form: SettingsForm,
) -> Result<Settings, AppError> {
    let today = Local::now().date_naive();
    operate(&state, move |session, path| {
        session.save_settings(path, &form, today)
    })
    .await
}

#[tauri::command]
pub async fn submit_entry(
    state: State<'_, AppState>,
    text: String,
) -> Result<SavedEntry, AppError> {
    let moment = Local::now();
    operate(&state, move |session, path| {
        session.save(path, &moment, &text)
    })
    .await
}

pub fn exit(app: &AppHandle, state: &AppState) -> Result<(), AppError> {
    lock_session(&state.session)?.prepare_exit(&state.config_path, None)?;
    app.exit(0);
    Ok(())
}

/// Handles an OS close or quit. Returns whether the app may exit immediately.
///
/// Unless the session is already exiting, the webview is asked to save the latest text and
/// exit via `request_exit`. If it does not answer in time, the app exits with the last
/// saved draft. Exiting stays blocked while another operation is running.
pub fn request_os_exit(app: &AppHandle, state: &AppState) -> bool {
    match lock_session(&state.session) {
        Ok(session) if session.exiting => return true,
        Ok(_) => {}
        Err(error) => {
            eprintln!("{}: {}", error.code, error.message);
            return false;
        }
    }
    state.close_pending.store(true, Ordering::SeqCst);
    if let Err(error) = app.emit("close-requested", ()) {
        eprintln!("close_event: {error}");
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(CLOSE_FALLBACK);
        let state = app.state::<AppState>();
        if state.close_pending.swap(false, Ordering::SeqCst) {
            if let Err(error) = exit(&app, &state) {
                eprintln!("{}: {}", error.code, error.message);
            }
        }
    });
    false
}

#[tauri::command]
pub async fn request_exit(
    app: AppHandle,
    state: State<'_, AppState>,
    text: Option<String>,
) -> Result<(), AppError> {
    state.close_pending.store(false, Ordering::SeqCst);
    operate(&state, move |session, path| {
        session.prepare_exit(path, text.as_deref())
    })
    .await?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use std::fs;

    #[test]
    fn save_reloads_configuration_and_cannot_append_twice() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let moment = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 9, 22, 1, 2, 0)
            .unwrap();
        let note =
            journal_core::daily_note_path(root.path(), moment.date_naive().pred_opt().unwrap());
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, b"# Journal\n").unwrap();
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "vault_root": root.path()
            }))
            .unwrap(),
        )
        .unwrap();
        let mut session = Session::default();
        draft::save(&path, "draft").unwrap();
        assert_eq!(
            session.save(&path, &moment, "draft").unwrap_err().code,
            "today_missing"
        );
        assert!(!session.saved);
        assert_eq!(draft::load(&path).unwrap(), "draft");
        config::set_fallback(&path, true, moment.date_naive()).unwrap();
        session.save(&path, &moment, "draft").unwrap();
        assert!(session.saved);
        assert_eq!(
            session.save(&path, &moment, "draft").unwrap_err().code,
            "already_saved"
        );
        assert_eq!(fs::read(note).unwrap(), b"# Journal\n\n[1:02am] draft\n");
        session
            .prepare_exit(&path, Some("must not restore a logged entry"))
            .unwrap();
        assert_eq!(draft::load(&path).unwrap(), "");
    }

    #[test]
    fn concurrent_operations_and_operations_after_exit_are_rejected() {
        let state = AppState::new(PathBuf::from("unused"));
        let mut guard = lock_session(&state.session).unwrap();
        assert!(matches!(lock_session(&state.session), Err(error) if error.code == "busy"));
        guard.exiting = true;
        assert_eq!(guard.check_writable().unwrap_err().code, "exiting");
    }

    #[test]
    fn escape_persists_before_exit_and_storage_errors_block_exit() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.json");
        let mut session = Session::default();
        session
            .prepare_exit(&config, Some("unfinished\ntext"))
            .unwrap();
        assert!(session.exiting);
        assert_eq!(draft::load(&config).unwrap(), "unfinished\ntext");
        session.prepare_exit(&config, None).unwrap();
        assert_eq!(draft::load(&config).unwrap(), "unfinished\ntext");
        fs::remove_file(config.with_file_name("draft.txt")).unwrap();
        fs::create_dir(config.with_file_name("draft.txt")).unwrap();
        let mut session = Session::default();
        assert!(session
            .prepare_exit(&config, Some("keep in the window"))
            .is_err());
        assert!(!session.exiting);
        session.saved = true;
        assert!(session.prepare_exit(&config, None).is_err());
        assert!(!session.exiting);
        assert_eq!(session.check_writable().unwrap_err().code, "already_saved");
        fs::remove_dir(config.with_file_name("draft.txt")).unwrap();
        session.prepare_exit(&config, None).unwrap();
        assert!(session.exiting);
    }

    #[test]
    fn settings_cannot_be_saved_after_an_entry_is_logged_or_while_exiting() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let form = SettingsForm {
            vault_root: Some(root.path().display().to_string()),
            use_yesterday_if_today_missing: false,
            ..Default::default()
        };
        let mut session = Session {
            saved: true,
            ..Default::default()
        };
        let error = session.save_settings(&path, &form, today).unwrap_err();
        assert_eq!(error.code, "already_saved");
        assert!(!path.exists());
        session.saved = false;
        session.exiting = true;
        let error = session.save_settings(&path, &form, today).unwrap_err();
        assert_eq!(error.code, "exiting");
        assert!(!path.exists());
        session.exiting = false;
        session.save_settings(&path, &form, today).unwrap();
        assert_eq!(config::load(&path).unwrap().vault_root, root.path());
    }

    #[test]
    fn save_inserts_under_the_configured_heading() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let moment = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 9, 22, 13, 5, 0)
            .unwrap();
        let note = journal_core::daily_note_path(root.path(), moment.date_naive());
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, b"# Day\n## Log\nold\n### Later\n").unwrap();
        let mut form = SettingsForm {
            vault_root: Some(root.path().display().to_string()),
            heading: "## Missing".into(),
            ..Default::default()
        };
        config::save_form(&path, &form, moment.date_naive()).unwrap();
        let mut session = Session::default();
        assert_eq!(
            session.save(&path, &moment, "entry").unwrap_err().code,
            "heading_missing"
        );
        form.heading = "## Log".into();
        config::save_form(&path, &form, moment.date_naive()).unwrap();
        session.save(&path, &moment, "entry").unwrap();
        assert_eq!(
            fs::read_to_string(note).unwrap(),
            "# Day\n## Log\nold\n\n[1:05pm] entry\n\n### Later\n"
        );
    }

    #[test]
    fn save_uses_the_configured_block_format() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let moment = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 9, 22, 13, 5, 0)
            .unwrap();
        let note = journal_core::daily_note_path(root.path(), moment.date_naive());
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, b"# Log\n").unwrap();
        let form = SettingsForm {
            vault_root: Some(root.path().display().to_string()),
            heading: "# Log".into(),
            entry_format: EntryFormat::Block,
            ..Default::default()
        };
        config::save_form(&path, &form, moment.date_naive()).unwrap();
        Session::default().save(&path, &moment, " entry ").unwrap();
        assert_eq!(
            fs::read_to_string(note).unwrap(),
            "# Log\n\n**1:05pm**\nentry\n\n---\n"
        );
    }

    #[test]
    fn autosave_writes_and_clears_the_draft_until_the_session_ends() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let mut session = Session::default();
        session.save_draft(&path, "  partial\nline ").unwrap();
        assert_eq!(draft::load(&path).unwrap(), "  partial\nline ");
        session.save_draft(&path, "").unwrap();
        assert!(!path.with_file_name("draft.txt").exists());
        session.save_draft(&path, "kept").unwrap();
        session.saved = true;
        let error = session.save_draft(&path, "after logging").unwrap_err();
        assert_eq!(error.code, "already_saved");
        session.saved = false;
        session.exiting = true;
        let error = session.save_draft(&path, "while exiting").unwrap_err();
        assert_eq!(error.code, "exiting");
        assert_eq!(draft::load(&path).unwrap(), "kept");
    }
}
