use crate::{
    config::{self, Settings},
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
    pub fn prepare_exit(&mut self) {
        self.exiting = true;
    }

    fn check_writable(&self) -> Result<(), AppError> {
        if self.saved {
            Err(AppError::new(
                "already_saved",
                "This entry was already saved. Close the window.",
            ))
        } else if self.exiting {
            Err(AppError::new("exiting", "The application is closing."))
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
        session.check_writable()?;
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
pub async fn load_settings(state: State<'_, AppState>) -> Result<Settings, AppError> {
    operate(&state, |_, path| Ok(config::load(path)?.settings(path))).await
}

#[tauri::command]
pub async fn set_fallback(state: State<'_, AppState>, enabled: bool) -> Result<Settings, AppError> {
    operate(&state, move |_, path| config::set_fallback(path, enabled)).await
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
    lock_session(&state.session)?.prepare_exit();
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn request_exit(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    exit(&app, &state)
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
        assert_eq!(
            session.save(&path, &moment, "draft").unwrap_err().code,
            "today_missing"
        );
        assert!(!session.saved);
        config::set_fallback(&path, true).unwrap();
        session.save(&path, &moment, "draft").unwrap();
        assert!(session.saved);
        assert_eq!(
            session.save(&path, &moment, "draft").unwrap_err().code,
            "already_saved"
        );
        assert_eq!(fs::read(note).unwrap(), b"# Journal\n\n[1:02am] draft\n");
    }

    #[test]
    fn concurrent_operations_and_operations_after_exit_are_rejected() {
        let state = AppState::new(PathBuf::from("unused"));
        let mut guard = lock_session(&state.session).unwrap();
        assert!(matches!(lock_session(&state.session), Err(error) if error.code == "busy"));
        guard.exiting = true;
        assert_eq!(guard.check_writable().unwrap_err().code, "exiting");
    }
}
