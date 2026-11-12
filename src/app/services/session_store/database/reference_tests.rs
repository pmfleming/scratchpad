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
fn later_piece_deletion_and_undo_update_total_bytes_without_rewriting_first_part() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab(&"a".repeat(800))];
        for index in 0..800 {
            insert(&mut tabs[0], index * 2, "x");
        }
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .seal_history_group();
        let original = "xa".repeat(800);
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(capture(&tabs), 1).await.unwrap();
        assert!(
            db.integer("SELECT count(*) FROM document_pieces")
                .await
                .unwrap()
                > 1
        );
        // Updating the independent byte count must not rewrite unchanged body data.
        db.conn.execute_batch("CREATE TRIGGER unchanged_first_part BEFORE UPDATE OF body,digest ON document_pieces WHEN old.part=0 BEGIN SELECT RAISE(ABORT,'first piece part rewritten'); END;").await.unwrap();
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .replace_char_ranges_with_undo(
                &[(1412..1600, String::new())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
        db.persist(capture(&tabs), 2).await.unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        let restored = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(restored.text_contents()[&id], original[..1412]);
        let mut document = restored
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
        assert_eq!(document.try_extract_text().unwrap(), original);
        // Undo grows the document using existing storage, leaving the first part intact too.
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .undo_last_operation()
            .unwrap();
        db.persist(capture(&tabs), 1).await.unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert_eq!(
            db.load(ReadScope::All)
                .await
                .unwrap()
                .unwrap()
                .text_contents()[&id],
            original
        );
    });
}

#[test]
fn batched_layouts_rollback_then_preserve_splits_and_reopened_reordering() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab("committed")];
        db.persist(capture(&tabs), 1).await.unwrap();
        for index in 0..65 {
            let mut next = tab(&format!("{index}: λ🦀\n"));
            if index % 3 == 0 {
                next.split_active_view(crate::app::domain::SplitAxis::Horizontal);
            }
            tabs.push(next);
        }
        db.conn.execute_batch("CREATE TRIGGER fail_split BEFORE INSERT ON layout_views WHEN new.ordinal=1 BEGIN SELECT RAISE(ABORT,'injected split failure'); END;").await.unwrap();
        assert!(db.persist(capture(&tabs), 2).await.is_err());
        let committed = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(committed.manifest.tabs.len(), 1);
        assert_eq!(
            committed.text_contents().into_values().next().unwrap(),
            "committed"
        );
        db.conn
            .execute("DROP TRIGGER fail_split", ())
            .await
            .unwrap();
        db.persist(capture(&tabs), 3).await.unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        tabs.rotate_left(17);
        tabs.push(tab("new after reopen"));
        db.persist(capture(&tabs), 1).await.unwrap();
        let restored = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(restored.manifest.tabs.len(), tabs.len());
        for (actual, expected) in restored.manifest.tabs.iter().zip(capture(&tabs).tabs) {
            assert!(
                super::layouts::LayoutRow::capture(actual).unwrap()
                    == super::layouts::LayoutRow::capture(&expected.session_tab).unwrap()
            );
        }
    });
}

#[test]
fn batched_small_documents_survive_failed_manifest_retry_with_exact_history() {
    block_on(async {
        let (_root, mut db) = database().await;
        let texts: Vec<_> = (0..130)
            .map(|i| format!("{i}: café 東京\n").repeat(300))
            .collect();
        let tabs: Vec<_> = texts.iter().map(|text| tab(text)).collect();
        db.conn.execute_batch("CREATE TRIGGER fail_batch BEFORE INSERT ON document_pieces BEGIN SELECT RAISE(ABORT,'injected'); END;").await.unwrap();
        assert!(db.persist(capture(&tabs), 1).await.is_err());
        db.conn
            .execute_batch("DROP TRIGGER fail_batch;")
            .await
            .unwrap();
        db.persist(capture(&tabs), 2).await.unwrap();
        let restored = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(restored.manifest.tabs.len(), texts.len());
        for (mut tab, expected) in restored.manifest.tabs.into_iter().zip(texts) {
            let mut document = tab.buffers.remove(0).history_document.unwrap();
            assert_eq!(document.try_extract_text().unwrap(), expected);
            document.undo_last_operation().unwrap();
            assert_eq!(document.try_extract_text().unwrap(), "");
            document.redo_last_operation().unwrap();
            assert_eq!(document.try_extract_text().unwrap(), expected);
        }
    });
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
fn completed_collection_sweep_does_not_query_again_until_its_idle_delay_expires() {
    block_on(async {
        let (_root, mut db) = database().await;
        let now = std::time::Instant::now();
        db.collect_idle_chunk_batch(now).await.unwrap();
        // Any attempted collection query now fails, so successful calls below
        // demonstrate that the worker's idle path does not touch the database.
        db.conn.execute("DROP TABLE chunks", ()).await.unwrap();
        db.statements.lock().unwrap().clear();
        for tick in 1..50 {
            db.collect_idle_chunk_batch(now + std::time::Duration::from_millis(tick * 100))
                .await
                .unwrap();
        }
        assert!(
            db.collect_idle_chunk_batch(now + std::time::Duration::from_secs(5))
                .await
                .is_err()
        );
    });
}

#[test]
fn idle_collection_rechecks_dropped_live_handles_without_another_write() {
    block_on(async {
        let (_root, mut db) = database().await;
        let store = db.chunk_store().unwrap();
        let chunk = store.seal(Arc::new("live-only chunk".into()), 1);
        db.drain_chunk_batch().await.unwrap();
        let now = std::time::Instant::now();
        db.collect_idle_chunk_batch(now).await.unwrap();
        db.collect_idle_chunk_batch(now).await.unwrap();
        assert_eq!(db.collection_cursor, 0);
        drop(chunk);
        db.collect_idle_chunk_batch(now + std::time::Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM chunks").await.unwrap(), 1);
        db.collect_idle_chunk_batch(now + std::time::Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM chunks").await.unwrap(), 0);
    });
}

#[test]
fn successful_writes_resume_collection_during_the_idle_delay() {
    block_on(async {
        let (_root, mut db) = database().await;
        db.collect_idle_chunk_batch(std::time::Instant::now())
            .await
            .unwrap();
        let store = db.chunk_store().unwrap();
        let chunk = store.seal(Arc::new("newly abandoned chunk".into()), 1);
        db.drain_chunk_batch().await.unwrap();
        drop(chunk);
        db.collect_idle_chunk_batch(std::time::Instant::now())
            .await
            .unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM chunks").await.unwrap(), 0);
    });
}

#[test]
fn idle_collection_keeps_scanning_past_live_batches_before_backing_off() {
    block_on(async {
        let (_root, mut db) = database().await;
        let store = db.chunk_store().unwrap();
        let mut chunks: Vec<_> = (0..9)
            .map(|n| store.seal(Arc::new(format!("collection chunk {n}")), 1))
            .collect();
        while store.pending_bytes() > 0 {
            db.drain_chunk_batch().await.unwrap();
        }
        chunks.truncate(4);
        // One wide scan skips the four live rows and deletes the next four; the
        // short follow-up scan deletes the last row and completes the sweep.
        for expected in [5, 4, 4] {
            db.collect_idle_chunk_batch(std::time::Instant::now())
                .await
                .unwrap();
            assert_eq!(
                db.integer("SELECT count(*) FROM chunks").await.unwrap(),
                expected
            );
        }
        assert_eq!(db.collection_cursor, 0);
        for (n, chunk) in chunks.iter().enumerate() {
            assert_eq!(
                chunk.load(false, false).unwrap().as_str(),
                format!("collection chunk {n}")
            );
        }
    });
}

#[test]
fn many_live_only_rows_cost_few_scans_before_the_idle_delay() {
    block_on(async {
        let (_root, mut db) = database().await;
        let store = db.chunk_store().unwrap();
        let live: Vec<_> = (0..300)
            .map(|n| store.seal(Arc::new(format!("live-only row {n}")), 1))
            .collect();
        let abandoned = store.seal(Arc::new("abandoned after live rows".into()), 1);
        while store.pending_bytes() > 0 {
            db.drain_chunk_batch().await.unwrap();
        }
        drop(abandoned);
        let now = std::time::Instant::now();
        // 301 unreferenced rows need three 128-row scans, not 76 four-row ones.
        for _ in 0..3 {
            db.collect_idle_chunk_batch(now).await.unwrap();
        }
        assert_eq!(
            db.integer("SELECT count(*) FROM chunks").await.unwrap(),
            300
        );
        assert_eq!(db.collection_cursor, 0);
        db.conn
            .execute("DROP TABLE chunk_bodies", ())
            .await
            .unwrap();
        db.conn.execute("DROP TABLE chunks", ()).await.unwrap();
        db.statements.lock().unwrap().clear();
        // The completed sweep is paused, so no further query runs.
        db.collect_idle_chunk_batch(now + std::time::Duration::from_secs(4))
            .await
            .unwrap();
        drop(live);
    });
}

#[test]
fn collection_batches_never_skip_rows_past_the_delete_limit() {
    block_on(async {
        let (_root, mut db) = database().await;
        let store = db.chunk_store().unwrap();
        let chunks: Vec<_> = (0..10)
            .map(|n| store.seal(Arc::new(format!("garbage row {n}")), 1))
            .collect();
        while store.pending_bytes() > 0 {
            db.drain_chunk_batch().await.unwrap();
        }
        drop(chunks);
        // Ten deletable rows: 4 + 4 + 2, with the last batch completing the sweep.
        assert!(!db.collect_chunk_batch().await.unwrap());
        assert_eq!(db.integer("SELECT count(*) FROM chunks").await.unwrap(), 6);
        assert!(!db.collect_chunk_batch().await.unwrap());
        assert_eq!(db.integer("SELECT count(*) FROM chunks").await.unwrap(), 2);
        assert!(db.collect_chunk_batch().await.unwrap());
        assert_eq!(db.integer("SELECT count(*) FROM chunks").await.unwrap(), 0);
        assert_eq!(db.collection_cursor, 0);
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

#[test]
fn batched_history_restore_attributes_missing_positions_and_bad_spans() {
    block_on(async {
        for corruption in [
            "DELETE FROM document_history WHERE temp_id=?",
            "UPDATE history SET packed_spans=x'00' WHERE temp_id=?",
        ] {
            let (_root, mut db) = database().await;
            let tabs: Vec<_> = (0..66)
                .map(|index| tab(&format!("{index}: café 東京")))
                .collect();
            db.persist(capture(&tabs), 1).await.unwrap();
            let bad = tabs[65].active_buffer().temp_id.clone();
            db.conn.execute(corruption, [bad.as_str()]).await.unwrap();
            let error = db.load(ReadScope::All).await.err().unwrap();
            assert!(error.to_string().contains("quarantined"));
            assert!(db.quarantined.contains_key(&bad));
            assert_eq!(db.quarantined.len(), 1);
            assert!(db.blocked.is_none());
        }
    });
}
