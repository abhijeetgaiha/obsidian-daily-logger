use journal_core::vault_index::{link_format, link_texts, scan_notes, LinkFormat, VaultNote};
use std::{fs, path::Path};

fn touch(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"").unwrap();
}

fn paths(notes: &[VaultNote]) -> Vec<&str> {
    notes.iter().map(|note| note.path.as_str()).collect()
}

#[test]
fn scans_markdown_notes_and_skips_dot_entries_and_other_files() {
    let root = tempfile::tempdir().unwrap();
    for file in [
        "Home.md",
        "Projects/Plan.md",
        "Projects/deep/Idea.MD",
        "daily/2026/2026-09-30.md",
        "image.png",
        "Projects/notes.txt",
        ".obsidian/workspace.md",
        ".trash/Deleted.md",
        "Projects/.hidden.md",
        ".md",
    ] {
        touch(root.path(), file);
    }
    let scan = scan_notes(root.path()).unwrap();
    assert!(!scan.truncated);
    assert_eq!(
        paths(&scan.notes),
        [
            "daily/2026/2026-09-30",
            "Home",
            "Projects/deep/Idea",
            "Projects/Plan"
        ]
    );
    let idea = &scan.notes[2];
    assert_eq!(
        (idea.name.as_str(), idea.folder.as_str()),
        ("Idea", "Projects/deep")
    );
    assert_eq!(scan.notes[1].folder, "");
}

#[test]
fn scanning_a_missing_vault_is_an_error() {
    let root = tempfile::tempdir().unwrap();
    assert!(scan_notes(&root.path().join("missing")).is_err());
}

#[test]
fn reads_the_link_format_from_app_json() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(link_format(root.path()), LinkFormat::Shortest);
    let app = root.path().join(".obsidian").join("app.json");
    fs::create_dir_all(app.parent().unwrap()).unwrap();
    for (contents, format) in [
        (r#"{"newLinkFormat": "relative"}"#, LinkFormat::Relative),
        (r#"{"newLinkFormat": "absolute"}"#, LinkFormat::Absolute),
        (r#"{"newLinkFormat": "shortest"}"#, LinkFormat::Shortest),
        (r#"{"newLinkFormat": "other"}"#, LinkFormat::Shortest),
        (r#"{"useMarkdownLinks": true}"#, LinkFormat::Shortest),
        ("not json", LinkFormat::Shortest),
    ] {
        fs::write(&app, contents).unwrap();
        assert_eq!(link_format(root.path()), format, "{contents}");
    }
}

#[test]
fn builds_link_text_for_each_format() {
    let root = tempfile::tempdir().unwrap();
    for file in ["A/Plan.md", "B/plan.md", "Solo.md", "daily/2026/Day.md"] {
        touch(root.path(), file);
    }
    let notes = scan_notes(root.path()).unwrap().notes;
    assert_eq!(
        paths(&notes),
        ["A/Plan", "B/plan", "daily/2026/Day", "Solo"]
    );
    assert_eq!(
        link_texts(&notes, LinkFormat::Shortest, "daily/2026"),
        ["A/Plan", "B/plan", "Day", "Solo"]
    );
    assert_eq!(
        link_texts(&notes, LinkFormat::Absolute, "daily/2026"),
        ["A/Plan", "B/plan", "daily/2026/Day", "Solo"]
    );
    assert_eq!(
        link_texts(&notes, LinkFormat::Relative, "daily/2026"),
        ["../../A/Plan", "../../B/plan", "Day", "../../Solo"]
    );
}
