use super::{
    LoadedPathResult, PendingBackgroundAction, PendingEncodingComplianceAction,
    PendingSessionHydrationAction, PendingTextMetadataAction, ScratchpadApp, StartupOptions,
    cold_session_tab_buffer_ids,
};
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
        cold_session_tabs: Default::default(),
    };
    app.tab_manager.rebuild_buffer_tab_index();
    app
}

#[test]
fn text_metadata_result_updates_matching_buffer_and_clears_pending_action() {
    let buffer = BufferState::new("sample.txt".to_owned(), "one".to_owned(), None);
    let buffer_id = buffer.id;
    let revision = buffer.document_revision();
    let mut app = app_with_buffer(buffer);
    app.state.io.pending_background_actions.insert(
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
            crate::app::domain::buffer::BufferLength {
                bytes: 7,
                chars: 7,
                lines: 2,
            },
            2,
            crate::app::domain::TextArtifactSummary::default(),
            format,
        )),
    );

    assert!(!app.state.io.pending_background_actions.contains_key(&42));
    assert_eq!(
        app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .line_count,
        2
    );
}

#[test]
fn stale_encoding_compliance_result_clears_action_without_mutating_buffer() {
    let buffer = BufferState::new("sample.txt".to_owned(), "plain".to_owned(), None);
    let buffer_id = buffer.id;
    let stale_revision = buffer.document_revision().saturating_add(1);
    let mut app = app_with_buffer(buffer);
    app.state.io.pending_background_actions.insert(
        7,
        PendingBackgroundAction::RefreshEncodingCompliance(PendingEncodingComplianceAction {
            buffer_id,
            revision: stale_revision,
        }),
    );

    app.apply_encoding_compliance_refreshed_result(7, buffer_id, stale_revision, Ok(true));

    assert!(!app.state.io.pending_background_actions.contains_key(&7));
    assert!(
        !app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .has_non_compliant_characters
    );
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
    app.state.io.pending_background_actions.insert(
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

    assert!(app.state.io.pending_background_actions.contains_key(&3));
    assert_eq!(app.tab_manager.tabs.as_slice().len(), 2);
}

#[test]
fn progressive_session_hydration_replaces_matching_cold_shell_after_index_shift() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().join("session"));
    let visible = WorkspaceTab::new(BufferState::new(
        "visible.txt".to_owned(),
        "visible".to_owned(),
        None,
    ));
    let hidden = WorkspaceTab::new(BufferState::new(
        "hidden.txt".to_owned(),
        "hidden body".to_owned(),
        None,
    ));
    store.persist(&[visible, hidden], 0, 16.0, false).unwrap();

    let mut streamed_tabs = Vec::new();
    let mut cold_payload = None;
    store
        .load_streaming(
            |_, _, _| true,
            |tab_index, tab, cold_session_tab| {
                if let Some(tab) = cold_session_tab {
                    cold_payload = Some(tab.clone());
                }
                streamed_tabs.push((tab_index, tab));
                true
            },
        )
        .unwrap();
    streamed_tabs.sort_by_key(|(tab_index, _)| *tab_index);

    let mut app = test_app();
    app.state.session_store = store.clone();
    app.tab_manager
        .set_tabs(streamed_tabs.into_iter().map(|(_, tab)| tab).collect(), 0);
    let cold_payload = cold_payload.unwrap();
    app.tab_manager
        .set_cold_session_tab(1, cold_payload.clone());
    app.state.io.pending_background_actions.insert(
        77,
        PendingBackgroundAction::HydrateSessionTab(PendingSessionHydrationAction {
            expected_buffer_ids: cold_session_tab_buffer_ids(&cold_payload),
        }),
    );

    app.tab_manager.insert_tab(
        0,
        WorkspaceTab::new(BufferState::new(
            "new.txt".to_owned(),
            "new".to_owned(),
            None,
        )),
    );
    let (hydrated_tab, restore_status) = store.restore_cold_session_tab(cold_payload);
    app.apply_session_tab_hydrated_result(77, 1, restore_status, hydrated_tab);

    assert!(!app.state.io.pending_background_actions.contains_key(&77));
    assert!(app.tab_manager.cold_session_tabs().is_empty());
    assert_eq!(
        app.tab_manager.tabs.as_slice()[2].active_buffer().text(),
        "hidden body"
    );
}

#[test]
fn unknown_background_result_is_ignored() {
    let mut app = test_app();
    let original_tabs = app.tab_manager.tabs.as_slice().len();

    app.apply_paths_loaded_result(999, Vec::new(), false);
    app.apply_session_persisted_result(999, Err("ignored".to_owned()));

    assert_eq!(app.tab_manager.tabs.as_slice().len(), original_tabs);
    assert!(app.state.status.current.is_none());
}
