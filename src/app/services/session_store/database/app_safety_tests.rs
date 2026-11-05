use super::super::{SessionStore, database};
use crate::app::app_state::{
    ScratchpadApp,
    test_support::{close_input, logic},
};
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::{session_manager, settings_store::SettingsStore};
use crate::app::startup::StartupOptions;
use eframe::egui;
use futures_lite::future::block_on;
use std::time::Instant;

fn tabs() -> Vec<WorkspaceTab> {
    ["healthy", "damaged"]
        .into_iter()
        .map(|text| WorkspaceTab::new(BufferState::new(text.into(), text.into(), None)))
        .collect()
}

#[test]
fn failed_blocking_restore_cannot_erase_healthy_tabs_after_quarantine() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path().join("session"));
    let tabs = tabs();
    store.persist(&tabs, 0, 16.0, false).unwrap();
    let id = tabs[1].active_buffer().temp_id.clone();
    store
        .worker()
        .unwrap()
        .call(move |db| {
            block_on(async {
                db.conn
                    .execute("UPDATE document_pieces SET body=x'00' WHERE id=?", [id])
                    .await
                    .map_err(database::error)?;
                Ok(())
            })
        })
        .unwrap();
    let mut app = ScratchpadApp::with_stores_and_startup(
        store.clone(),
        SettingsStore::new(root.path().join("settings")),
        StartupOptions::default(),
    );
    assert!(app.state.persistence.session_restore_failed);
    // The DB still permits healthy streaming clients to save. The fallback app must not.
    assert!(!store.writes_blocked());
    app.tab_manager.mark_session_dirty();
    app.state.persistence.last_session_persist = Instant::now()
        .checked_sub(std::time::Duration::from_secs(2))
        .unwrap();
    session_manager::maybe_persist_session(&mut app, &egui::Context::default());
    assert!(!app.state.background_io.has_pending_persist());
    session_manager::persist_session_now(&mut app).unwrap();
    drop(app); // default drop persistence is enabled
    store
        .worker()
        .unwrap()
        .call(|db| {
            block_on(async {
                assert_eq!(db.integer("SELECT count(*) FROM tabs").await?, 2);
                assert_eq!(db.integer("SELECT count(*) FROM document_pieces").await?, 2);
                assert_eq!(db.integer("SELECT count(*) FROM quarantine").await?, 1);
                Ok(())
            })
        })
        .unwrap();
    assert_eq!(
        store.load_startup_visible().unwrap().unwrap().tabs[0]
            .active_buffer()
            .text(),
        "healthy"
    );
}

#[test]
fn incompatible_session_requires_explicit_reset_then_saves_and_restarts() {
    let root = tempfile::tempdir().unwrap();
    let session_root = root.path().join("session");
    let store = SessionStore::new(session_root.clone());
    store.persist(&tabs(), 0, 16.0, false).unwrap();
    store
        .worker()
        .unwrap()
        .call(|db| {
            block_on(async {
                db.conn
                    .execute("PRAGMA user_version=9", ())
                    .await
                    .map_err(database::error)?;
                db.checkpoint().await
            })
        })
        .unwrap();
    drop(store);
    let previous = std::fs::read(session_root.join("session.db")).unwrap();
    let source = root.path().join("user-note.txt");
    std::fs::write(&source, "saved text").unwrap();
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(session_root.clone()),
        SettingsStore::new(root.path().join("settings")),
        StartupOptions::default(),
    );
    assert!(app.state.persistence.session_restore_failed);
    assert!(app.state.dialogs.status_history.is_open());
    assert!(
        app.state
            .status
            .current
            .as_ref()
            .unwrap()
            .detail
            .as_ref()
            .unwrap()
            .contains("version 9")
    );
    assert_eq!(
        std::fs::read(session_root.join("session.db")).unwrap(),
        previous
    );
    app.tab_manager.set_tabs(tabs(), 0);
    let ctx = egui::Context::default();
    session_manager::start_new_session(&mut app, &ctx);
    assert!(app.state.persistence.session_reset.is_some());
    // Closing cannot terminate the backup or lose edits made during the reset.
    crate::app::app_state::frame::request_exit(&mut app, &ctx);
    assert!(!app.state.window.close_in_progress);
    app.tab_manager.tabs.as_mut_slice()[0]
        .active_buffer_mut()
        .replace_text("new edits".into());
    wait_for_reset(&mut app);
    assert!(!app.state.persistence.session_restore_failed);
    assert!(!app.state.persistence.session_store.writes_blocked());
    let detail = app
        .state
        .status
        .current
        .as_ref()
        .unwrap()
        .detail
        .as_ref()
        .unwrap();
    let backup = std::path::PathBuf::from(
        detail
            .lines()
            .next()
            .unwrap()
            .strip_prefix("Previous session moved to: ")
            .unwrap(),
    );
    assert_eq!(std::fs::read(backup.join("session.db")).unwrap(), previous);
    assert!(detail.contains("temporary-file cleanup"));
    let output = logic(&mut app, &ctx, &close_input());
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT].contains(&egui::ViewportCommand::Close)
    );
    assert!(app.state.window.close_in_progress);
    assert!(!app.state.persistence.persist_session_on_drop);
    drop(app);
    assert_eq!(std::fs::read_to_string(source).unwrap(), "saved text");
    let restored = SessionStore::new(session_root).load().unwrap().unwrap();
    assert_eq!(restored.tabs.len(), 2);
    assert_eq!(restored.tabs[0].active_buffer().text(), "new edits");
    std::fs::remove_dir_all(backup).unwrap();
}

fn wait_for_reset(app: &mut ScratchpadApp) {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    while app.state.persistence.session_reset.is_some() && Instant::now() < deadline {
        session_manager::poll_session_reset(app);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(app.state.persistence.session_reset.is_none());
}

#[test]
fn failed_reset_keeps_saving_paused_and_can_be_retried() {
    let root = tempfile::tempdir().unwrap();
    let session_root = root.path().join("session");
    std::fs::write(&session_root, "blocked directory").unwrap();
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(session_root.clone()),
        SettingsStore::new(root.path().join("settings")),
        StartupOptions::default(),
    );
    let ctx = egui::Context::default();
    session_manager::start_new_session(&mut app, &ctx);
    wait_for_reset(&mut app);
    assert!(app.state.persistence.session_restore_failed);
    assert!(
        app.state
            .status
            .current
            .as_ref()
            .unwrap()
            .text
            .contains("try again")
    );
    assert_eq!(
        std::fs::read_to_string(&session_root).unwrap(),
        "blocked directory"
    );
    std::fs::remove_file(&session_root).unwrap();
    session_manager::start_new_session(&mut app, &ctx);
    wait_for_reset(&mut app);
    assert!(!app.state.persistence.session_restore_failed);
    let detail = app
        .state
        .status
        .current
        .as_ref()
        .unwrap()
        .detail
        .as_ref()
        .unwrap();
    let backup = detail
        .lines()
        .next()
        .unwrap()
        .strip_prefix("Previous session moved to: ")
        .unwrap();
    std::fs::remove_dir_all(backup).unwrap();
}

#[test]
fn final_split_view_close_retains_latest_text_and_rolls_back_before_removing_the_view() {
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::app_state::test_support::test_app(root.path());
    let mut tab = WorkspaceTab::new(BufferState::new("first".into(), "first".into(), None));
    let view = tab
        .open_buffer_as_split(
            BufferState::new("second".into(), "latest unsaved".into(), None),
            crate::app::domain::SplitAxis::Vertical,
            true,
            0.5,
        )
        .unwrap();
    let id = tab.buffer_for_view(view).unwrap().temp_id.clone();
    app.tab_manager.set_tabs(vec![tab], 0);
    let store = app.state.persistence.session_store.clone();
    store.worker().unwrap().call(|db| block_on(async {
        db.conn.execute_batch("CREATE TRIGGER refuse_view_archive BEFORE INSERT ON closed_tabs BEGIN SELECT RAISE(ABORT,'transient failure'); END;").await.map_err(database::error)
    })).unwrap();
    crate::app::commands::perform_close_view(&mut app, view);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    assert_eq!(app.tab_manager.active_tab().unwrap().layout.leaf_count(), 2);
    assert_eq!(
        app.tab_manager
            .active_tab()
            .unwrap()
            .buffer_for_view(view)
            .unwrap()
            .text(),
        "latest unsaved"
    );
    store
        .worker()
        .unwrap()
        .call(|db| {
            block_on(async {
                db.conn
                    .execute("DROP TRIGGER refuse_view_archive", ())
                    .await
                    .map_err(database::error)?;
                Ok(())
            })
        })
        .unwrap();
    crate::app::commands::perform_close_view(&mut app, view);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    assert_eq!(app.tab_manager.active_tab().unwrap().layout.leaf_count(), 1);
    drop(app);
    drop(store);
    let store = SessionStore::new(root.path().join("session"));
    let archive = store.closed_tabs().unwrap().remove(0);
    let restored = store.restore_closed_tab(&archive.token).unwrap();
    assert_eq!(restored.active_buffer().temp_id, id);
    assert_eq!(restored.active_buffer().text(), "latest unsaved");
    assert!(
        !restored
            .active_buffer()
            .document()
            .history_entries()
            .is_empty()
    );
}

#[test]
fn failed_archive_commit_leaves_the_live_tab_open() {
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::app_state::test_support::test_app(root.path());
    app.tab_manager.set_tabs(tabs(), 0);
    let store = app.state.persistence.session_store.clone();
    crate::app::services::session_manager::persist_session_now(&mut app).unwrap();
    app.tab_manager.tabs.as_mut_slice()[0]
        .active_buffer_mut()
        .replace_text("latest unsaved text".into());
    store.worker().unwrap().call(|db| block_on(async {
        db.conn.execute_batch("CREATE TRIGGER refuse_archive BEFORE INSERT ON closed_tabs BEGIN SELECT RAISE(ABORT, 'transient failure'); END;").await.map_err(database::error)
    })).unwrap();
    let count = app.tab_manager.tabs.len();
    crate::app::app_state::workspace_controller::perform_close_tab_no_persist(&mut app, 0);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    assert_eq!(app.tab_manager.tabs.len(), count);
    assert_eq!(
        app.tab_manager.tabs.as_slice()[0].active_buffer().text(),
        "latest unsaved text"
    );
    store
        .worker()
        .unwrap()
        .call(|db| {
            block_on(async {
                db.conn
                    .execute("DROP TRIGGER refuse_archive", ())
                    .await
                    .map_err(database::error)?;
                Ok(())
            })
        })
        .unwrap();
    crate::app::app_state::workspace_controller::perform_close_tab_no_persist(&mut app, 0);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    assert_eq!(app.tab_manager.tabs.len(), count - 1);
    let archive = store.closed_tabs().unwrap().remove(0);
    assert_eq!(
        store
            .restore_closed_tab(&archive.token)
            .unwrap()
            .active_buffer()
            .text(),
        "latest unsaved text"
    );
}

#[test]
fn retryable_session_save_failure_keeps_window_open_and_does_not_loop() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path().join("session"));
    store.persist(&tabs(), 0, 16.0, false).unwrap();
    let mut app = ScratchpadApp::with_stores_and_startup(
        store.clone(),
        SettingsStore::new(root.path().join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    store.worker().unwrap().call(|db| block_on(async {
        db.conn.execute_batch("CREATE TRIGGER refuse BEFORE INSERT ON session_state BEGIN SELECT RAISE(ABORT, 'transient failure'); END;").await.map_err(database::error)
    })).unwrap();
    crate::app::app_state::settings_controller::close_settings(&mut app);
    app.tab_manager.set_tabs(Vec::new(), 0);
    let ctx = egui::Context::default();
    let input = egui::RawInput::default();
    let _ = logic(&mut app, &ctx, &input);
    assert!(!app.state.window.close_in_progress);
    assert!(!store.writes_blocked());
    let count = app.state.status.history.len();
    for _ in 0..3 {
        let _ = logic(&mut app, &ctx, &input);
    }
    assert_eq!(app.state.status.history.len(), count);
    store
        .worker()
        .unwrap()
        .call(|db| {
            block_on(async {
                db.conn
                    .execute("DROP TRIGGER refuse", ())
                    .await
                    .map_err(database::error)?;
                Ok(())
            })
        })
        .unwrap();
    let _ = logic(&mut app, &ctx, &close_input());
    assert!(app.state.window.close_in_progress);
}

#[test]
fn closing_cold_split_retains_both_documents_and_remaining_layout() {
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::app_state::test_support::test_app(root.path());
    let mut tab = WorkspaceTab::new(BufferState::new(
        "first".into(),
        "first unsaved".into(),
        None,
    ));
    let view = tab
        .open_buffer_as_split(
            BufferState::new("second".into(), "second unsaved".into(), None),
            crate::app::domain::SplitAxis::Vertical,
            true,
            0.5,
        )
        .unwrap();
    let cold = super::super::cold_tab_from_workspace_tab(&tab);
    let store = app.state.persistence.session_store.clone();
    store.persist(&[tab], 0, 14.0, false).unwrap();
    let shell = super::super::Restorer::cold_tab_shell(&cold);
    app.tab_manager.set_tabs(vec![shell], 0);
    app.tab_manager.set_cold_session_tab(0, cold);
    crate::app::commands::perform_close_view(&mut app, view);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    assert_eq!(app.tab_manager.active_tab().unwrap().layout.leaf_count(), 1);
    session_manager::persist_session_now(&mut app).unwrap();
    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.tabs[0].active_buffer().text(), "first unsaved");
    assert_eq!(restored.tabs[0].layout.leaf_count(), 1);
    let archive = store.closed_tabs().unwrap().remove(0);
    assert_eq!(
        store
            .restore_closed_tab(&archive.token)
            .unwrap()
            .active_buffer()
            .text(),
        "second unsaved"
    );
}

#[test]
fn unavailable_session_store_does_not_trap_tabs_or_split_panes() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("session"), "unavailable").unwrap();
    let mut app = crate::app::app_state::test_support::test_app(root.path());
    assert!(app.state.persistence.session_restore_failed);
    let path = root.path().join("saved.txt");
    std::fs::write(&path, "saved").unwrap();
    let load = || {
        crate::app::services::file_service::FileService::buffer_from_observed(
            &path,
            crate::app::services::file_service::FileService::read_observed_file(&path).unwrap(),
        )
    };
    let mut tab = WorkspaceTab::new(load());
    let view = tab
        .open_buffer_as_split(load(), crate::app::domain::SplitAxis::Vertical, true, 0.5)
        .unwrap();
    app.tab_manager
        .set_tabs(vec![tab, WorkspaceTab::untitled()], 0);
    crate::app::commands::perform_close_view(&mut app, view);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    assert_eq!(app.tab_manager.active_tab().unwrap().layout.leaf_count(), 1);
    crate::app::app_state::workspace_controller::perform_close_tab_no_persist(&mut app, 0);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    assert_eq!(app.tab_manager.tabs.len(), 1);
    assert!(
        app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .path
            .is_none()
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("session")).unwrap(),
        "unavailable"
    );
}

#[test]
fn cold_documents_survive_a_failed_backup_and_retry_after_writer_retirement() {
    use super::super::{
        Restorer, SessionActiveSurface, SessionPersistRequest, cold_tab_from_workspace_tab,
    };
    let root = tempfile::tempdir().unwrap();
    let mut store = SessionStore::new(root.path().to_owned());
    let text = "retained λ🦀\r\n".repeat(50_000);
    let tab = WorkspaceTab::new(BufferState::new("cold".into(), text.clone(), None));
    store
        .persist(std::slice::from_ref(&tab), 0, 16.0, false)
        .unwrap();
    let cold = cold_tab_from_workspace_tab(&tab);
    let shell = Restorer::cold_tab_shell(&cold);
    let cold_tabs = std::collections::HashMap::from([(0, cold)]);
    let capture = || {
        SessionPersistRequest::capture_with_cold_tabs(
            std::slice::from_ref(&shell),
            &cold_tabs,
            0,
            SessionActiveSurface::Workspace,
            16.0,
            false,
        )
    };
    let obstacle = root.path().join("session.db-journal");
    std::fs::create_dir(&obstacle).unwrap();
    assert!(store.start_new_session(capture()).is_err());
    std::fs::remove_dir(&obstacle).unwrap();
    let backup = store.start_new_session(capture()).unwrap();
    assert_eq!(
        store.load().unwrap().unwrap().tabs[0]
            .active_buffer()
            .text(),
        text
    );
    std::fs::remove_dir_all(backup).unwrap();
}
