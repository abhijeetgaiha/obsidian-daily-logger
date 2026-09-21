use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<journal_core::LogError> for AppError {
    fn from(error: journal_core::LogError) -> Self {
        use journal_core::LogError;
        let code = match &error {
            LogError::EmptyEntry => "empty_entry",
            LogError::TodayMissing { .. } => "today_missing",
            LogError::NotesMissing { .. } => "notes_missing",
            LogError::DateRange => "date_range",
            LogError::Structure(_) => "note_structure",
            LogError::Encoding(_) => "note_encoding",
            LogError::Io { .. } => "note_io",
        };
        Self::new(code, error.to_string())
    }
}
