use super::*;
use crate::app::app_state::PendingOpenTabsAction;
use crate::app::domain::{BufferState, TabManager, TextFormatMetadata, WorkspaceTab};
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::SettingsStore;

fn test_app() -> ScratchpadApp {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.keep();
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.join("session")),
        SettingsStore::new(root.join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    app
}

fn app_with_buffer(buffer: BufferState) -> ScratchpadApp {
    let mut app = test_app();
    app.tab_manager = TabManager {
        tabs: vec![WorkspaceTab::new(buffer)],
        active_tab_index: 0,
        pending_action: None,
        session_dirty: false,
        pending_scroll_to_active: false,
        buffer_tab_index: Default::default(),
    };
    app.rebuild_buffer_tab_index();
    app
}

#[test]
fn text_metadata_result_updates_matching_buffer_and_clears_pending_action() {
    let buffer = BufferState::new("sample.txt".to_owned(), "one".to_owned(), None);
    let buffer_id = buffer.id;
    let revision = buffer.document_revision();
    let mut app = app_with_buffer(buffer);
    app.io.pending_background_actions.insert(
        42,
        PendingBackgroundAction::RefreshTextMetadata(PendingTextMetadataAction {
            buffer_id,
            revision,
        }),
    );
    let mut format = TextFormatMetadata::utf8_for_new_file("one\ntwo");
    format.refresh_from_text("one\ntwo");

    app.apply_text_metadata_refreshed_result(
        42,
        buffer_id,
        revision,
        Ok((
            2,
            crate::app::domain::TextArtifactSummary::default(),
            format,
        )),
    );

    assert!(!app.io.pending_background_actions.contains_key(&42));
    assert_eq!(app.tabs()[0].active_buffer().line_count, 2);
}

#[test]
fn stale_encoding_compliance_result_clears_action_without_mutating_buffer() {
    let buffer = BufferState::new("sample.txt".to_owned(), "plain".to_owned(), None);
    let buffer_id = buffer.id;
    let stale_revision = buffer.document_revision().saturating_add(1);
    let mut app = app_with_buffer(buffer);
    app.io.pending_background_actions.insert(
        7,
        PendingBackgroundAction::RefreshEncodingCompliance(PendingEncodingComplianceAction {
            buffer_id,
            revision: stale_revision,
        }),
    );

    app.apply_encoding_compliance_refreshed_result(7, buffer_id, stale_revision, Ok(true));

    assert!(!app.io.pending_background_actions.contains_key(&7));
    assert!(!app.tabs()[0].active_buffer().has_non_compliant_characters);
}

#[test]
fn partial_open_tabs_result_keeps_action_until_terminal_result() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opened.txt");
    let buffer = BufferState::new(
        "opened.txt".to_owned(),
        "opened".to_owned(),
        Some(path.clone()),
    );
    let mut app = test_app();
    app.io.pending_background_actions.insert(
        3,
        PendingBackgroundAction::OpenTabs(PendingOpenTabsAction {
            accumulator: crate::app::services::file_controller::OpenBatchSummary::default(),
        }),
    );

    app.apply_paths_loaded_result(
        3,
        vec![LoadedPathResult {
            path,
            disk_state: None,
            result: Ok(buffer),
        }],
        true,
    );

    assert!(app.io.pending_background_actions.contains_key(&3));
    assert_eq!(app.tabs().len(), 2);
}

#[test]
fn unknown_background_result_is_ignored() {
    let mut app = test_app();
    let original_tabs = app.tabs().len();

    app.apply_paths_loaded_result(999, Vec::new(), false);
    app.apply_session_persisted_result(999, Err("ignored".to_owned()));

    assert_eq!(app.tabs().len(), original_tabs);
    assert!(app.status.current.is_none());
}
