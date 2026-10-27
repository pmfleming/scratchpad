use super::{
    BufferState, Duration, Instant, ScratchpadApp, SessionStore, SettingsStore, StartupOptions,
    WorkspaceTab, egui, hidden_input, logic, test_app,
};
use crate::app::app_state::{PendingBackgroundAction, PendingStartupRestoreAction, frame};
use crate::app::services::{background_io::BackgroundIoResult, session_manager};

fn deliver(app: &mut ScratchpadApp, result: BackgroundIoResult) {
    let (tx, rx) = std::sync::mpsc::channel();
    app.state.background_io.rx = rx;
    tx.send(result).unwrap();
    app.drain_background_io_results();
}

fn saved_tabs() -> Vec<WorkspaceTab> {
    ["a.txt", "b.txt"]
        .into_iter()
        .map(|name| WorkspaceTab::new(BufferState::new(name.into(), format!("saved {name}"), None)))
        .collect()
}

fn assert_original_session(store: &SessionStore) {
    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.tabs.len(), 2);
    for (tab, name) in restored.tabs.iter().zip(["a.txt", "b.txt"]) {
        assert_eq!(tab.active_buffer().name, name);
        assert_eq!(tab.active_buffer().text(), format!("saved {name}"));
    }
}

#[test]
fn closing_or_dropping_before_first_frame_preserves_the_saved_session() {
    for explicit_close in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::new(root.path().join("session"));
        store.persist(&saved_tabs(), 1, 16.0, false).unwrap();
        let mut app = ScratchpadApp::with_stores_and_runtime_startup(
            store.clone(),
            SettingsStore::new(root.path().join("settings")),
            StartupOptions::default(),
        );
        assert!(app.startup_restore_pending());
        if explicit_close {
            let ctx = egui::Context::default();
            let mut input = hidden_input();
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
            let output = logic(&mut app, &ctx, &input);
            assert!(
                output.viewport_commands[&egui::ViewportId::ROOT]
                    .contains(&egui::ViewportCommand::Close)
            );
            assert!(app.state.window.close_in_progress);
        }
        drop(app);
        assert_original_session(&store);
    }
}

// Hold the restore deterministically, independently of the worker's speed. Results
// can be delivered explicitly to exercise the streamed-prefix and failure windows.
fn hold_restore(app: &mut ScratchpadApp) -> u64 {
    let id = app.state.background_io.allocate_background_request_id();
    app.state.background_io.insert_pending_background_action(
        id,
        PendingBackgroundAction::StartupRestore(PendingStartupRestoreAction {
            startup_options: StartupOptions::default(),
            loaded_from_settings: true,
            restore_started: false,
            streamed_tab_count: 0,
            streamed_tab_indices: Vec::new(),
        }),
    );
    id
}

#[test]
fn placeholder_close_and_streamed_prefix_cannot_save_or_trigger_empty_exit() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    let store = app.state.persistence.session_store.clone();
    store.persist(&saved_tabs(), 1, 16.0, false).unwrap();
    let request_id = hold_restore(&mut app);
    super::super::settings_controller::close_settings(&mut app);
    super::super::workspace_controller::perform_close_tab(&mut app, 0);
    let ctx = egui::Context::default();
    let _ = logic(&mut app, &ctx, &hidden_input());
    assert!(!app.state.window.close_in_progress);
    assert!(!app.state.window.empty_workspace_exit_attempted);
    assert_original_session(&store);

    deliver(
        &mut app,
        BackgroundIoResult::SessionTabRestored {
            request_id,
            tab_index: 0,
            tab: Box::new(saved_tabs().remove(0)),
            cold_session_tab: None,
        },
    );
    assert_eq!(app.tab_manager.tabs.len(), 1);
    app.tab_manager.mark_session_dirty();
    app.state.persistence.last_session_persist =
        Instant::now().checked_sub(Duration::from_secs(2)).unwrap();
    session_manager::maybe_persist_session(&mut app, &ctx);
    assert!(!app.state.background_io.has_pending_persist());
    session_manager::persist_session_now(&mut app).unwrap();
    assert!(app.tab_manager.session_dirty);
    assert_original_session(&store);

    app.set_session_persist_on_drop(true);
    drop(app);
    assert_original_session(&store);
}

#[test]
fn failed_streamed_restore_keeps_the_persistence_fence_after_pending_action_is_removed() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    let store = app.state.persistence.session_store.clone();
    store.persist(&saved_tabs(), 1, 16.0, false).unwrap();
    let request_id = hold_restore(&mut app);
    deliver(
        &mut app,
        BackgroundIoResult::SessionRestored {
            request_id,
            result: Err("injected restore failure".into()),
        },
    );
    assert!(!app.startup_restore_pending());
    assert!(app.state.persistence.session_restore_failed);
    session_manager::persist_session_now(&mut app).unwrap();
    app.set_session_persist_on_drop(true);
    drop(app);
    assert_original_session(&store);
}

#[test]
fn permanently_unavailable_store_does_not_trap_window_or_retry_on_drop() {
    let root = tempfile::tempdir().unwrap();
    let session_root = root.path().join("not-a-directory");
    std::fs::write(&session_root, b"preserve me").unwrap();
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(session_root.clone()),
        SettingsStore::new(root.path().join("settings")),
        StartupOptions {
            restore_session: false,
            restore_session_explicit: true,
            ..Default::default()
        },
    );
    super::super::settings_controller::close_settings(&mut app);
    super::super::workspace_controller::perform_close_tab(&mut app, 0);
    let ctx = egui::Context::default();
    let output = logic(&mut app, &ctx, &hidden_input());
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT].contains(&egui::ViewportCommand::Close)
    );
    assert!(app.state.window.close_in_progress);
    assert!(!app.state.persistence.persist_session_on_drop);
    let statuses = app.state.status.history.len();
    let _ = logic(&mut app, &ctx, &hidden_input());
    assert_eq!(app.state.status.history.len(), statuses);
    drop(app);
    assert_eq!(std::fs::read(session_root).unwrap(), b"preserve me");
}

#[test]
fn automatic_exit_failure_is_attempted_once_and_explicit_close_can_retry() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    super::super::settings_controller::close_settings(&mut app);
    super::super::workspace_controller::perform_close_tab(&mut app, 0);
    let settings_root = root.path().join("blocked-settings");
    std::fs::write(&settings_root, b"block directory creation").unwrap();
    app.state.persistence.settings_store = SettingsStore::new(settings_root.clone());
    let ctx = egui::Context::default();
    let _ = logic(&mut app, &ctx, &hidden_input());
    assert!(!app.state.window.close_in_progress);
    assert!(app.state.window.empty_workspace_exit_attempted);
    let statuses = app.state.status.history.len();
    for _ in 0..3 {
        let _ = logic(&mut app, &ctx, &hidden_input());
    }
    assert_eq!(app.state.status.history.len(), statuses);
    std::fs::remove_file(settings_root).unwrap();
    frame::request_exit(&mut app, &ctx);
    assert!(app.state.window.close_in_progress);
}
