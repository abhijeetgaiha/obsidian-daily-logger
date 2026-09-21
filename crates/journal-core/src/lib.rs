use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike};
use regex::Regex;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::LazyLock,
};
use thiserror::Error;

static JOURNAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^# Journal[ \t]*(?:\r?\n|$)").unwrap());
static BOUNDARY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^##[ \t]+.+(?:\r?\n|$)").unwrap());
const BOM: &[u8] = b"\xef\xbb\xbf";

#[derive(Debug, Error)]
pub enum LogError {
    #[error("Journal text cannot be empty.")]
    EmptyEntry,
    #[error(
        "Today's daily note ({date}) is missing. Enable yesterday's fallback to use yesterday."
    )]
    TodayMissing { date: NaiveDate },
    #[error("No daily note found for {today} or {yesterday}.")]
    NotesMissing {
        today: NaiveDate,
        yesterday: NaiveDate,
    },
    #[error("The previous calendar date cannot be represented.")]
    DateRange,
    #[error("Expected exactly one '# Journal' heading; found {0}.")]
    Structure(usize),
    #[error("The daily note is not valid UTF-8: {0}")]
    Encoding(#[from] std::str::Utf8Error),
    #[error("Could not {operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

pub fn daily_note_path(root: &Path, date: NaiveDate) -> PathBuf {
    root.join("daily")
        .join(format!("{:04}", date.year()))
        .join(date.format("%Y-%m").to_string())
        .join(format!("{}.md", date.format("%Y-%m-%d")))
}

fn is_file(path: &Path) -> Result<bool, LogError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(LogError::Io {
            operation: "inspect",
            path: path.to_owned(),
            source,
        }),
    }
}

pub fn select_daily_note(
    root: &Path,
    today: NaiveDate,
    use_yesterday: bool,
) -> Result<PathBuf, LogError> {
    let path = daily_note_path(root, today);
    if is_file(&path)? {
        return Ok(path);
    }
    if !use_yesterday {
        return Err(LogError::TodayMissing { date: today });
    }
    let yesterday = today.pred_opt().ok_or(LogError::DateRange)?;
    let path = daily_note_path(root, yesterday);
    if is_file(&path)? {
        Ok(path)
    } else {
        Err(LogError::NotesMissing { today, yesterday })
    }
}

pub fn format_timestamp<Tz: TimeZone>(moment: &DateTime<Tz>) -> String {
    let hour = moment.hour() % 12;
    format!(
        "[{}:{:02}{}]",
        if hour == 0 { 12 } else { hour },
        moment.minute(),
        if moment.hour() < 12 { "am" } else { "pm" }
    )
}

pub fn trim_entry(text: &str) -> &str {
    // Python str.strip also treats the four ASCII information separators as whitespace.
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

pub fn build_updated_note(data: &[u8], entry: &str) -> Result<Vec<u8>, LogError> {
    let (bom, body) = match data.strip_prefix(BOM) {
        Some(body) => (BOM, body),
        None => (&b""[..], data),
    };
    let text = std::str::from_utf8(body)?;
    let newline = match text.find('\n') {
        Some(index) if index > 0 && body[index - 1] == b'\r' => "\r\n",
        _ => "\n",
    };
    let headings: Vec<_> = JOURNAL.find_iter(text).collect();
    if headings.len() != 1 {
        return Err(LogError::Structure(headings.len()));
    }
    let start = headings[0].end();
    let insertion = BOUNDARY
        .find(&text[start..])
        .map_or(text.len(), |boundary| start + boundary.start());
    let (before, after) = text.split_at(insertion);
    let double_newline = newline.repeat(2);
    let leading = if before.ends_with(&double_newline) {
        ""
    } else if before.ends_with(newline) {
        newline
    } else {
        &double_newline
    };
    let trailing = if after.is_empty() {
        newline
    } else {
        &double_newline
    };
    let mut updated = Vec::with_capacity(data.len() + entry.len() + 8);
    updated.extend_from_slice(bom);
    for part in [before, leading, entry, trailing, after] {
        updated.extend_from_slice(part.as_bytes());
    }
    Ok(updated)
}

pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    atomic_write_with_permissions(path, data, Some(fs::metadata(path)?.permissions()))
}

pub fn atomic_write_or_create(path: &Path, data: &[u8]) -> io::Result<()> {
    let permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    atomic_write_with_permissions(path, data, permissions)
}

fn atomic_write_with_permissions(
    path: &Path,
    data: &[u8],
    permissions: Option<fs::Permissions>,
) -> io::Result<()> {
    if permissions.as_ref().is_some_and(fs::Permissions::readonly) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the destination is read-only",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "destination has no parent"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".journal-logger-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    temporary.write_all(data)?;
    temporary.as_file().sync_all()?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub fn append_entry<Tz: TimeZone>(
    root: &Path,
    moment: &DateTime<Tz>,
    use_yesterday: bool,
    text: &str,
) -> Result<PathBuf, LogError> {
    let text = trim_entry(text);
    if text.is_empty() {
        return Err(LogError::EmptyEntry);
    }
    let path = select_daily_note(root, moment.date_naive(), use_yesterday)?;
    let data = fs::read(&path).map_err(|source| LogError::Io {
        operation: "read",
        path: path.clone(),
        source,
    })?;
    let entry = format!("{} {}", format_timestamp(moment), text);
    let updated = build_updated_note(&data, &entry)?;
    atomic_write(&path, &updated).map_err(|source| LogError::Io {
        operation: "write",
        path: path.clone(),
        source,
    })?;
    Ok(path)
}
