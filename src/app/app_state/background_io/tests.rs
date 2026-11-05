use super::{
    LoadedPathResult, PendingBackgroundAction, PendingEncodingComplianceAction,
    PendingSessionHydrationAction, PendingTextMetadataAction, ScratchpadApp,
    cold_session_tab_buffer_ids,
};
use crate::app::app_state::PendingOpenTabsAction;
use crate::app::app_state::test_support;
use crate::app::domain::{BufferState, TabManager, TextFormatMetadata, WorkspaceTab};
use crate::app::services::session_store::SessionStore;

fn test_app() -> ScratchpadApp {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.keep();
    test_support::test_app(&root)
}

fn app_with_buffer(buffer: BufferState) -> ScratchpadApp {
    let mut app = test_app();
    app.tab_manager = TabManager::for_test_tabs(vec![WorkspaceTab::new(buffer)]);
    app
}

#[test]
fn forgetting_history_rejects_a_delayed_reopen_result() {
    let mut app = app_with_buffer(BufferState::new("live".into(), "live text".into(), None));
    let closed = WorkspaceTab::new(BufferState::new(
        "closed".into(),
        "closed text".into(),
        None,
    ));
    let document = closed.active_buffer().temp_id.clone();
    app.state.background_io.insert_pending_background_action(
        731,
        PendingBackgroundAction::ReopenRetained(crate::app::app_state::PendingRetainedOpenAction {
            expected_epoch: app.state.persistence.session_store.history_epoch(),
            document: Some(document),
            node: Some(1),
        }),
    );
    let live = app.tab_manager.active_tab().unwrap().active_buffer().id;
    assert!(crate::app::app_state::workspace::mutation::clear_text_history(&mut app, &[live]));
    app.apply_retained_reopened_result(731, Ok((Box::new(closed), None)));
    assert_eq!(app.tab_manager.tabs.len(), 1);
    assert_eq!(
        app.tab_manager.active_tab().unwrap().active_buffer().text(),
        "live text"
    );
}

#[test]
fn a_delayed_commit_acknowledges_only_its_captured_activity() {
    use crate::app::services::session_store::{
        SessionCaptureAck,
        activity::{ActivityEvent, ActivityKind},
    };
    let mut app = app_with_buffer(BufferState::new("live".into(), "text".into(), None));
    let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
    let first = ActivityEvent::new(ActivityKind::Opened, buffer);
    let later = ActivityEvent::new(ActivityKind::Closed, buffer);
    let sequence = later.sequence;
    app.state.background_io.insert_pending_background_action(
        981,
        PendingBackgroundAction::PersistSession(
            crate::app::app_state::PendingSessionPersistAction {
                close: None,
                acknowledgement: SessionCaptureAck {
                    activity_sequences: vec![first.sequence],
                    ..SessionCaptureAck::default()
                },
            },
        ),
    );
    app.state.persistence.pending_activity = vec![first, later];
    app.tab_manager.mark_session_dirty();
    app.apply_session_persisted_result(981, Ok(()));
    assert_eq!(app.state.persistence.pending_activity.len(), 1);
    assert_eq!(app.state.persistence.pending_activity[0].sequence, sequence);
    assert!(app.tab_manager.session_dirty);
}

#[test]
fn background_wait_reports_timeout_instead_of_claiming_completion() {
    let mut app = test_app();
    let timeout = std::time::Duration::ZERO;
    assert!(app.wait_for_background_io_idle_timeout(timeout));
    app.state.background_io.pending_background_actions.insert(
        999,
        PendingBackgroundAction::OpenTabs(PendingOpenTabsAction {
            accumulator: crate::app::services::file_controller::OpenBatchSummary::default(),
        }),
    );
    assert!(!app.wait_for_background_io_idle_timeout(timeout));
    app.state
        .background_io
        .pending_background_actions
        .remove(&999);
    assert!(app.wait_for_background_io_idle_timeout(timeout));
}

#[test]
fn text_metadata_result_updates_matching_buffer_and_clears_pending_action() {
    let buffer = BufferState::new("sample.txt".to_owned(), "one".to_owned(), None);
    let buffer_id = buffer.id;
    let revision = buffer.document_revision();
    let mut app = app_with_buffer(buffer);
    app.state.background_io.pending_background_actions.insert(
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

    assert!(
        !app.state
            .background_io
            .pending_background_actions
            .contains_key(&42)
    );
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
    app.state.background_io.pending_background_actions.insert(
        7,
        PendingBackgroundAction::RefreshEncodingCompliance(PendingEncodingComplianceAction {
            buffer_id,
            revision: stale_revision,
        }),
    );

    app.apply_encoding_compliance_refreshed_result(7, buffer_id, stale_revision, Ok(true));

    assert!(
        !app.state
            .background_io
            .pending_background_actions
            .contains_key(&7)
    );
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
    app.state.background_io.pending_background_actions.insert(
        3,
        PendingBackgroundAction::OpenTabs(PendingOpenTabsAction {
            accumulator: crate::app::services::file_controller::OpenBatchSummary::default(),
        }),
    );

    app.apply_paths_loaded_result(
        3,
        vec![LoadedPathResult {
            missing: false,
            retained_tab: None,
            path,
            disk_state: None,
            result: Ok(buffer),
        }],
        true,
    );

    assert!(
        app.state
            .background_io
            .pending_background_actions
            .contains_key(&3)
    );
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
            |_, _| true,
            |tab_index, tab, cold_session_tab| {
                if let Some(tab) = cold_session_tab {
                    cold_payload = Some(tab);
                }
                streamed_tabs.push((tab_index, tab));
                true
            },
        )
        .unwrap();
    streamed_tabs.sort_by_key(|(tab_index, _)| *tab_index);

    let mut app = test_app();
    app.state.persistence.session_store = store.clone();
    app.tab_manager
        .set_tabs(streamed_tabs.into_iter().map(|(_, tab)| tab).collect(), 0);
    let cold_payload = cold_payload.unwrap();
    app.tab_manager
        .set_cold_session_tab(1, cold_payload.clone());
    app.state.background_io.pending_background_actions.insert(
        77,
        PendingBackgroundAction::HydrateSessionTab(PendingSessionHydrationAction {
            progressive: true,
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
    let (hydrated_tab, restore_status) = store.restore_cold_session_tab(cold_payload).unwrap();
    app.apply_session_tab_hydrated_result(77, 1, Ok((Box::new(hydrated_tab), restore_status)));

    assert!(
        !app.state
            .background_io
            .pending_background_actions
            .contains_key(&77)
    );
    assert!(app.tab_manager.cold_session_tabs().is_empty());
    assert_eq!(
        app.tab_manager.tabs.as_slice()[2].active_buffer().text(),
        "hidden body"
    );
}

#[test]
fn failed_hydration_keeps_cold_recovery_payload() {
    let mut app = app_with_buffer(BufferState::new("recover".into(), "original".into(), None));
    let cold = crate::app::services::session_store::cold_tab_from_workspace_tab(
        &app.tab_manager.tabs.as_slice()[0],
    );
    let expected_buffer_ids = cold_session_tab_buffer_ids(&cold);
    app.tab_manager.set_cold_session_tab(0, cold);
    app.state.background_io.pending_background_actions.insert(
        78,
        PendingBackgroundAction::HydrateSessionTab(PendingSessionHydrationAction {
            progressive: true,
            expected_buffer_ids,
        }),
    );
    app.apply_session_tab_hydrated_result(78, 0, Err("corrupt recovery content".into()));
    assert!(app.tab_manager.cold_session_tabs().contains_key(&0));
    assert_eq!(
        app.tab_manager.tabs.as_slice()[0].active_buffer().text(),
        "original"
    );
    assert!(app.state.status.current.is_some());
    assert!(
        !app.state
            .background_io
            .pending_background_actions
            .contains_key(&78)
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

#[test]
fn timeline_cold_hydration_runs_conflict_search_and_restore_status_updates() {
    use crate::app::services::{
        file_service::FileService,
        session_store::{RestoreStatus, RestoreStatusLevel, activity::ActivityKind},
    };
    for missing in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("tracked.txt");
        std::fs::write(&path, "saved").unwrap();
        let mut app = test_support::test_app(root.path());
        let mut buffer = FileService::buffer_from_observed(
            &path,
            FileService::read_observed_file(&path).unwrap(),
        );
        buffer
            .document_mut()
            .replace_exact_with_history("local unsaved");
        buffer.is_dirty = true;
        let id = buffer.temp_id.clone();
        let tab = WorkspaceTab::new(buffer);
        let cold = crate::app::services::session_store::cold_tab_from_workspace_tab(&tab);
        let store = app.state.persistence.session_store.clone();
        store.persist(&[tab], 0, 14.0, false).unwrap();
        // Obtain a real unloaded shell using the streaming restore API.
        let mut shell = None;
        store
            .load_streaming(
                |_, _| true,
                |_, tab, _| {
                    shell = Some(tab);
                    true
                },
            )
            .unwrap();
        let mut shell = shell.unwrap();
        shell.active_buffer_mut().replace_text(String::new());
        app.tab_manager.set_tabs(vec![shell], 0);
        app.tab_manager.set_cold_session_tab(0, cold.clone());
        if missing {
            std::fs::remove_file(&path).unwrap();
        } else {
            std::fs::write(&path, "external").unwrap();
        }
        let (tab, _) = store.restore_cold_session_tab(cold).unwrap();
        app.state.search_state.panel.open = true;
        app.state.search_state.runtime.dirty = false;
        app.state.background_io.insert_pending_background_action(
            736,
            PendingBackgroundAction::ReopenRetained(
                crate::app::app_state::PendingRetainedOpenAction {
                    expected_epoch: store.history_epoch(),
                    document: Some(id),
                    node: None,
                },
            ),
        );
        app.apply_retained_reopened_result(
            736,
            Ok((
                Box::new(tab),
                Some(RestoreStatus {
                    level: RestoreStatusLevel::Warning,
                    message: "Hydration warning".into(),
                }),
            )),
        );
        app.wait_for_background_io_idle();
        assert!(app.tab_manager.cold_session_tabs().is_empty());
        assert!(app.state.search_state.runtime.dirty);
        assert!(
            app.state
                .status
                .history
                .iter()
                .any(|entry| entry.text == "Hydration warning")
        );
        assert!(
            app.state
                .persistence
                .pending_activity
                .iter()
                .any(|event| event.kind
                    == if missing {
                        ActivityKind::Missing
                    } else {
                        ActivityKind::DiskConflict
                    })
        );
        assert_eq!(
            app.tab_manager.active_tab().unwrap().active_buffer().text(),
            "local unsaved"
        );
    }
}

#[test]
fn large_prepared_paste_publishes_atomically_and_undo_keeps_the_original_text() {
    use crate::app::domain::{CharCursor, CursorRange};
    let mut app = app_with_buffer(BufferState::new("paste".into(), "base".into(), None));
    let selection = CursorRange::one(CharCursor::new(0));
    app.tab_manager.active_tab_mut().unwrap().layout.views_mut()[0].cursor_range = Some(selection);
    let text = "λ🦀\n".repeat(50_000);
    assert!(crate::app::app_state::workspace::editing::insert_text_in_active_view(&mut app, &text));
    assert_eq!(
        app.tab_manager.active_tab().unwrap().active_buffer().text(),
        "base"
    );
    assert!(app.wait_for_background_io_idle_timeout(std::time::Duration::from_secs(10)));
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    let normalized = crate::app::domain::buffer::normalize_inserted_text_line_endings(
        &text,
        buffer.format.preferred_line_ending_style(),
    )
    .into_owned();
    assert_eq!(buffer.text(), format!("{normalized}base"));
    assert!(buffer.is_dirty);
    buffer.undo_last_text_operation().unwrap();
    assert_eq!(buffer.text(), "base");
}

#[test]
fn raw_bulk_input_is_left_with_a_focused_non_editor_widget() {
    let mut app = app_with_buffer(BufferState::new("paste".into(), "base".into(), None));
    let ctx = eframe::egui::Context::default();
    let other = crate::app::ui::widget_ids::ctx_key("search-query-test");
    ctx.memory_mut(|memory| memory.request_focus(other));
    let mut input = eframe::egui::RawInput {
        events: vec![eframe::egui::Event::Paste("x".repeat(300_000))],
        ..Default::default()
    };
    eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
    assert_eq!(input.events.len(), 1);
    assert!(
        !app.tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .is_preparing_text()
    );
}

#[test]
fn large_prepared_paste_preserves_independent_split_carets() {
    use crate::app::domain::{CharCursor, CursorRange, SplitAxis};
    let mut app = app_with_buffer(BufferState::new("split".into(), "base".into(), None));
    let tab = app.tab_manager.active_tab_mut().unwrap();
    let origin = tab.layout.active_view_id();
    let other = tab.split_active_view(SplitAxis::Vertical).unwrap();
    tab.activate_view(origin);
    for (id, index) in [(origin, 0), (other, 3)] {
        let (buffer, view) = tab.buffer_and_view_mut(id).unwrap();
        view.set_cursor_range_anchored(buffer, CursorRange::one(CharCursor::new(index)));
    }
    let text = "λ".repeat(150_000);
    assert!(crate::app::app_state::workspace::editing::insert_text_in_active_view(&mut app, &text));
    assert!(app.wait_for_background_io_idle_timeout(std::time::Duration::from_secs(10)));
    let tab = app.tab_manager.active_tab().unwrap();
    assert_eq!(
        tab.layout
            .view(origin)
            .unwrap()
            .cursor_range
            .unwrap()
            .primary
            .index,
        150_000
    );
    assert_eq!(
        tab.layout
            .view(other)
            .unwrap()
            .cursor_range
            .unwrap()
            .primary
            .index,
        150_003
    );
}

#[test]
fn a_close_keeps_input_waiting_for_its_view_to_replay() {
    let mut app = app_with_buffer(BufferState::new("queued".into(), "base".into(), None));
    app.tab_manager.active_tab_mut().unwrap().layout.views_mut()[0]
        .deferred_editor_events
        .push(eframe::egui::Event::Text("kept".into()));
    assert!(crate::app::services::session_manager::persist_tab_close(&mut app, 0).is_err());
    assert_eq!(app.tab_manager.tabs.len(), 1);
    assert_eq!(
        app.tab_manager.active_tab().unwrap().layout.views()[0]
            .deferred_editor_events
            .len(),
        1
    );
}
