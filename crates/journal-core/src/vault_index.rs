//! Lists a vault's Markdown notes for `[[` link completion and works out the link text
//! Obsidian would insert for each, following the vault's "New link format" setting.

use crate::{obsidian, LogError};
use serde_json::Value;
use std::{collections::HashMap, fs, path::Path};

/// Scanning stops after this many notes so very large vaults stay responsive.
pub const MAX_NOTES: usize = 20_000;

/// A Markdown note in the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultNote {
    /// Vault-relative path with `/` separators and without `.md`, e.g. `Projects/Plan`.
    pub path: String,
    /// The file name without `.md`, e.g. `Plan`.
    pub name: String,
    /// The vault-relative folder, empty for the vault root, e.g. `Projects`.
    pub folder: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NoteScan {
    /// Sorted case-insensitively by path.
    pub notes: Vec<VaultNote>,
    /// Whether scanning stopped at [`MAX_NOTES`].
    pub truncated: bool,
}

/// Obsidian's "New link format" (`newLinkFormat` in `.obsidian/app.json`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinkFormat {
    /// The file name, or the full path when the name is not unique.
    #[default]
    Shortest,
    /// A path relative to the note containing the link.
    Relative,
    /// The full vault path.
    Absolute,
}

/// Every `.md` note under `root`, skipping dot-files and dot-folders (such as `.obsidian` and
/// `.trash`) as Obsidian does. Symlinked folders are not followed, and unreadable
/// subfolders and non-UTF-8 names are skipped.
pub fn scan_notes(root: &Path) -> Result<NoteScan, LogError> {
    let mut scan = NoteScan::default();
    let mut pending = vec![(root.to_path_buf(), String::new())];
    let mut first = true;
    while let Some((dir, folder)) = pending.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(source) if first => {
                return Err(LogError::Io {
                    operation: "list",
                    path: dir,
                    source,
                })
            }
            Err(_) => continue,
        };
        first = false;
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = if folder.is_empty() {
                name.clone()
            } else {
                format!("{folder}/{name}")
            };
            if kind.is_dir() {
                pending.push((entry.path(), path));
                continue;
            }
            let is_file = kind.is_file() || (kind.is_symlink() && entry.path().is_file());
            let Some(stem) = note_stem(&name).filter(|_| is_file) else {
                continue;
            };
            if scan.notes.len() == MAX_NOTES {
                scan.truncated = true;
                break;
            }
            scan.notes.push(VaultNote {
                path: path[..path.len() - (name.len() - stem.len())].to_owned(),
                name: stem.to_owned(),
                folder: folder.clone(),
            });
        }
        if scan.truncated {
            break;
        }
    }
    scan.notes
        .sort_by_cached_key(|note| (note.path.to_lowercase(), note.path.clone()));
    Ok(scan)
}

fn note_stem(name: &str) -> Option<&str> {
    let (stem, extension) = name.rsplit_once('.')?;
    (extension.eq_ignore_ascii_case("md") && !stem.is_empty()).then_some(stem)
}

/// The vault's link format. A missing or unreadable `app.json`, or an unknown value, means
/// Obsidian's default, [`LinkFormat::Shortest`].
pub fn link_format(root: &Path) -> LinkFormat {
    let path = root.join(".obsidian").join("app.json");
    match obsidian::read_json(&path) {
        Ok(Some(settings)) => match settings.get("newLinkFormat").and_then(Value::as_str) {
            Some("relative") => LinkFormat::Relative,
            Some("absolute") => LinkFormat::Absolute,
            _ => LinkFormat::Shortest,
        },
        _ => LinkFormat::Shortest,
    }
}

/// The text between `[[` and `]]` for each note, in the same order as `notes`. `from_folder` is
/// the vault-relative folder of the note being written, used for [`LinkFormat::Relative`].
pub fn link_texts(notes: &[VaultNote], format: LinkFormat, from_folder: &str) -> Vec<String> {
    match format {
        LinkFormat::Absolute => notes.iter().map(|note| note.path.clone()).collect(),
        LinkFormat::Relative => notes
            .iter()
            .map(|note| relative_path(from_folder, &note.path))
            .collect(),
        LinkFormat::Shortest => {
            let mut counts = HashMap::new();
            for note in notes {
                *counts.entry(note.name.to_lowercase()).or_insert(0usize) += 1;
            }
            notes
                .iter()
                .map(|note| {
                    if counts[&note.name.to_lowercase()] == 1 {
                        note.name.clone()
                    } else {
                        note.path.clone()
                    }
                })
                .collect()
        }
    }
}

fn relative_path(from_folder: &str, target: &str) -> String {
    let from: Vec<&str> = from_folder.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = target.split('/').collect();
    let (target_dirs, _) = to.split_at(to.len() - 1);
    let common = from
        .iter()
        .zip(target_dirs)
        .take_while(|(a, b)| a == b)
        .count();
    std::iter::repeat_n("..", from.len() - common)
        .chain(to[common..].iter().copied())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str) -> VaultNote {
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
        VaultNote {
            path: path.into(),
            name: name.into(),
            folder: folder.into(),
        }
    }

    #[test]
    fn relative_paths_climb_to_the_common_folder() {
        assert_eq!(relative_path("", "A/B"), "A/B");
        assert_eq!(relative_path("A", "A/B"), "B");
        assert_eq!(
            relative_path("daily/2026", "Projects/Plan"),
            "../../Projects/Plan"
        );
        assert_eq!(relative_path("daily/2026", "daily/Plan"), "../Plan");
        assert_eq!(relative_path("daily", "Plan"), "../Plan");
    }

    #[test]
    fn shortest_uses_the_full_path_only_for_duplicate_names() {
        let notes = [
            note("A/Plan"),
            note("B/plan"),
            note("Solo"),
            note("C/Other"),
        ];
        assert_eq!(
            link_texts(&notes, LinkFormat::Shortest, ""),
            ["A/Plan", "B/plan", "Solo", "Other"]
        );
        assert_eq!(
            link_texts(&notes, LinkFormat::Absolute, "x"),
            ["A/Plan", "B/plan", "Solo", "C/Other"]
        );
    }
}
