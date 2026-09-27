use chrono::{FixedOffset, NaiveDate, TimeZone};
use journal_core::{
    append_entry, atomic_write, build_updated_note, format_clock_time, format_timestamp,
    list_headings, locate_daily_note, render_entry, select_daily_note, trim_entry,
    validate_heading, DuplicateHeading,
    EntryFormat::{self, Block, Inline},
    LocatedNote, LogError, NoteLayout, Placement,
};
use std::{fs, path::Path};

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn under(heading: &str, duplicates: DuplicateHeading) -> Placement {
    Placement {
        heading: Some(heading.into()),
        duplicates,
    }
}

fn journal() -> Placement {
    under("# Journal", DuplicateHeading::Error)
}

fn layout() -> NoteLayout {
    NoteLayout::new("daily", "YYYY/YYYY-MM/YYYY-MM-DD").unwrap()
}

fn daily_note_path(root: &Path, day: NaiveDate) -> std::path::PathBuf {
    layout().path(root, day).unwrap()
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
            select_daily_note(root.path(), &layout(), today, true),
            Err(LogError::NotesMissing { .. })
        ));
        let previous = note(root.path(), yesterday, b"# Journal\n");
        assert!(matches!(
            select_daily_note(root.path(), &layout(), today, false),
            Err(LogError::TodayMissing { .. })
        ));
        assert_eq!(
            select_daily_note(root.path(), &layout(), today, true).unwrap(),
            previous
        );
        let current = note(root.path(), today, b"# Journal\n");
        for fallback in [false, true] {
            assert_eq!(
                select_daily_note(root.path(), &layout(), today, fallback).unwrap(),
                current
            );
        }
    }
}

#[test]
fn location_prefers_today_then_yesterday_and_ignores_directories() {
    for (today, yesterday) in [
        (date(2026, 1, 1), date(2025, 12, 31)),
        (date(2026, 3, 1), date(2026, 2, 28)),
    ] {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            locate_daily_note(root.path(), &layout(), today).unwrap(),
            None
        );
        fs::create_dir_all(daily_note_path(root.path(), today)).unwrap();
        assert_eq!(
            locate_daily_note(root.path(), &layout(), today).unwrap(),
            None
        );
        let previous = note(root.path(), yesterday, b"# Journal\n");
        assert_eq!(
            locate_daily_note(root.path(), &layout(), today).unwrap(),
            Some(LocatedNote {
                path: previous,
                is_yesterday: true,
            })
        );
        fs::remove_dir(daily_note_path(root.path(), today)).unwrap();
        let current = note(root.path(), today, b"# Journal\n");
        assert_eq!(
            locate_daily_note(root.path(), &layout(), today).unwrap(),
            Some(LocatedNote {
                path: current,
                is_yesterday: false,
            })
        );
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
            "# Journal\n\nentry\n\n### Sub\n# Other\ntext",
        ),
        (
            "# Journal\nold #tag\n#tag\n#\n####### seven\n# Next",
            "# Journal\nold #tag\n#tag\n#\n####### seven\n\nentry\n\n# Next",
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
            build_updated_note(input.as_bytes(), "entry", &journal()).unwrap(),
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
            build_updated_note(input.as_bytes(), "entry", &journal()),
            Err(LogError::HeadingMissing { heading }) if heading == "# Journal"
        ));
    }
    assert!(matches!(
        build_updated_note(b"# Journal\n# Journal\n", "entry", &journal()),
        Err(LogError::HeadingDuplicate { count: 2, .. })
    ));
    assert!(matches!(
        build_updated_note(b"# Journal\n\xff", "entry", &journal()),
        Err(LogError::Encoding(_))
    ));
    assert!(matches!(
        build_updated_note(b"\xff", "entry", &Placement::default()),
        Err(LogError::Encoding(_))
    ));
}

#[test]
fn empty_heading_appends_to_the_end_of_the_file() {
    let cases = [
        ("", "entry\n"),
        ("\n", "\n\nentry\n"),
        ("text", "text\n\nentry\n"),
        ("text\n", "text\n\nentry\n"),
        ("text\n\n", "text\n\nentry\n"),
        (
            "# Journal\nold\n## Next\nkeep",
            "# Journal\nold\n## Next\nkeep\n\nentry\n",
        ),
        ("a\r\nb\r\n", "a\r\nb\r\n\r\nentry\r\n"),
    ];
    for (input, expected) in cases {
        assert_eq!(
            build_updated_note(input.as_bytes(), "entry", &Placement::default()).unwrap(),
            expected.as_bytes(),
            "input: {input:?}"
        );
    }
    assert_eq!(
        build_updated_note(b"\xef\xbb\xbf", "entry", &Placement::default()).unwrap(),
        "\u{feff}entry\n".as_bytes()
    );
}

#[test]
fn configured_headings_match_exactly_and_end_at_any_heading() {
    let daily = under("## Daily Log", DuplicateHeading::Error);
    assert_eq!(
        build_updated_note(
            b"# Day\n## Daily Log \t\nold\n#### Deep\n## Other\n",
            "entry",
            &daily
        )
        .unwrap(),
        b"# Day\n## Daily Log \t\nold\n\nentry\n\n#### Deep\n## Other\n"
    );
    assert_eq!(
        build_updated_note(b"## Daily Log", "entry", &daily).unwrap(),
        b"## Daily Log\n\nentry\n"
    );
    for input in [
        "# Daily Log\n",
        "### Daily Log\n",
        "## daily log\n",
        "## Daily  Log\n",
        "## Daily Log extra\n",
    ] {
        assert!(
            matches!(
                build_updated_note(input.as_bytes(), "entry", &daily),
                Err(LogError::HeadingMissing { .. })
            ),
            "input: {input:?}"
        );
    }
    let special = under("# Notes (a+b)*", DuplicateHeading::Error);
    assert_eq!(
        build_updated_note(b"# Notes (a+b)*\n", "entry", &special).unwrap(),
        b"# Notes (a+b)*\n\nentry\n"
    );
    assert!(build_updated_note(b"# Notes aab\n", "entry", &special).is_err());
}

#[test]
fn duplicate_headings_follow_the_configured_policy() {
    let input = b"# Log\none\n## Mid\n# Log\ntwo\n";
    assert!(matches!(
        build_updated_note(input, "entry", &under("# Log", DuplicateHeading::Error)),
        Err(LogError::HeadingDuplicate { count: 2, heading }) if heading == "# Log"
    ));
    assert_eq!(
        build_updated_note(input, "entry", &under("# Log", DuplicateHeading::First)).unwrap(),
        b"# Log\none\n\nentry\n\n## Mid\n# Log\ntwo\n"
    );
    assert_eq!(
        build_updated_note(input, "entry", &under("# Log", DuplicateHeading::Last)).unwrap(),
        b"# Log\none\n## Mid\n# Log\ntwo\n\nentry\n"
    );
    for duplicates in [DuplicateHeading::First, DuplicateHeading::Last] {
        assert!(matches!(
            build_updated_note(b"text", "entry", &under("# Log", duplicates)),
            Err(LogError::HeadingMissing { .. })
        ));
    }
}

#[test]
fn heading_validation() {
    for (input, expected) in [
        ("", None),
        ("  \t ", None),
        ("# Journal", Some("# Journal")),
        ("  ## Daily Log  ", Some("## Daily Log")),
        ("###### Six", Some("###### Six")),
        ("#\tTabbed", Some("#\tTabbed")),
    ] {
        assert_eq!(
            validate_heading(input).unwrap().as_deref(),
            expected,
            "{input:?}"
        );
    }
    for input in [
        "Journal",
        "#Journal",
        "#",
        "## ",
        "####### Seven",
        " x # Journal",
        "# Journal\n## Two",
    ] {
        assert!(validate_heading(input).is_err(), "{input:?}");
    }
}

#[test]
fn bom_and_multiline_unicode_are_preserved() {
    let result = build_updated_note(
        b"\xef\xbb\xbf# Journal\r\n## Next\r\n",
        "first\nsecond \u{1f333}",
        &journal(),
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
    let result = append_entry(
        root.path(),
        &layout(),
        &moment,
        true,
        &journal(),
        Inline,
        "  hello\nworld  ",
    )
    .unwrap();
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
            append_entry(
                root.path(),
                &layout(),
                &moment,
                false,
                &journal(),
                Inline,
                text
            ),
            Err(LogError::EmptyEntry)
        ));
    }
    assert!(matches!(
        append_entry(
            root.path(),
            &layout(),
            &moment,
            false,
            &journal(),
            Inline,
            "entry"
        ),
        Err(LogError::HeadingMissing { .. })
    ));
    assert_eq!(fs::read(&path).unwrap(), b"no journal");
    fs::write(&path, b"# Journal\n\xff").unwrap();
    assert!(matches!(
        append_entry(
            root.path(),
            &layout(),
            &moment,
            false,
            &journal(),
            Inline,
            "entry"
        ),
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

#[test]
fn listed_headings_are_distinct_ordered_and_insertable() {
    let text = "intro\r\n# Journal \t\r\n## Daily Log\n#tag\n##\n## \n####### seven\n\
                ### Deep\n# Journal\n## Padded\u{a0}\n   # Indented\n###### Six";
    let headings = list_headings(text);
    assert_eq!(
        headings,
        ["# Journal", "## Daily Log", "### Deep", "###### Six"]
    );
    for heading in &headings {
        let placement = Placement {
            heading: Some(heading.clone()),
            duplicates: DuplicateHeading::First,
        };
        assert!(
            build_updated_note(text.as_bytes(), "entry", &placement).is_ok(),
            "{heading}"
        );
    }
    assert!(list_headings("").is_empty());
    assert!(list_headings("no headings\n#tag").is_empty());
}

fn block_at(hour: u32, minute: u32) -> chrono::DateTime<FixedOffset> {
    FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(2026, 9, 27, hour, minute, 0)
        .unwrap()
}

#[test]
fn entry_formats_render_inline_and_block_layouts() {
    let moment = block_at(13, 5);
    assert_eq!(format_clock_time(&moment), "1:05pm");
    assert_eq!(format_clock_time(&block_at(0, 0)), "12:00am");
    assert_eq!(EntryFormat::default(), Inline);
    assert_eq!(render_entry(Inline, &moment, "text", "\n"), "[1:05pm] text");
    assert_eq!(
        render_entry(Block, &moment, "a\nb", "\n"),
        "**1:05pm**\na\nb\n\n---"
    );
    assert_eq!(
        render_entry(Block, &moment, "a\nb", "\r\n"),
        "**1:05pm**\r\na\nb\r\n\r\n---"
    );
}

#[test]
fn block_entries_are_placed_like_inline_entries() {
    let root = tempfile::tempdir().unwrap();
    let moment = block_at(9, 7);
    let path = note(root.path(), moment.date_naive(), b"");
    let append = |text: &str, placement: &Placement| {
        append_entry(
            root.path(),
            &layout(),
            &moment,
            false,
            placement,
            Block,
            text,
        )
        .unwrap()
    };
    append(" first ", &Placement::default());
    assert_eq!(fs::read(&path).unwrap(), b"**9:07am**\nfirst\n\n---\n");
    append("second\nline", &Placement::default());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "**9:07am**\nfirst\n\n---\n\n**9:07am**\nsecond\nline\n\n---\n"
    );

    fs::write(&path, b"# Journal\nold\n## Next\n").unwrap();
    append("entry", &journal());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# Journal\nold\n\n**9:07am**\nentry\n\n---\n\n## Next\n"
    );

    fs::write(&path, b"\xef\xbb\xbf# Journal\r\nold\r\n").unwrap();
    append("crlf", &journal());
    assert_eq!(
        fs::read(&path).unwrap(),
        "\u{feff}# Journal\r\nold\r\n\r\n**9:07am**\r\ncrlf\r\n\r\n---\r\n".as_bytes()
    );
}
