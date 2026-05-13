use super::{
    AppDiagnostic, AppDiagnosticKind, DiagnosticsState, ERROR_LOG_NAME, append_diagnostic_to_path,
    build_io_diagnostic, extract_hexes, should_capture_log_record,
};
use log::{Level, Metadata};
use std::fs;

#[test]
fn append_creates_error_log() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested").join(ERROR_LOG_NAME);
    let diagnostic = AppDiagnostic::new(AppDiagnosticKind::Io, "created");

    append_diagnostic_to_path(&path, &diagnostic).unwrap();

    let contents = fs::read_to_string(path).unwrap();
    assert!(contents.contains("created"));
}

#[test]
fn append_does_not_overwrite_existing_log() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(ERROR_LOG_NAME);

    append_diagnostic_to_path(&path, &AppDiagnostic::new(AppDiagnosticKind::Io, "first")).unwrap();
    append_diagnostic_to_path(&path, &AppDiagnostic::new(AppDiagnosticKind::Io, "second")).unwrap();

    let contents = fs::read_to_string(path).unwrap();
    assert!(contents.contains("first"));
    assert!(contents.contains("second"));
}

#[test]
fn structured_io_diagnostic_includes_operation_path_and_details() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(ERROR_LOG_NAME);
    let target_path = directory.path().join("target.txt");
    let diagnostic = build_io_diagnostic(
        "save_file",
        Some(&target_path),
        "test_source",
        "disk full".to_owned(),
        [("encoding", "UTF-8".to_owned())],
    );

    append_diagnostic_to_path(&path, &diagnostic).unwrap();

    let contents = fs::read_to_string(path).unwrap();
    let payload: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
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
    state.begin_frame();

    state.track_widget(
        "id".to_owned(),
        "hex".to_owned(),
        "rect-a".to_owned(),
        "first",
        "loc".to_owned(),
        None,
    );
    state.track_widget(
        "id".to_owned(),
        "hex".to_owned(),
        "rect-b".to_owned(),
        "second",
        "loc".to_owned(),
        None,
    );
    state.begin_frame();
    state.track_widget(
        "id".to_owned(),
        "hex".to_owned(),
        "rect-a".to_owned(),
        "first",
        "loc".to_owned(),
        None,
    );
    state.track_widget(
        "id".to_owned(),
        "hex".to_owned(),
        "rect-b".to_owned(),
        "second",
        "loc".to_owned(),
        None,
    );

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
fn logger_filter_captures_egui_id_warnings() {
    let metadata = Metadata::builder()
        .level(Level::Warn)
        .target("egui")
        .build();

    assert!(should_capture_log_record(&metadata, "id clash detected"));
}

#[test]
fn logger_filter_captures_app_warnings() {
    let metadata = Metadata::builder()
        .level(Level::Warn)
        .target("scratchpad")
        .build();

    assert!(should_capture_log_record(&metadata, "ordinary warning"));
}

#[test]
fn logger_filter_ignores_dependency_warnings() {
    let metadata = Metadata::builder()
        .level(Level::Warn)
        .target("some_dependency")
        .build();

    assert!(!should_capture_log_record(&metadata, "ordinary warning"));
}
