use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike};
use regex::Regex;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::LazyLock,
};
use thiserror::Error;

static VALID_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#{1,6}[ \t]+\S").unwrap());
static BOUNDARY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^#{1,6}[ \t]+\S").unwrap());
static HEADING_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^#{1,6}[ \t]+\S[^\n]*").unwrap());
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
    #[error(
        "The heading \"{heading}\" was not found in the daily note. Add it to the note or \
         change the heading in Settings."
    )]
    HeadingMissing { heading: String },
    #[error(
        "The heading \"{heading}\" appears {count} times in the daily note. Remove the extra \
         headings or choose which one to use in Settings."
    )]
    HeadingDuplicate { heading: String, count: usize },
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

/// Which occurrence to use when the configured heading appears more than once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DuplicateHeading {
    #[default]
    Error,
    First,
    Last,
}

/// Where entries are inserted: at the end of the file, or at the end of a heading's section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Placement {
    pub heading: Option<String>,
    pub duplicates: DuplicateHeading,
}

/// Trims a configured heading; empty means "end of file", otherwise it must be an ATX heading.
pub fn validate_heading(input: &str) -> Result<Option<String>, String> {
    let heading = input.trim();
    if heading.is_empty() {
        return Ok(None);
    }
    if heading.contains(['\r', '\n']) {
        return Err("the heading must be a single line".into());
    }
    if !VALID_HEADING.is_match(heading) {
        return Err(
            "the heading must be 1 to 6 '#' characters, a space, and text, \
             e.g. \"# Journal\" or \"## Daily Log\""
                .into(),
        );
    }
    Ok(Some(heading.to_owned()))
}

/// Distinct headings in document order, limited to ones that `build_updated_note` can match.
pub fn list_headings(text: &str) -> Vec<String> {
    let mut headings: Vec<String> = Vec::new();
    for line in HEADING_LINE.find_iter(text) {
        let heading = line.as_str().trim_end_matches([' ', '\t', '\r']);
        let round_trips = validate_heading(heading)
            .ok()
            .flatten()
            .is_some_and(|valid| valid == heading);
        if round_trips && !headings.iter().any(|seen| seen == heading) {
            headings.push(heading.to_owned());
        }
    }
    headings
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedNote {
    pub path: PathBuf,
    pub is_yesterday: bool,
}

/// Finds today's note, or yesterday's if today's is missing, regardless of the fallback setting.
pub fn locate_daily_note(root: &Path, today: NaiveDate) -> Result<Option<LocatedNote>, LogError> {
    let path = daily_note_path(root, today);
    if is_file(&path)? {
        return Ok(Some(LocatedNote {
            path,
            is_yesterday: false,
        }));
    }
    let yesterday = today.pred_opt().ok_or(LogError::DateRange)?;
    let path = daily_note_path(root, yesterday);
    Ok(is_file(&path)?.then_some(LocatedNote {
        path,
        is_yesterday: true,
    }))
}

/// How an entry is laid out in the note.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EntryFormat {
    /// `[1:05pm] text`
    #[default]
    Inline,
    /// `**1:05pm**`, the text on the next line, a blank line, then `---`.
    Block,
}

/// Local clock time such as `1:05pm`.
pub fn format_clock_time<Tz: TimeZone>(moment: &DateTime<Tz>) -> String {
    let hour = moment.hour() % 12;
    format!(
        "{}:{:02}{}",
        if hour == 0 { 12 } else { hour },
        moment.minute(),
        if moment.hour() < 12 { "am" } else { "pm" }
    )
}

pub fn format_timestamp<Tz: TimeZone>(moment: &DateTime<Tz>) -> String {
    format!("[{}]", format_clock_time(moment))
}

/// Renders a trimmed entry; `newline` is used only for the format's own line breaks.
pub fn render_entry<Tz: TimeZone>(
    format: EntryFormat,
    moment: &DateTime<Tz>,
    text: &str,
    newline: &str,
) -> String {
    match format {
        EntryFormat::Inline => format!("{} {}", format_timestamp(moment), text),
        EntryFormat::Block => {
            let blank = newline.repeat(2);
            // The blank line before `---` keeps the text from becoming a setext heading.
            format!("**{}**{newline}{text}{blank}---", format_clock_time(moment))
        }
    }
}

pub fn trim_entry(text: &str) -> &str {
    // Python str.strip also treats the four ASCII information separators as whitespace.
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

pub fn build_updated_note(
    data: &[u8],
    entry: &str,
    placement: &Placement,
) -> Result<Vec<u8>, LogError> {
    build_updated_note_with(data, |_| entry.to_owned(), placement)
}

/// Like `build_updated_note`, but renders the entry with the note's detected newline.
pub fn build_updated_note_with(
    data: &[u8],
    render: impl FnOnce(&str) -> String,
    placement: &Placement,
) -> Result<Vec<u8>, LogError> {
    let (bom, body) = match data.strip_prefix(BOM) {
        Some(body) => (BOM, body),
        None => (&b""[..], data),
    };
    let text = std::str::from_utf8(body)?;
    let newline = match text.find('\n') {
        Some(index) if index > 0 && body[index - 1] == b'\r' => "\r\n",
        _ => "\n",
    };
    let insertion = match &placement.heading {
        None => text.len(),
        Some(heading) => {
            let pattern = format!(r"(?m)^{}[ \t]*(?:\r?\n|$)", regex::escape(heading));
            let pattern = Regex::new(&pattern).expect("escaped heading is a valid pattern");
            let headings: Vec<_> = pattern.find_iter(text).collect();
            let chosen = match (headings.as_slice(), placement.duplicates) {
                ([], _) => {
                    return Err(LogError::HeadingMissing {
                        heading: heading.clone(),
                    })
                }
                ([only], _) => only,
                ([first, ..], DuplicateHeading::First) => first,
                ([.., last], DuplicateHeading::Last) => last,
                (all, DuplicateHeading::Error) => {
                    return Err(LogError::HeadingDuplicate {
                        heading: heading.clone(),
                        count: all.len(),
                    })
                }
            };
            let start = chosen.end();
            BOUNDARY
                .find(&text[start..])
                .map_or(text.len(), |boundary| start + boundary.start())
        }
    };
    let (before, after) = text.split_at(insertion);
    let entry = render(newline);
    let entry = entry.as_str();
    let double_newline = newline.repeat(2);
    let leading = if before.is_empty() || before.ends_with(&double_newline) {
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
    placement: &Placement,
    format: EntryFormat,
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
    let updated = build_updated_note_with(
        &data,
        |newline| render_entry(format, moment, text, newline),
        placement,
    )?;
    atomic_write(&path, &updated).map_err(|source| LogError::Io {
        operation: "write",
        path: path.clone(),
        source,
    })?;
    Ok(path)
}
