use super::super::*;
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::BufferState;
use crate::app::services::{session_manager, settings_store::SettingsStore};
use crate::app::startup::StartupOptions;
use eframe::{App, egui};

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
fn unsupported_schema_allows_exit_without_replacing_the_database() {
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
                    .execute("PRAGMA user_version=999", ())
                    .await
                    .map_err(database::error)?;
                db.checkpoint().await
            })
        })
        .unwrap();
    drop(store);
    let before = std::fs::read(session_root.join("session.db")).unwrap();
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(session_root.clone()),
        SettingsStore::new(root.path().join("settings")),
        // Skip restore so exit actually attempts (and reports) the refused save.
        StartupOptions {
            restore_session: false,
            restore_session_explicit: true,
            ..Default::default()
        },
    );
    let ctx = egui::Context::default();
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let output = ctx.run_logic(&input, |ctx| {
        app.logic(ctx, &mut eframe::Frame::_new_kittest());
    });
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT].contains(&egui::ViewportCommand::Close)
    );
    assert!(app.state.window.close_in_progress);
    assert!(!app.state.persistence.persist_session_on_drop);
    drop(app);
    assert_eq!(
        std::fs::read(session_root.join("session.db")).unwrap(),
        before
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
    let _ = ctx.run_logic(&input, |ctx| {
        app.logic(ctx, &mut eframe::Frame::_new_kittest());
    });
    assert!(!app.state.window.close_in_progress);
    assert!(!store.writes_blocked());
    let count = app.state.status.history.len();
    for _ in 0..3 {
        let _ = ctx.run_logic(&input, |ctx| {
            app.logic(ctx, &mut eframe::Frame::_new_kittest());
        });
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
    let mut close = input;
    close
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let _ = ctx.run_logic(&close, |ctx| {
        app.logic(ctx, &mut eframe::Frame::_new_kittest());
    });
    assert!(app.state.window.close_in_progress);
}
