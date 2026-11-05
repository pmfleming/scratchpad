use super::{
    ReadScope,
    tests::{database, insert, tab},
};
use crate::app::services::session_store::SessionPersistRequest;
use futures_lite::future::block_on;

#[test]
fn reference_ranges_survive_multiple_unicode_edits_eof_changes_and_replay() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab(&"original λ🦀\r\n".repeat(25_000))];
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .bind_store(db.chunk_store().unwrap())
            .unwrap();
        for round in 0..32 {
            let length = tabs[0].active_buffer().document().piece_tree().len_chars();
            let at = match round % 3 {
                0 => 0,
                1 => length / 2,
                _ => length,
            };
            insert(&mut tabs[0], at, "🦀λ\0\r\n");
            db.persist(
                SessionPersistRequest::capture(&tabs, 0, 14.0, false),
                round + 1,
            )
            .await
            .unwrap();
            let stored = db.load(ReadScope::All).await.unwrap().unwrap();
            assert_eq!(
                stored.text_contents()[&tabs[0].active_buffer().temp_id],
                tabs[0].active_buffer().text()
            );
        }
        let mut document = db
            .load(ReadScope::All)
            .await
            .unwrap()
            .unwrap()
            .manifest
            .tabs
            .remove(0)
            .buffers
            .remove(0)
            .history_document
            .unwrap();
        while tabs[0]
            .active_buffer_mut()
            .document_mut()
            .undo_last_operation()
            .is_some()
        {
            document.undo_last_operation().unwrap();
            assert_eq!(
                document.try_extract_text().unwrap(),
                tabs[0].active_buffer().text()
            );
        }
    });
}

#[test]
fn failed_reference_manifest_is_retryable_without_rescanning_immutable_text() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab(&"original λ🦀\r\n".repeat(100_000))];
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .bind_store(db.chunk_store().unwrap())
            .unwrap();
        db.persist(SessionPersistRequest::capture(&tabs, 0, 14.0, false), 1)
            .await
            .unwrap();
        insert(&mut tabs[0], 100, "local edit");
        db.conn.execute_batch("CREATE TRIGGER fail_save BEFORE INSERT ON document_pieces BEGIN SELECT RAISE(ABORT,'injected'); END;").await.unwrap();
        assert!(
            db.persist(SessionPersistRequest::capture(&tabs, 0, 14.0, false), 2)
                .await
                .is_err()
        );
        db.conn.execute("DROP TRIGGER fail_save", ()).await.unwrap();
        let reads = db.chunk_store().unwrap().epoch().read_count();
        let retry = db
            .persist(SessionPersistRequest::capture(&tabs, 0, 14.0, false), 3)
            .await
            .unwrap();
        assert_eq!(retry.snapshot_bytes_read, 0);
        assert_eq!(db.chunk_store().unwrap().epoch().read_count(), reads);
        assert_eq!(
            db.read_all_contents().await.unwrap()[&tabs[0].active_buffer().temp_id],
            tabs[0].active_buffer().text()
        );
    });
}
