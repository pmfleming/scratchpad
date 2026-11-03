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
    store.worker().unwrap().call(move |db| block_on(async {
        db.conn.execute("UPDATE blobs SET body=x'00' WHERE digest IN (SELECT digest FROM parts WHERE temp_id=?)", [id]).await.map_err(database::error)?;
        Ok(())
    })).unwrap();
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
                assert_eq!(db.integer("SELECT count(*) FROM snapshots").await?, 2);
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
