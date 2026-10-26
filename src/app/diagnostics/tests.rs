use super::{
    AppDiagnostic, AppDiagnosticKind, DiagnosticsState, ERROR_LOG_NAME, append_diagnostic_to_path,
    build_io_diagnostic, extract_hexes, should_capture_log_record,
};
use log::{Level, Metadata};
use std::fs;

#[test]
fn structured_log_creates_parent_and_preserves_ordered_records_when_appending() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested").join(ERROR_LOG_NAME);
    let target_path = directory.path().join("target.txt");
    let diagnostic = build_io_diagnostic(
        "save_file",
        Some(&target_path),
        "test_source",
        "disk full".to_owned(),
        [("encoding", "UTF-8".to_owned())],
    );

    append_diagnostic_to_path(&path, &diagnostic).unwrap();

    append_diagnostic_to_path(&path, &AppDiagnostic::new(AppDiagnosticKind::Io, "second")).unwrap();
    let contents = fs::read_to_string(path).unwrap();
    let records: Vec<serde_json::Value> = contents
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["message"], "disk full");
    assert_eq!(records[1]["message"], "second");
    let payload = &records[0];
    assert_eq!(payload["kind"], "io");
    assert_eq!(payload["operation"], "save_file");
    assert_eq!(payload["source"], "test_source");
    assert_eq!(payload["path"], target_path.display().to_string());
    assert_eq!(payload["details"]["encoding"], "UTF-8");
}

#[test]
fn unavailable_log_path_does_not_panic() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_path_buf();
    let mut state = DiagnosticsState::new(path);

    state.append_diagnostic(&AppDiagnostic::new(AppDiagnosticKind::Io, "blocked"));

    assert!(state.last_write_error.is_some());
}

#[test]
fn duplicate_widget_ids_are_deduplicated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(ERROR_LOG_NAME);
    let mut state = DiagnosticsState::new(path.clone());
    for _ in 0..2 {
        state.begin_frame();
        for (rect, kind) in [("rect-a", "first"), ("rect-b", "second")] {
            state.track_widget(
                "id".into(),
                "hex".into(),
                rect.into(),
                kind,
                "loc".into(),
                None,
            );
        }
    }

    let contents = fs::read_to_string(path).unwrap();
    assert_eq!(contents.matches("egui_id_conflict").count(), 1);
}

#[test]
fn extract_hexes_reads_all_warning_ids() {
    let message = "Widget rect [[0.0 0.0] - [1.0 1.0]] changed id between passes: prev ids: [\"AAAA\", \"BBBB\", \"CCCC\"], new ids: [\"DDDD\"]";

    assert_eq!(
        extract_hexes(message, "prev ids: [", "]"),
        vec!["AAAA", "BBBB", "CCCC"]
    );
    assert_eq!(extract_hexes(message, "new ids: [", "]"), vec!["DDDD"]);
}

#[test]
fn changed_id_warning_attributes_all_matching_pass_sites() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(ERROR_LOG_NAME);
    let mut state = DiagnosticsState::new(path.clone());

    state.begin_pass(0);
    state.track_widget(
        "prev-id".to_owned(),
        "BBBB".to_owned(),
        "prev-rect".to_owned(),
        "prev_kind",
        "prev.rs:10".to_owned(),
        Some("prev-parent".to_owned()),
    );
    state.begin_pass(1);
    state.track_widget(
        "new-id".to_owned(),
        "DDDD".to_owned(),
        "new-rect".to_owned(),
        "new_kind",
        "new.rs:20".to_owned(),
        Some("new-parent".to_owned()),
    );

    state.log_record(
        Level::Warn,
        "egui::context",
        "Widget rect [[0.0 0.0] - [1.0 1.0]] changed id between passes: prev ids: [\"AAAA\", \"BBBB\"], new ids: [\"CCCC\", \"DDDD\"]".to_owned(),
    );

    let contents = fs::read_to_string(path).unwrap();
    let payload: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
    assert_eq!(
        payload["details"]["prev_site"],
        "BBBB prev.rs:10 prev_kind pass=0 parent_ui_id=prev-parent"
    );
    assert_eq!(
        payload["details"]["new_site"],
        "DDDD new.rs:20 new_kind pass=1 parent_ui_id=new-parent"
    );
}

#[test]
fn logger_filters_by_severity_origin_and_the_exact_benign_clipboard_error() {
    let empty_clipboard = "arboard paste error: The clipboard contents were not available in the requested format or the clipboard is empty.";
    for (level, target, message, expected) in [
        (Level::Warn, "egui", "id clash detected", true),
        (Level::Warn, "scratchpad", "ordinary warning", true),
        (Level::Warn, "scratchpad::worker", "ordinary warning", true),
        (Level::Warn, "some_dependency", "ordinary warning", false),
        (Level::Warn, "some_dependency", "Widget rect changed", true),
        (Level::Info, "scratchpad", "ordinary warning", false),
        (Level::Debug, "egui", "Widget rect changed", false),
        (
            Level::Error,
            "egui_winit::clipboard",
            empty_clipboard,
            false,
        ),
        (
            Level::Error,
            "egui_winit::clipboard",
            "arboard paste error: clipboard provider is unavailable",
            true,
        ),
        (Level::Error, "egui", empty_clipboard, true),
    ] {
        let metadata = Metadata::builder().level(level).target(target).build();
        assert_eq!(
            should_capture_log_record(&metadata, message),
            expected,
            "{level:?} {target}: {message}"
        );
    }
}
