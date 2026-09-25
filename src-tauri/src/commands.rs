use crate::{
    config::{self, Settings},
    draft,
    error::AppError,
};
use chrono::{DateTime, Local, TimeZone};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, TryLockError},
};
use tauri::{AppHandle, State};

#[derive(Default)]
pub struct Session {
    saved: bool,
    exiting: bool,
}

pub struct AppState {
    pub config_path: PathBuf,
    pub session: Arc<Mutex<Session>>,
}

impl AppState {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            session: Arc::new(Mutex::new(Session::default())),
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
    .map_err(|_| {
        AppError::new(
            "worker_failed",
            "The operation failed unexpectedly. Check the note before retrying.",
        )
    })?
}

#[tauri::command]
pub async fn load_draft(state: State<'_, AppState>) -> Result<String, AppError> {
    operate(&state, |_, path| draft::load(path)).await
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

#[tauri::command]
pub async fn request_exit(
    app: AppHandle,
    state: State<'_, AppState>,
    text: Option<String>,
) -> Result<(), AppError> {
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
}
