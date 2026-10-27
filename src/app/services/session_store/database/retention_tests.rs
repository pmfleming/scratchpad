use super::{
    closed::CLOSED_TAB_LIMIT,
    tests::{database, insert, request, tab},
};
use crate::app::{
    app_state::{ScratchpadApp, workspace_controller},
    services::{
        session_store::{CapturedClosedTab, SessionStore},
        settings_store::SettingsStore,
    },
    startup::StartupOptions,
};
use futures_lite::future::block_on;

#[test]
fn closing_last_tab_retains_latest_unsaved_text_and_undo_across_restart() {
    let root = tempfile::tempdir().unwrap();
    let session_root = root.path().join("session");
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(session_root.clone()),
        SettingsStore::new(root.path().join("settings")),
        StartupOptions::default(),
    );
    let mut original = tab("before");
    insert(&mut original, 6, " after");
    app.tab_manager.set_tabs(vec![original], 0);
    workspace_controller::perform_close_tab(&mut app, 0);
    assert!(app.tab_manager.tabs.is_empty());
    assert!(app.state.persistence.pending_closed_tabs.is_empty());
    drop(app);
    let store = SessionStore::new(session_root);
    assert!(store.load().unwrap().is_none());
    let closed = store.closed_tabs().unwrap();
    assert_eq!(closed.len(), 1);
    let mut recovered = store.restore_closed_tab(&closed[0].token).unwrap();
    assert_eq!(recovered.active_buffer().text(), "before after");
    recovered
        .buffers
        .buffer
        .document_mut()
        .undo_last_operation()
        .unwrap();
    assert_eq!(recovered.active_buffer().text(), "before");
    store.persist(&[recovered], 0, 16.0, false).unwrap();
    assert!(store.closed_tabs().unwrap().is_empty());
}

#[test]
fn closed_retention_is_bounded_retry_safe_and_shares_blob_bodies() {
    block_on(async {
        let (_root, mut db) = database().await;
        let closed = CapturedClosedTab::capture(&tab("shared"), None);
        let mut first = request(&[]);
        first.closed_tabs.push(closed.clone());
        db.persist(first, 1).await.unwrap();
        let mut retry = request(&[]);
        retry.closed_tabs.push(closed);
        db.persist(retry, 2).await.unwrap();
        assert_eq!(db.closed_rows().await.unwrap().len(), 1);
        for generation in 3..(CLOSED_TAB_LIMIT as u64 + 6) {
            let mut next = request(&[]);
            next.closed_tabs
                .push(CapturedClosedTab::capture(&tab("shared"), None));
            db.persist(next, generation).await.unwrap();
        }
        assert_eq!(db.closed_rows().await.unwrap().len(), CLOSED_TAB_LIMIT);
        assert_eq!(
            db.integer("SELECT count(*) FROM snapshots").await.unwrap(),
            CLOSED_TAB_LIMIT as i64
        );
        assert_eq!(db.integer("SELECT count(*) FROM blobs").await.unwrap(), 1);
    });
}

#[test]
fn damaged_closed_history_is_quarantined_before_retention_expiry() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut precious = tab("precious");
        insert(&mut precious, 0, "undoable ");
        let closed = CapturedClosedTab::capture(&precious, None);
        let token = closed.token.clone();
        let mut capture = request(&[]);
        capture.closed_tabs.push(closed);
        db.persist(capture, 1).await.unwrap();
        db.conn
            .execute("UPDATE history SET entry=x'00'", ())
            .await
            .unwrap();
        assert!(db.read_closed_tab(&token).await.is_err());
        assert_eq!(db.quarantined.len(), 1);
        for generation in 2..CLOSED_TAB_LIMIT as u64 + 3 {
            let mut capture = request(&[]);
            capture
                .closed_tabs
                .push(CapturedClosedTab::capture(&tab("healthy"), None));
            db.persist(capture, generation).await.unwrap();
        }
        assert!(
            !db.closed_rows()
                .await
                .unwrap()
                .iter()
                .any(|row| row.token == token)
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM snapshots").await.unwrap(),
            CLOSED_TAB_LIMIT as i64 + 1
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM history WHERE entry=x'00'")
                .await
                .unwrap(),
            1
        );
        db.conn
            .execute("UPDATE closed_tabs SET payload=x'00'", ())
            .await
            .unwrap();
        assert!(db.persist(request(&[]), 100).await.is_err());
        assert!(db.blocked.is_some());
    });
}

#[test]
fn failed_archive_transaction_leaves_live_session_and_retries_without_duplicate_archive() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab("keep me")];
        db.persist(request(&tabs), 1).await.unwrap();
        let closed = CapturedClosedTab::capture(&tabs[0], None);
        db.conn.execute_batch("CREATE TRIGGER fail_closed BEFORE INSERT ON closed_tabs BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        let mut close = request(&[]);
        close.closed_tabs.push(closed.clone());
        assert!(db.persist(close, 2).await.is_err());
        assert!(db.closed_rows().await.unwrap().is_empty());
        assert_eq!(db.integer("SELECT count(*) FROM tabs").await.unwrap(), 1);
        db.conn
            .execute("DROP TRIGGER fail_closed", ())
            .await
            .unwrap();
        let mut close = request(&[]);
        close.closed_tabs.push(closed);
        db.persist(close, 3).await.unwrap();
        assert_eq!(db.closed_rows().await.unwrap().len(), 1);
        assert_eq!(db.integer("SELECT count(*) FROM tabs").await.unwrap(), 0);
    });
}
