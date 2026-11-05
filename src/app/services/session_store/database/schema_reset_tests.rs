use super::{
    Database,
    tests::{database, request, tab},
};
use crate::app::services::session_store::SessionStore;
use futures_lite::future::block_on;

#[test]
fn fresh_database_has_no_reset_warning() {
    block_on(async {
        let (_root, db) = database().await;
        assert!(db.startup_notices.is_empty());
    });
}

#[test]
fn foreign_database_is_never_reset() {
    block_on(async {
        let (root, db) = database().await;
        db.conn
            .execute_batch("PRAGMA application_id=123; PRAGMA user_version=9;")
            .await
            .unwrap();
        db.checkpoint().await.unwrap();
        drop(db);
        let path = root.path().join("session.db");
        let before = std::fs::read(&path).unwrap();
        assert!(Database::open(root.path()).await.is_err());
        assert_eq!(std::fs::read(path).unwrap(), before);
    });
}

#[test]
fn tree_rows_and_positions_round_trip_transactionally() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tab = tab("");
        for text in ["a", "b", "c"] {
            let at = tab.active_buffer().document().piece_tree().len_chars();
            super::tests::insert(&mut tab, at, text);
        }
        let document = tab.active_buffer_mut().document_mut();
        document.navigate_history(1).unwrap();
        document
            .replace_char_ranges_with_undo(
                &[(1..1, "branch".into())],
                crate::app::domain::CursorRange::default(),
                crate::app::domain::CursorRange::default(),
            )
            .unwrap();
        document.navigate_history(1).unwrap();
        let request = request(&[tab]);
        let entries = &request.tabs[0].session_tab.buffers[0].text_history;
        assert_eq!(entries.len(), 4);
        let expected = entries.clone();
        let id = request.tabs[0].session_tab.buffers[0].temp_id.clone();
        db.persist(request, 1).await.unwrap();
        assert_eq!(db.read_history(&id).await.unwrap(), expected);
        assert_eq!(
            db.integer("SELECT count(*) FROM document_history WHERE now_id=1 AND tip_id=4")
                .await
                .unwrap(),
            1
        );
        db.checkpoint().await.unwrap();
        drop(db);
        let db = Database::open(root.path()).await.unwrap();
        assert_eq!(db.read_history(&id).await.unwrap(), expected);
        let mut document = crate::app::domain::TextDocument::new("a".into());
        document.restore_exported_history(db.read_history(&id).await.unwrap());
        document.redo_last_operation().unwrap();
        assert_eq!(document.extract_text(), "abranch");
        document.navigate_history(3).unwrap();
        assert_eq!(document.extract_text(), "abc");
        assert_eq!(db.integer("PRAGMA user_version").await.unwrap(), 12);
    });
}

#[test]
fn malformed_database_is_not_a_schema_reset() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("session.db");
    let contents = b"not a database";
    std::fs::write(&path, contents).unwrap();
    assert!(block_on(Database::open(root.path())).is_err());
    assert_eq!(std::fs::read(path).unwrap(), contents);
}

#[test]
fn recovery_export_does_not_reset_an_unsupported_source() {
    block_on(async {
        let (root, mut db) = database().await;
        db.persist(request(&[tab("keep for export")]), 1)
            .await
            .unwrap();
        db.conn
            .execute("PRAGMA user_version=999", ())
            .await
            .unwrap();
        db.checkpoint().await.unwrap();
        drop(db);
        let path = root.path().join("session.db");
        let before = std::fs::read(&path).unwrap();
        let store = SessionStore::new(root.path().to_owned());
        assert!(store.export_recovery(&root.path().join("export")).is_err());
        drop(store);
        assert_eq!(std::fs::read(path).unwrap(), before);
    });
}

#[test]
fn lazy_queries_do_not_silently_reset_an_incompatible_session() {
    block_on(async {
        let (root, db) = database().await;
        db.conn.execute("PRAGMA user_version=9", ()).await.unwrap();
        drop(db);
        let store = SessionStore::new(root.path().to_owned());
        let clone = store.clone();
        assert!(store.take_notices().is_empty());
        assert!(clone.closed_tabs().is_err());
        assert!(clone.writes_blocked());
        assert!(store.take_notices().is_empty());
        assert!(clone.take_notices().is_empty());
    });
}

#[test]
fn inconsistent_cursor_and_corrupt_branch_payload_are_rejected() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab("immutable")];
        db.persist(request(&tabs), 1).await.unwrap();
        let mut malformed = request(&tabs);
        malformed.tabs[0].session_tab.buffers[0].text_history[0].fingerprint ^= 1;
        assert!(db.persist(malformed, 2).await.is_err());
        let id = &tabs[0].active_buffer().temp_id;
        assert_eq!(db.read_history(id).await.unwrap().len(), 1);
        assert!(
            db.conn
                .execute("UPDATE document_history SET now_id=999", ())
                .await
                .is_err()
        );
        db.conn
            .execute("PRAGMA foreign_keys=OFF", ())
            .await
            .unwrap();
        db.conn
            .execute("UPDATE document_history SET now_id=999", ())
            .await
            .unwrap();
        assert!(db.read_history(id).await.is_err());
        assert!(
            db.hydrate(request(&tabs).tabs.remove(0).session_tab)
                .await
                .is_err()
        );
    });
}

#[test]
fn closed_file_branches_now_and_edit_recency_survive_restart_and_forget() {
    use crate::app::services::session_store::{CapturedClosedTab, SessionPersistRequest};
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path().to_owned());
    let mut tab = tab("");
    let doc = tab.active_buffer_mut().document_mut();
    for text in ["a", "ab", "abc"] {
        doc.replace_exact_with_history(text);
    }
    doc.navigate_history(1).unwrap();
    doc.replace_exact_with_history("alternate");
    let last_edit = doc.last_edit();
    doc.navigate_history(1).unwrap();
    let archive = CapturedClosedTab::capture(&tab, None);
    let token = archive.token.clone();
    let mut save = SessionPersistRequest::capture(&[], 0, 16.0, false);
    save.closed_tabs.push(archive);
    store.persist_request(save).unwrap();
    drop(store);
    let store = SessionStore::new(root.path().to_owned());
    let recent = store.recent_history_files().unwrap();
    assert_eq!((recent[0].edited_at, recent[0].edit_seq), last_edit);
    let mut reopened = store.restore_closed_tab(&token).unwrap();
    let doc = reopened.active_buffer_mut().document_mut();
    assert_eq!((doc.history_now(), doc.history_tip()), (1, 4));
    doc.redo_last_operation().unwrap();
    assert_eq!(doc.extract_text(), "alternate");
    doc.navigate_history(3).unwrap();
    assert_eq!(doc.extract_text(), "abc");
    assert_eq!(doc.last_edit(), last_edit);
    doc.clear_operation_history();
    store.persist(&[reopened], 0, 16.0, false).unwrap();
    drop(store);
    let restored = SessionStore::new(root.path().to_owned())
        .load()
        .unwrap()
        .unwrap();
    let doc = restored.tabs[0].active_buffer().document();
    assert_eq!(doc.extract_text(), "abc");
    assert_eq!(doc.history_entries().len(), 1);
}

#[test]
#[ignore = "manual edit-tree database profile"]
fn profile_edit_tree_database_growth_and_restart() {
    use std::time::Instant;
    for count in [1_000, 10_000] {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::new(root.path().to_owned());
        let mut tab = tab("");
        for index in 0..count {
            tab.active_buffer_mut()
                .document_mut()
                .replace_exact_with_history(if index % 2 == 0 { "α" } else { "β" });
        }
        let start = Instant::now();
        store.persist(&[tab], 0, 16.0, false).unwrap();
        let save = start.elapsed();
        drop(store);
        let start = Instant::now();
        let restored = SessionStore::new(root.path().to_owned())
            .load()
            .unwrap()
            .unwrap();
        let restart = start.elapsed();
        assert_eq!(restored.tabs[0].active_buffer().text(), "β");
        eprintln!(
            "database nodes={count} save_ms={} restart_ms={} database_bytes={}",
            save.as_millis(),
            restart.as_millis(),
            std::fs::metadata(root.path().join("session.db"))
                .unwrap()
                .len()
        );
    }
}

#[test]
fn recent_closed_files_report_history_even_at_root_or_without_edit_timestamps() {
    use crate::app::services::session_store::{CapturedClosedTab, SessionPersistRequest};
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path().to_owned());
    let mut initial = tab("initial contents");
    initial.active_buffer_mut().name = "initial".into();
    let mut undone = tab("undo to root");
    undone.active_buffer_mut().name = "undone".into();
    undone
        .active_buffer_mut()
        .undo_last_text_operation()
        .unwrap();
    let mut empty = tab("");
    empty.active_buffer_mut().name = "empty".into();
    let mut save = SessionPersistRequest::capture(&[], 0, 16.0, false);
    for tab in [initial, undone, empty] {
        save.closed_tabs
            .push(CapturedClosedTab::capture(&tab, None));
    }
    store.persist_request(save).unwrap();
    let files = store.recent_history_files().unwrap();
    assert_eq!(files.len(), 3);
    assert!(
        files
            .iter()
            .find(|file| file.label == "initial")
            .unwrap()
            .has_history
    );
    assert!(
        files
            .iter()
            .find(|file| file.label == "undone")
            .unwrap()
            .has_history
    );
    assert!(
        !files
            .iter()
            .find(|file| file.label == "empty")
            .unwrap()
            .has_history
    );
}
