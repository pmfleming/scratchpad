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
        db.conn
            .execute("PRAGMA user_version=999", ())
            .await
            .unwrap();
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
