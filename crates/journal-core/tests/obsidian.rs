use chrono::NaiveDate;
use journal_core::{
    locate_daily_note,
    obsidian::{detect_source, resolve_layout, NoteSource},
    LogError, NoteLayout,
};
use serde_json::json;
use std::{fs, path::Path};

const FIXTURE: &str = include_str!("fixtures/periodic-notes-0.x.json");

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(".obsidian").join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn vault() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".obsidian")).unwrap();
    root
}

fn periodic(root: &Path, data: &str) {
    write(
        root,
        "community-plugins.json",
        r#"["dataview", "periodic-notes"]"#,
    );
    write(root, "plugins/periodic-notes/data.json", data);
}

fn core(root: &Path, plugins: serde_json::Value, settings: Option<serde_json::Value>) {
    write(root, "core-plugins.json", &plugins.to_string());
    if let Some(settings) = settings {
        write(root, "daily-notes.json", &settings.to_string());
    }
}

fn relative(root: &Path, source: NoteSource) -> String {
    resolve_layout(root, source)
        .unwrap()
        .layout
        .relative_path(date(2026, 9, 27))
        .unwrap()
}

#[test]
fn the_users_periodic_notes_settings_resolve_to_the_nested_layout() {
    let root = vault();
    periodic(root.path(), FIXTURE);
    core(
        root.path(),
        json!({ "file-explorer": true, "daily-notes": false }),
        Some(json!({ "folder": "daily", "autorun": false, "template": "templates/Daily Note" })),
    );
    assert_eq!(
        detect_source(root.path()).unwrap(),
        Some(NoteSource::Periodic)
    );
    let resolved = resolve_layout(root.path(), NoteSource::Periodic).unwrap();
    assert_eq!(
        resolved.describe(),
        "Periodic Notes: daily/YYYY/YYYY-MM/YYYY-MM-DD"
    );
    assert_eq!(
        resolved.layout.relative_path(date(2026, 9, 27)).unwrap(),
        "daily/2026/2026-09/2026-09-27.md"
    );
    let path = resolved
        .layout
        .path(root.path(), date(2026, 9, 27))
        .unwrap();
    assert_eq!(
        path,
        root.path()
            .join("daily")
            .join("2026")
            .join("2026-09")
            .join("2026-09-27.md")
    );
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "# Journal\n").unwrap();
    assert_eq!(
        locate_daily_note(root.path(), &resolved.layout, date(2026, 9, 27))
            .unwrap()
            .unwrap()
            .path,
        path
    );
    assert!(matches!(
        resolve_layout(root.path(), NoteSource::Daily),
        Err(LogError::PluginDisabled { .. })
    ));
}

#[test]
fn periodic_notes_1_0_uses_the_active_calendar_set() {
    let root = vault();
    let data = json!({
        "activeCalendarSet": "Work",
        "calendarSets": [
            { "id": "Default", "day": { "enabled": true, "folder": "home", "format": "YYYY" } },
            {
                "id": "Work",
                "day": { "enabled": true, "folder": "/work/log/", "format": "[Day] Do MMM YYYY" },
                "week": { "enabled": true, "format": "gggg-ww" }
            }
        ]
    });
    periodic(root.path(), &data.to_string());
    assert_eq!(
        relative(root.path(), NoteSource::Periodic),
        "work/log/Day 27th Sep 2026.md"
    );
    let data = json!({ "activeCalendarSet": "Missing", "calendarSets": [] });
    periodic(root.path(), &data.to_string());
    assert!(matches!(
        detect_source(root.path()),
        Err(LogError::PluginSettings { .. })
    ));
}

#[test]
fn periodic_notes_needs_the_plugin_and_daily_notes_enabled() {
    let root = vault();
    let enabled = json!({ "daily": { "enabled": true, "folder": "", "format": "" } });
    for (plugins, data) in [
        (None, Some(enabled.clone())),
        (Some(r#"["dataview"]"#), Some(enabled.clone())),
        (Some(r#"["periodic-notes"]"#), None),
        (
            Some(r#"["periodic-notes"]"#),
            Some(json!({ "daily": { "enabled": false } })),
        ),
        (
            Some(r#"["periodic-notes"]"#),
            Some(json!({ "daily": { "folder": "x" } })),
        ),
        (
            Some(r#"["periodic-notes"]"#),
            Some(json!({ "weekly": { "enabled": true } })),
        ),
    ] {
        let _ = fs::remove_dir_all(root.path().join(".obsidian"));
        fs::create_dir(root.path().join(".obsidian")).unwrap();
        if let Some(plugins) = plugins {
            write(root.path(), "community-plugins.json", plugins);
        }
        if let Some(data) = data {
            write(
                root.path(),
                "plugins/periodic-notes/data.json",
                &data.to_string(),
            );
        }
        assert_eq!(detect_source(root.path()).unwrap(), None, "{plugins:?}");
        assert!(matches!(
            resolve_layout(root.path(), NoteSource::Periodic),
            Err(LogError::PluginDisabled { .. })
        ));
    }
    periodic(root.path(), &enabled.to_string());
    assert_eq!(relative(root.path(), NoteSource::Periodic), "2026-09-27.md");
}

#[test]
fn core_daily_notes_reads_both_plugin_list_forms_and_defaults() {
    let root = vault();
    core(root.path(), json!({ "daily-notes": true }), None);
    assert_eq!(detect_source(root.path()).unwrap(), Some(NoteSource::Daily));
    assert_eq!(relative(root.path(), NoteSource::Daily), "2026-09-27.md");
    assert_eq!(
        resolve_layout(root.path(), NoteSource::Daily)
            .unwrap()
            .describe(),
        "Daily notes: YYYY-MM-DD"
    );
    core(
        root.path(),
        json!(["file-explorer", "daily-notes"]),
        Some(json!({ "folder": "Journal/Daily", "format": "YYYY-MM-DD dddd", "autorun": true })),
    );
    assert_eq!(
        relative(root.path(), NoteSource::Daily),
        "Journal/Daily/2026-09-27 Sunday.md"
    );
    core(root.path(), json!({ "daily-notes": false }), None);
    assert_eq!(detect_source(root.path()).unwrap(), None);
    assert!(matches!(
        resolve_layout(root.path(), NoteSource::Daily),
        Err(LogError::PluginDisabled { .. })
    ));
}

#[test]
fn both_plugins_enabled_suggests_periodic_but_each_source_resolves_to_its_own() {
    let root = vault();
    periodic(root.path(), FIXTURE);
    core(
        root.path(),
        json!({ "daily-notes": true }),
        Some(json!({ "folder": "notes" })),
    );
    assert_eq!(
        detect_source(root.path()).unwrap(),
        Some(NoteSource::Periodic)
    );
    assert_eq!(
        relative(root.path(), NoteSource::Periodic),
        "daily/2026/2026-09/2026-09-27.md"
    );
    assert_eq!(
        relative(root.path(), NoteSource::Daily),
        "notes/2026-09-27.md"
    );
}

#[test]
fn missing_vaults_and_broken_settings_are_errors() {
    let plain = tempfile::tempdir().unwrap();
    assert!(matches!(
        detect_source(plain.path()),
        Err(LogError::NotAVault { .. })
    ));
    assert!(matches!(
        resolve_layout(plain.path(), NoteSource::Daily),
        Err(LogError::NotAVault { .. })
    ));
    fs::write(plain.path().join(".obsidian"), "a file").unwrap();
    assert!(matches!(
        detect_source(plain.path()),
        Err(LogError::NotAVault { .. })
    ));

    for (file, contents) in [
        ("community-plugins.json", "{"),
        ("community-plugins.json", r#"{"periodic-notes": true}"#),
        ("core-plugins.json", "[1"),
        ("core-plugins.json", r#""daily-notes""#),
        ("core-plugins.json", r#"{"daily-notes": "yes"}"#),
    ] {
        let root = vault();
        write(root.path(), file, contents);
        let error = detect_source(root.path()).unwrap_err();
        assert!(
            matches!(error, LogError::PluginSettings { .. }),
            "{file}: {contents}"
        );
        assert!(error.to_string().contains(file), "{error}");
    }

    for data in [
        "not json",
        "[]",
        r#"{"daily": 3}"#,
        r#"{"daily": {"enabled": "yes"}}"#,
        r#"{"daily": {"enabled": true, "folder": 1}}"#,
        r#"{"calendarSets": {}}"#,
    ] {
        let root = vault();
        periodic(root.path(), data);
        let error = resolve_layout(root.path(), NoteSource::Periodic).unwrap_err();
        assert!(matches!(error, LogError::PluginSettings { .. }), "{data}");
        assert!(error.to_string().contains("data.json"), "{error}");
    }

    let root = vault();
    core(
        root.path(),
        json!({ "daily-notes": true }),
        Some(json!({ "format": 5 })),
    );
    assert!(matches!(
        resolve_layout(root.path(), NoteSource::Daily),
        Err(LogError::PluginSettings { .. })
    ));
    core(
        root.path(),
        json!({ "daily-notes": true }),
        Some(json!({ "format": "YYYY-MM-DD HH:mm" })),
    );
    let error = resolve_layout(root.path(), NoteSource::Daily).unwrap_err();
    assert!(matches!(error, LogError::DateFormat(_)), "{error}");
    assert!(error.to_string().contains("\"HH\""), "{error}");
}

#[test]
fn layouts_normalize_folders_and_reject_escaping_paths() {
    let day = date(2026, 1, 5);
    let layout = NoteLayout::new(" /Journal//Daily/ ", "").unwrap();
    assert_eq!(layout.describe(), "Journal/Daily/YYYY-MM-DD");
    assert_eq!(
        layout.relative_path(day).unwrap(),
        "Journal/Daily/2026-01-05.md"
    );
    let nested = NoteLayout::new("", "YYYY//MM/[x]DD").unwrap();
    assert_eq!(nested.relative_path(day).unwrap(), "2026/01/x05.md");
    for (folder, format) in [("..", "YYYY"), ("a/../b", "YYYY"), ("C:", "YYYY")] {
        assert!(matches!(
            NoteLayout::new(folder, format),
            Err(LogError::NotePath(_))
        ));
    }
    for format in ["[../]YYYY", "[/]", "[.]", "[a\\b]YYYY"] {
        let layout = NoteLayout::new("daily", format).unwrap();
        assert!(
            matches!(layout.relative_path(day), Err(LogError::NotePath(_))),
            "{format}"
        );
    }
}
