use chrono::{FixedOffset, NaiveDate, TimeZone};
use journal_core::{
    append_entry, atomic_write, build_updated_note, daily_note_path, format_timestamp,
    select_daily_note, trim_entry, LogError,
};
use std::{fs, path::Path};

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn note(root: &Path, day: NaiveDate, bytes: &[u8]) -> std::path::PathBuf {
    let path = daily_note_path(root, day);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn selection_policy_and_calendar_boundaries() {
    for (today, yesterday) in [
        (date(2026, 1, 1), date(2025, 12, 31)),
        (date(2024, 3, 1), date(2024, 2, 29)),
        (date(2026, 3, 1), date(2026, 2, 28)),
    ] {
        let root = tempfile::tempdir().unwrap();
        assert!(matches!(
            select_daily_note(root.path(), today, true),
            Err(LogError::NotesMissing { .. })
        ));
        let previous = note(root.path(), yesterday, b"# Journal\n");
        assert!(matches!(
            select_daily_note(root.path(), today, false),
            Err(LogError::TodayMissing { .. })
        ));
        assert_eq!(
            select_daily_note(root.path(), today, true).unwrap(),
            previous
        );
        let current = note(root.path(), today, b"# Journal\n");
        for fallback in [false, true] {
            assert_eq!(
                select_daily_note(root.path(), today, fallback).unwrap(),
                current
            );
        }
    }
}

#[test]
fn timestamps_use_the_supplied_local_clock() {
    let zone = FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap();
    for (hour, minute, expected) in [
        (0, 0, "[12:00am]"),
        (1, 5, "[1:05am]"),
        (11, 59, "[11:59am]"),
        (12, 0, "[12:00pm]"),
        (13, 5, "[1:05pm]"),
        (23, 59, "[11:59pm]"),
    ] {
        let moment = zone.with_ymd_and_hms(2026, 1, 1, hour, minute, 0).unwrap();
        assert_eq!(format_timestamp(&moment), expected);
    }
}

#[test]
fn exact_heading_and_spacing_fixtures() {
    let cases = [
        ("# Journal", "# Journal\n\nentry\n"),
        ("# Journal\n", "# Journal\n\nentry\n"),
        ("# Journal\n\n", "# Journal\n\nentry\n"),
        ("# Journal\n\n\n", "# Journal\n\n\nentry\n"),
        ("# Journal\nold", "# Journal\nold\n\nentry\n"),
        ("# Journal\nold\n", "# Journal\nold\n\nentry\n"),
        (
            "front\n# Journal \t\nold\n## Next\nkeep\n## Other\n",
            "front\n# Journal \t\nold\n\nentry\n\n## Next\nkeep\n## Other\n",
        ),
        ("# Journal\n## Next", "# Journal\n\nentry\n\n## Next"),
        (
            "# Journal\n### Sub\n# Other\ntext",
            "# Journal\n### Sub\n# Other\ntext\n\nentry\n",
        ),
        (
            "# Journal\n##\n## \nend",
            "# Journal\n##\n## \nend\n\nentry\n",
        ),
        (
            "# Journal\n```\n## Inside fence\n```",
            "# Journal\n```\n\nentry\n\n## Inside fence\n```",
        ),
        (
            "# Journal\r\nold\r\n## Next\r\n",
            "# Journal\r\nold\r\n\r\nentry\r\n\r\n## Next\r\n",
        ),
        (
            "front\r\n# Journal\nold\n",
            "front\r\n# Journal\nold\n\r\n\r\nentry\r\n",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(
            build_updated_note(input.as_bytes(), "entry").unwrap(),
            expected.as_bytes(),
            "input: {input:?}"
        );
    }
}

#[test]
fn invalid_heading_counts_and_encoding() {
    for input in [
        "",
        " # Journal\n",
        "# journal\n",
        "# Journal extra\n",
        "## Journal\n",
    ] {
        assert!(matches!(
            build_updated_note(input.as_bytes(), "entry"),
            Err(LogError::Structure(0))
        ));
    }
    assert!(matches!(
        build_updated_note(b"# Journal\n# Journal\n", "entry"),
        Err(LogError::Structure(2))
    ));
    assert!(matches!(
        build_updated_note(b"# Journal\n\xff", "entry"),
        Err(LogError::Encoding(_))
    ));
}

#[test]
fn bom_and_multiline_unicode_are_preserved() {
    let result = build_updated_note(
        b"\xef\xbb\xbf# Journal\r\n## Next\r\n",
        "first\nsecond \u{1f333}",
    )
    .unwrap();
    assert_eq!(
        result,
        "\u{feff}# Journal\r\n\r\nfirst\nsecond \u{1f333}\r\n\r\n## Next\r\n".as_bytes()
    );
    assert_eq!(trim_entry("\u{1c}\u{a0} a  b\nc \u{1f}"), "a  b\nc");
}

#[test]
fn append_is_exact_and_uses_invocation_date_even_near_midnight() {
    let root = tempfile::tempdir().unwrap();
    let zone = FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap();
    let moment = zone.with_ymd_and_hms(2026, 1, 1, 0, 5, 0).unwrap();
    let previous = note(root.path(), date(2025, 12, 31), b"# Journal\n");
    let result = append_entry(root.path(), &moment, true, "  hello\nworld  ").unwrap();
    assert_eq!(result, previous);
    assert_eq!(
        fs::read(previous).unwrap(),
        b"# Journal\n\n[12:05am] hello\nworld\n"
    );
    assert!(!daily_note_path(root.path(), date(2026, 1, 1)).exists());
}

#[test]
fn invalid_entries_and_notes_do_not_write() {
    let root = tempfile::tempdir().unwrap();
    let moment = FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
        .unwrap();
    let path = note(root.path(), moment.date_naive(), b"no journal");
    for text in ["", " \t\n", "\u{1f}"] {
        assert!(matches!(
            append_entry(root.path(), &moment, false, text),
            Err(LogError::EmptyEntry)
        ));
    }
    assert!(matches!(
        append_entry(root.path(), &moment, false, "entry"),
        Err(LogError::Structure(0))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"no journal");
    fs::write(&path, b"# Journal\n\xff").unwrap();
    assert!(matches!(
        append_entry(root.path(), &moment, false, "entry"),
        Err(LogError::Encoding(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"# Journal\n\xff");
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn atomic_replace_preserves_permissions_and_cleans_temporary_files() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.md");
    fs::write(&path, b"original").unwrap();
    let permissions = fs::metadata(&path).unwrap().permissions();
    atomic_write(&path, b"replacement").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"replacement");
    assert_eq!(fs::metadata(&path).unwrap().permissions(), permissions);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn readonly_destination_is_not_modified() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.md");
    fs::write(&path, b"original").unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let result = atomic_write(&path, b"replacement");
    fs::set_permissions(&path, original).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), b"original");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn failed_replacement_cleans_up_and_never_removes_destination() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("directory");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("keep"), b"untouched").unwrap();
    assert!(atomic_write(&path, b"replacement").is_err());
    assert_eq!(fs::read(path.join("keep")).unwrap(), b"untouched");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn atomic_note_replacement_still_requires_an_existing_destination() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("missing.md");
    assert!(atomic_write(&path, b"must not create a missing note").is_err());
    assert!(!path.exists());
}
