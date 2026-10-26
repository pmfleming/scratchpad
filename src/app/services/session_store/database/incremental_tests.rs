use super::{
    tests::{request, tab},
    *,
};
use crate::app::domain::{CharCursor, CursorRange, WorkspaceTab};
use futures_lite::future::block_on;
use turso::params;

fn insert(tab: &mut WorkspaceTab, at: usize, text: &str) {
    tab.buffers
        .buffer
        .document_mut()
        .replace_char_ranges_with_undo(
            &[(at..at, text.into())],
            CursorRange::one(CharCursor::new(at)),
            CursorRange::one(CharCursor::new(at + text.chars().count())),
        )
        .unwrap();
}
async fn audit(db: &Database) {
    db.conn.execute_batch("CREATE TABLE writes(kind TEXT NOT NULL, bytes INTEGER NOT NULL) STRICT;
        CREATE TRIGGER history_insert AFTER INSERT ON history BEGIN INSERT INTO writes VALUES('insert', length(new.entry)); END;
        CREATE TRIGGER history_update AFTER UPDATE ON history BEGIN INSERT INTO writes VALUES('update', length(new.entry)); END;
        CREATE TRIGGER history_delete AFTER DELETE ON history BEGIN INSERT INTO writes VALUES('delete', 0); END;
        CREATE TRIGGER blob_insert AFTER INSERT ON blobs BEGIN INSERT INTO writes VALUES('blob', length(new.body)); END;").await.unwrap();
}
async fn reset(db: &Database) {
    db.conn.execute("DELETE FROM writes", ()).await.unwrap();
}
async fn count(db: &Database, kind: &str) -> i64 {
    db.integer(&format!("SELECT count(*) FROM writes WHERE kind='{kind}'"))
        .await
        .unwrap()
}

#[test]
fn history_appends_updates_prunes_and_restarts_without_rewriting_unchanged_entries() {
    block_on(async {
        let root = tempfile::tempdir().unwrap();
        let mut db = Database::open(root.path()).await.unwrap();
        let mut tabs = vec![tab("base")];
        for _ in 0..4 {
            insert(&mut tabs[0], 0, "entry");
        }
        let id = tabs[0].buffers.buffer.temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        audit(&db).await;
        insert(&mut tabs[0], 0, "new");
        db.persist(request(&tabs), 2).await.unwrap();
        assert_eq!(count(&db, "insert").await, 1);
        assert_eq!(count(&db, "update").await, 0);
        let payload = db
            .conn
            .query("SELECT payload FROM tabs", ())
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<Vec<u8>>(0)
            .unwrap();
        assert!(!String::from_utf8(payload).unwrap().contains("text_history"));
        reset(&db).await;
        tabs[0]
            .buffers
            .buffer
            .document_mut()
            .undo_last_operation()
            .unwrap();
        db.persist(request(&tabs), 3).await.unwrap();
        assert_eq!(count(&db, "update").await, 1);
        assert_eq!(count(&db, "insert").await, 0);
        // Coalescing changes global_seq, payload, and selections under the same ID.
        let mut captured = request(&tabs);
        let entries = &mut captured.tabs[0].session_tab.buffers[0].text_history;
        let last = entries.last_mut().unwrap();
        last.global_seq += 100;
        last.next_selection.primary_index += 1;
        last.summary.push_str(" coalesced");
        entries[0].drop_payloads();
        let expected = entries.clone();
        reset(&db).await;
        db.persist(captured, 4).await.unwrap();
        assert_eq!(count(&db, "update").await, 2);
        assert_eq!(db.read_history(&id).await.unwrap(), expected);
        // Drop the prefix, redo suffix, and an interior entry in one capture.
        let mut captured = request(&tabs);
        captured.tabs[0].session_tab.buffers[0].text_history =
            vec![expected[1].clone(), expected[3].clone()];
        db.persist(captured, 5).await.unwrap();
        assert_eq!(count(&db, "delete").await, 3);
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert!(
            crate::app::domain::buffer::next_text_history_global_seq() > expected[3].global_seq
        );
        let mut captured = request(&tabs);
        captured.tabs[0].session_tab.buffers[0].text_history =
            vec![expected[1].clone(), expected[3].clone()];
        reset(&db).await;
        db.persist(captured, 1).await.unwrap();
        assert_eq!(
            count(&db, "insert").await + count(&db, "update").await + count(&db, "delete").await,
            0
        );
        let mut captured = request(&tabs);
        captured.tabs[0].session_tab.buffers[0].text_history.clear();
        db.persist(captured, 2).await.unwrap();
        assert!(db.read_history(&id).await.unwrap().is_empty());
    });
}

#[test]
fn failed_commit_retries_history_and_content_together() {
    block_on(async {
        let root = tempfile::tempdir().unwrap();
        let mut db = Database::open(root.path()).await.unwrap();
        let mut tabs = vec![tab("original")];
        insert(&mut tabs[0], 0, "a");
        db.persist(request(&tabs), 1).await.unwrap();
        let id = tabs[0].buffers.buffer.temp_id.clone();
        let original = db.read_history(&id).await.unwrap();
        db.conn.execute_batch("CREATE TRIGGER fail_commit BEFORE INSERT ON session_state BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        insert(&mut tabs[0], 0, "b");
        assert!(db.persist(request(&tabs), 2).await.is_err());
        assert_eq!(db.read_history(&id).await.unwrap(), original);
        assert_eq!(db.read_all_contents().await.unwrap()[&id], "aoriginal");
        db.conn
            .execute("DROP TRIGGER fail_commit", ())
            .await
            .unwrap();
        db.persist(request(&tabs), 3).await.unwrap();
        assert_eq!(db.read_history(&id).await.unwrap().len(), 2);
        assert_eq!(db.read_all_contents().await.unwrap()[&id], "baoriginal");
    });
}

#[test]
fn cold_history_is_lazy_and_survives_metadata_saves_and_quarantine() {
    block_on(async {
        let root = tempfile::tempdir().unwrap();
        let mut db = Database::open(root.path()).await.unwrap();
        let mut tabs = vec![tab("active"), tab("cold")];
        insert(&mut tabs[1], 0, "history");
        let id = tabs[1].buffers.buffer.temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        let mut shells = db
            .load(ReadScope::Shells)
            .await
            .unwrap()
            .unwrap()
            .manifest
            .tabs;
        assert!(shells[1].buffers[0].text_history.is_empty());
        let mut captured = request(&tabs);
        captured.tabs[1].session_tab = shells.pop().unwrap();
        captured.tabs[1].preserve_existing = true;
        captured.tabs[1].buffer_snapshots.clear();
        captured.font_size += 1.0;
        db.persist(captured, 2).await.unwrap();
        let cold = request(&tabs).tabs.pop().unwrap().session_tab;
        assert_eq!(
            db.hydrate(cold.clone()).await.unwrap().0.buffers[0]
                .text_history
                .len(),
            1
        );
        db.conn
            .execute(
                "UPDATE history SET entry=x'00' WHERE temp_id=?",
                [id.as_str()],
            )
            .await
            .unwrap();
        assert!(db.hydrate(cold).await.is_err());
        assert!(db.quarantined.contains_key(&id));
        db.persist(request(&[]), 3).await.unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM history").await.unwrap(), 1);
        assert_eq!(db.integer("SELECT count(*) FROM parts").await.unwrap(), 1);
    });
}

fn diverse_text(bytes: usize) -> String {
    let mut state = 42u64;
    (0..bytes)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (b' ' + (state % 95) as u8) as char
        })
        .collect()
}

#[test]
fn cdc_reuses_bodies_after_front_middle_end_edits_and_across_tabs() {
    block_on(async {
        let root = tempfile::tempdir().unwrap();
        let mut db = Database::open(root.path()).await.unwrap();
        let text = diverse_text(8 * 1024 * 1024);
        let mut tabs = vec![tab(&text), tab(&text)];
        db.persist(request(&tabs), 1).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM parts").await.unwrap(),
            2 * db.integer("SELECT count(*) FROM blobs").await.unwrap()
        );
        audit(&db).await;
        for (generation, at) in [1, text.len() / 2, text.len()].into_iter().enumerate() {
            reset(&db).await;
            insert(&mut tabs[0], at, "🦀");
            db.persist(request(&tabs), generation as u64 + 2)
                .await
                .unwrap();
            let written = db
                .integer("SELECT coalesce(sum(bytes),0) FROM writes WHERE kind='blob'")
                .await
                .unwrap();
            assert!(
                written > 0 && written < 1024 * 1024,
                "edited chunk bytes: {written}"
            );
            let contents = db.read_all_contents().await.unwrap();
            assert_eq!(
                contents[&tabs[0].buffers.buffer.temp_id],
                tabs[0].buffers.buffer.text()
            );
            assert_eq!(contents[&tabs[1].buffers.buffer.temp_id], text);
        }
        // No-op and metadata-only saves don't even attempt inserts into blobs.
        reset(&db).await;
        let mut captured = request(&tabs);
        captured.font_size += 1.0;
        db.persist(captured, 5).await.unwrap();
        assert_eq!(count(&db, "blob").await, 0);
        tabs.remove(0);
        db.persist(request(&tabs), 6).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM parts").await.unwrap(),
            db.integer("SELECT count(*) FROM blobs").await.unwrap()
        );
        db.persist(request(&[]), 7).await.unwrap();
        for table in ["parts", "blobs", "history", "buffer_ids", "snapshots"] {
            assert_eq!(
                db.integer(&format!("SELECT count(*) FROM {table}"))
                    .await
                    .unwrap(),
                0
            );
        }
    });
}

#[test]
fn opening_reserves_global_sequences_from_unhydrated_cold_history() {
    block_on(async {
        let root = tempfile::tempdir().unwrap();
        let mut db = Database::open(root.path()).await.unwrap();
        let mut tabs = vec![tab("active"), tab("cold")];
        insert(&mut tabs[1], 0, "entry");
        let mut captured = request(&tabs);
        let entry = &mut captured.tabs[1].session_tab.buffers[0].text_history[0];
        let high = entry.global_seq + 1_000_000;
        entry.global_seq = high;
        db.persist(captured, 1).await.unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        let shells = db.load(ReadScope::Shells).await.unwrap().unwrap();
        assert!(shells.manifest.tabs[1].buffers[0].text_history.is_empty());
        assert!(crate::app::domain::buffer::next_text_history_global_seq() > high);
    });
}

#[test]
fn cdc_boundaries_ignore_piece_segmentation_and_truncation_removes_stale_parts() {
    block_on(async {
        let root = tempfile::tempdir().unwrap();
        let mut db = Database::open(root.path()).await.unwrap();
        let text = diverse_text(1024 * 1024);
        let mut tabs = vec![tab(&text)];
        db.persist(request(&tabs), 1).await.unwrap();
        audit(&db).await;
        let mut fragmented = tab("");
        for chunk in text.as_bytes().chunks(7919) {
            let at = fragmented
                .buffers
                .buffer
                .document()
                .piece_tree()
                .len_chars();
            insert(&mut fragmented, at, std::str::from_utf8(chunk).unwrap());
        }
        tabs.push(fragmented);
        db.persist(request(&tabs), 2).await.unwrap();
        assert_eq!(count(&db, "blob").await, 0);
        tabs.remove(0);
        for value in ["short λ🦀", ""] {
            tabs[0].buffers.buffer.replace_text(value.into());
            db.persist(request(&tabs), 3).await.unwrap();
            assert_eq!(
                db.read_all_contents()
                    .await
                    .unwrap()
                    .into_values()
                    .next()
                    .unwrap(),
                value
            );
            assert_eq!(
                db.integer("SELECT count(*) FROM parts").await.unwrap(),
                i64::from(!value.is_empty())
            );
            assert_eq!(
                db.integer("SELECT count(*) FROM blobs").await.unwrap(),
                i64::from(!value.is_empty())
            );
        }
    });
}

#[test]
fn strict_types_and_foreign_keys_are_enforced_by_turso() {
    block_on(async {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).await.unwrap();
        assert_eq!(db.integer("PRAGMA foreign_keys").await.unwrap(), 1);
        assert!(
            db.conn
                .execute("INSERT INTO history VALUES('missing', 1, 1, x'00')", ())
                .await
                .is_err()
        );
        assert!(
            db.conn
                .execute("INSERT INTO parts VALUES('missing', 0, x'00')", ())
                .await
                .is_err()
        );
        assert!(
            db.conn
                .execute("INSERT INTO blobs VALUES(x'00', 'not a blob')", ())
                .await
                .is_err()
        );
        db.conn
            .execute("INSERT INTO buffer_ids VALUES(?)", params!["clean"])
            .await
            .unwrap();
        db.conn
            .execute("INSERT INTO history VALUES('clean', 1, 1, x'00')", ())
            .await
            .unwrap();
        db.conn.execute("DELETE FROM buffer_ids", ()).await.unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM history").await.unwrap(), 0);
    });
}
