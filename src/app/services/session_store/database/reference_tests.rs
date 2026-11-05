//! Production captures, including absence of eager body reads during restore.
use super::{
    Database, ReadScope,
    tests::{database, insert, tab},
};
use crate::app::domain::{CharCursor, CursorRange, WorkspaceTab};
use crate::app::services::session_store::SessionPersistRequest;
use futures_lite::future::block_on;
use std::sync::Arc;

fn capture(tabs: &[WorkspaceTab]) -> SessionPersistRequest {
    SessionPersistRequest::capture(tabs, 0, 14.0, false)
}

#[test]
fn forget_releases_whole_chunks_without_rewriting_logical_offsets_or_old_clones() {
    block_on(async {
        let (_root, mut db) = database().await;
        let text = "A".repeat(256 * 1024) + &"B".repeat(256 * 1024) + &"λ".repeat(128 * 1024);
        let mut tabs = vec![tab(&text)];
        db.persist(capture(&tabs), 1).await.unwrap();
        let old = tabs[0].clone();
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .replace_char_ranges_with_undo(
                &[(0..512 * 1024, String::new())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .clear_operation_history();
        db.persist(capture(&tabs), 2).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM document_text")
                .await
                .unwrap(),
            1
        );
        assert_eq!(old.active_buffer().text(), text);
        drop(old);
        for _ in 0..4 {
            db.collect_chunk_batch().await.unwrap();
        }
        assert_eq!(db.integer("SELECT count(*) FROM chunks").await.unwrap(), 1);
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
        assert_eq!(document.try_extract_text().unwrap(), "λ".repeat(128 * 1024));
        document.undo_last_operation().unwrap();
        assert_eq!(document.try_extract_text().unwrap(), "");
        document.redo_last_operation().unwrap();
        assert_eq!(document.try_extract_text().unwrap(), "λ".repeat(128 * 1024));
    });
}

#[test]
fn large_entries_use_packed_spans_and_keep_exact_replay_after_restore() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab(&"abc".repeat(300))];
        let changes = (0..300)
            .rev()
            .map(|i| (i * 3..i * 3 + 1, "λ🦀".into()))
            .collect::<Vec<_>>();
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .replace_char_ranges_with_undo(&changes, CursorRange::default(), CursorRange::default())
            .unwrap();
        db.persist(capture(&tabs), 1).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM history WHERE packed_spans IS NOT NULL")
                .await
                .unwrap(),
            1
        );
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
        assert_eq!(document.try_extract_text().unwrap(), "λ🦀bc".repeat(300));
        document.undo_last_operation().unwrap();
        assert_eq!(document.try_extract_text().unwrap(), "abc".repeat(300));
    });
}

#[test]
fn reference_restore_is_lazy_and_preserves_unicode_branches_and_export() {
    block_on(async {
        let (root, mut db) = database().await;
        let text = "λ🦀\r\noriginal\0".repeat(200_000);
        let mut tabs = vec![tab(&text)];
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .bind_store(db.chunk_store().unwrap())
            .unwrap();
        insert(&mut tabs[0], 1, "first");
        let branch = tabs[0].active_buffer().document().history_now();
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .undo_last_operation()
            .unwrap();
        insert(&mut tabs[0], 2, "second");
        let expected = tabs[0].active_buffer().text();
        db.persist(capture(&tabs), 1).await.unwrap();
        assert_eq!(
            db.integer(
                "SELECT count(*) FROM sqlite_master WHERE name IN ('snapshots','parts','blobs')"
            )
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM pragma_table_info('history') WHERE name='entry'")
                .await
                .unwrap(),
            0
        );
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        db.chunk_store().unwrap().set_limits(0, 0);
        let stored = db.load(ReadScope::All).await.unwrap().unwrap();
        let mut document = stored
            .manifest
            .tabs
            .into_iter()
            .next()
            .unwrap()
            .buffers
            .remove(0)
            .history_document
            .unwrap();
        assert_eq!(db.chunk_store().unwrap().epoch().read_count(), 0);
        let snapshot = document.snapshot();
        assert_eq!(db.chunk_store().unwrap().epoch().read_count(), 0);
        assert_eq!(snapshot.len_bytes(), expected.len());
        assert_eq!(document.try_extract_text().unwrap(), expected);
        let exported = document.try_exported_history().unwrap();
        assert!(exported.iter().all(|e| e.payloads_valid()));
        document.navigate_history(branch).unwrap();
        let mut first = text.clone();
        first.insert_str("λ".len(), "first");
        assert_eq!(document.try_extract_text().unwrap(), first);
    });
}

#[test]
fn typing_writes_tail_suffixes_and_navigation_changes_no_history_rows() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("")];
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .bind_store(db.chunk_store().unwrap())
            .unwrap();
        for index in 0..64 {
            insert(&mut tabs[0], index, "λ");
            db.persist(capture(&tabs), index as u64 + 1).await.unwrap();
        }
        assert_eq!(
            db.integer("SELECT sum(length(body)) FROM document_tail")
                .await
                .unwrap(),
            128
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM document_tail")
                .await
                .unwrap(),
            64
        );
        db.conn.execute_batch("CREATE TABLE changes(kind TEXT) STRICT; CREATE TRIGGER update_entry AFTER UPDATE ON history BEGIN INSERT INTO changes VALUES('entry'); END; CREATE TRIGGER update_position AFTER UPDATE ON document_history BEGIN INSERT INTO changes VALUES('position'); END;").await.unwrap();
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .undo_last_operation()
            .unwrap();
        db.persist(capture(&tabs), 65).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM changes WHERE kind='entry'")
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM changes WHERE kind='position'")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            db.integer("SELECT sum(length(body)) FROM document_tail")
                .await
                .unwrap(),
            128
        );
    });
}

#[test]
fn divergent_tail_capture_replaces_segments_without_changing_old_snapshot() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("base")];
        insert(&mut tabs[0], 4, "left λ");
        let old = tabs[0].clone();
        db.persist(capture(&tabs), 1).await.unwrap();
        insert(&mut tabs[0], 10, "right 🦀");
        db.persist(capture(&tabs), 2).await.unwrap();
        db.persist(capture(std::slice::from_ref(&old)), 3)
            .await
            .unwrap();
        assert_eq!(
            db.read_all_contents().await.unwrap()[&old.active_buffer().temp_id],
            "baseleft λ"
        );
        assert_eq!(tabs[0].active_buffer().text(), "baseleft λright 🦀");
    });
}

#[test]
fn collection_keeps_live_only_handles_and_durable_history_roots() {
    block_on(async {
        let (_root, mut db) = database().await;
        let store = db.chunk_store().unwrap();
        let chunk = store.seal(Arc::new("live-only λ".repeat(1024)), 1);
        db.drain_chunk_batch().await.unwrap();
        let row = chunk.reference().unwrap().id;
        for _ in 0..3 {
            db.collect_chunk_batch().await.unwrap();
        }
        assert_eq!(db.integer("SELECT count(*) FROM chunks").await.unwrap(), 1);
        assert_eq!(
            chunk.load(true, false).unwrap().as_str(),
            "live-only λ".repeat(1024)
        );
        drop(chunk);
        for _ in 0..3 {
            db.collect_chunk_batch().await.unwrap();
        }
        assert_eq!(
            db.integer("SELECT count(*) FROM chunks").await.unwrap(),
            0,
            "row {row}"
        );
        let mut tabs = vec![tab(&"history only λ".repeat(30_000))];
        let length = tabs[0].active_buffer().document().piece_tree().len_chars();
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .replace_char_ranges_with_undo(
                &[(0..length, String::new())],
                CursorRange::one(CharCursor::new(0)),
                CursorRange::one(CharCursor::new(0)),
            )
            .unwrap();
        db.persist(capture(&tabs), 1).await.unwrap();
        drop(tabs);
        for _ in 0..3 {
            db.collect_chunk_batch().await.unwrap();
        }
        let stored = db.load(ReadScope::All).await.unwrap().unwrap();
        let mut document = stored
            .manifest
            .tabs
            .into_iter()
            .next()
            .unwrap()
            .buffers
            .remove(0)
            .history_document
            .unwrap();
        document.undo_last_operation().unwrap();
        assert_eq!(
            document.try_extract_text().unwrap(),
            "history only λ".repeat(30_000)
        );
    });
}
