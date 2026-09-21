use crate::error::AppError;
use std::{fs, io, path::Path};

fn error(path: &Path, operation: &str, source: io::Error) -> AppError {
    AppError::new(
        "draft_io",
        format!(
            "Could not {operation} the draft at {}: {source}",
            path.display()
        ),
    )
}

pub fn load(config_path: &Path) -> Result<String, AppError> {
    let path = config_path.with_file_name("draft.txt");
    match fs::read_to_string(&path) {
        Ok(text) => Ok(text),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(error(&path, "load", source)),
    }
}

pub fn save(config_path: &Path, text: &str) -> Result<(), AppError> {
    if text.is_empty() {
        return clear(config_path);
    }
    let path = config_path.with_file_name("draft.txt");
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("draft_io", "The draft location has no parent directory."))?;
    fs::create_dir_all(parent).map_err(|source| error(&path, "save", source))?;
    journal_core::atomic_write_or_create(&path, text.as_bytes())
        .map_err(|source| error(&path, "save", source))
}

pub fn clear(config_path: &Path) -> Result<(), AppError> {
    let path = config_path.with_file_name("draft.txt");
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(error(&path, "clear", source)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_without_configuration_and_clear_empty_drafts() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("app").join("config.json");
        assert_eq!(load(&config).unwrap(), "");
        let text = "  unfinished \u{1f333}\nsecond line\n ";
        save(&config, text).unwrap();
        assert_eq!(load(&config).unwrap(), text);
        assert!(!config.exists());
        save(&config, "replacement").unwrap();
        assert_eq!(load(&config).unwrap(), "replacement");
        save(&config, "").unwrap();
        assert_eq!(load(&config).unwrap(), "");
        clear(&config).unwrap();
        assert_eq!(fs::read_dir(config.parent().unwrap()).unwrap().count(), 0);
    }

    #[test]
    fn failures_do_not_hide_or_overwrite_the_previous_draft() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.json");
        let path = config.with_file_name("draft.txt");
        save(&config, "keep me").unwrap();
        let original = fs::metadata(&path).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        let result = save(&config, "replacement");
        fs::set_permissions(&path, original).unwrap();
        assert_eq!(result.unwrap_err().code, "draft_io");
        assert_eq!(load(&config).unwrap(), "keep me");
        fs::write(&path, [0xff]).unwrap();
        assert_eq!(load(&config).unwrap_err().code, "draft_io");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(clear(&config).is_err());
        assert!(save(&config, "replacement").is_err());
    }
}
