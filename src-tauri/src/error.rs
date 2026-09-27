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
            LogError::NotAVault { .. } => "not_a_vault",
            LogError::NoDailyNotesPlugin => "no_daily_notes_plugin",
            LogError::PluginDisabled { .. } => "plugin_disabled",
            LogError::PluginSettings { .. } => "plugin_settings",
            LogError::DateFormat(_) => "date_format",
            LogError::NotePath(_) => "note_path",
            LogError::HeadingMissing { .. } => "heading_missing",
            LogError::HeadingDuplicate { .. } => "heading_duplicate",
            LogError::Encoding(_) => "note_encoding",
            LogError::Io { .. } => "note_io",
        };
        Self::new(code, error.to_string())
    }
}
