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
        let mut request = request(&[tab]);
        let entries = &mut request.tabs[0].session_tab.buffers[0].text_history;
        assert_eq!(entries.len(), 3);
        entries[2].parent_id = entries[0].id;
        for entry in entries.iter_mut() {
            entry.current = false;
        }
        entries[0].current = true;
        let expected = entries.clone();
        let id = request.tabs[0].session_tab.buffers[0].temp_id.clone();
        db.persist(request, 1).await.unwrap();
        assert_eq!(db.read_history(&id).await.unwrap(), expected);
        assert_eq!(
            db.integer("SELECT count(*) FROM document_history WHERE now_id=1 AND tip_id=3")
                .await
                .unwrap(),
            1
        );
        db.checkpoint().await.unwrap();
        drop(db);
        let db = Database::open(root.path()).await.unwrap();
        assert_eq!(db.read_history(&id).await.unwrap(), expected);
        assert_eq!(db.integer("PRAGMA user_version").await.unwrap(), 10);
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
fn lazy_initialization_notices_are_shared_and_consumed_once() {
    block_on(async {
        let (root, db) = database().await;
        db.conn.execute("PRAGMA user_version=9", ()).await.unwrap();
        drop(db);
        let store = SessionStore::new(root.path().to_owned());
        let clone = store.clone();
        assert!(store.take_notices().is_empty());
        assert!(clone.closed_tabs().unwrap().is_empty());
        assert!(!clone.writes_blocked());
        assert_eq!(store.take_notices().len(), 1);
        assert!(clone.take_notices().is_empty());
    });
}
