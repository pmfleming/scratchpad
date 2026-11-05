use super::{
    Database, ReadScope,
    tests::{database, insert, request as exported_request, tab},
};
use crate::app::services::session_store::{CapturedClosedTab, SessionPersistRequest};
use futures_lite::future::block_on;
fn request(tabs: &[crate::app::domain::WorkspaceTab]) -> SessionPersistRequest {
    SessionPersistRequest::capture(tabs, 0, 14.0, false)
}
async fn audit(db: &Database) {
    db.conn.execute_batch("CREATE TABLE writes(kind TEXT NOT NULL,bytes INTEGER NOT NULL) STRICT;
    CREATE TRIGGER history_insert AFTER INSERT ON history BEGIN INSERT INTO writes VALUES('insert',new.bytes); END;
    CREATE TRIGGER history_update AFTER UPDATE ON history BEGIN INSERT INTO writes VALUES('update',new.bytes); END;
    CREATE TRIGGER history_delete AFTER DELETE ON history BEGIN INSERT INTO writes VALUES('delete',0); END;
    CREATE TRIGGER body_insert AFTER INSERT ON chunk_bodies BEGIN INSERT INTO writes VALUES('body',length(new.body)); END;").await.unwrap();
}
async fn count(db: &Database, kind: &str) -> i64 {
    db.integer(&format!("SELECT count(*) FROM writes WHERE kind='{kind}'"))
        .await
        .unwrap()
}
async fn reset(db: &Database) {
    db.conn.execute("DELETE FROM writes", ()).await.unwrap();
}

#[test]
fn history_appends_updates_prunes_and_restarts_without_rewriting_unchanged_entries() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab("base")];
        for _ in 0..4 {
            insert(&mut tabs[0], 0, "entry");
            tabs[0]
                .active_buffer_mut()
                .document_mut()
                .seal_history_group();
        }
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        audit(&db).await;
        insert(&mut tabs[0], 0, "new");
        db.persist(request(&tabs), 2).await.unwrap();
        assert_eq!(count(&db, "insert").await, 1);
        assert_eq!(count(&db, "update").await, 0);
        reset(&db).await;
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .undo_last_operation()
            .unwrap();
        db.persist(request(&tabs), 3).await.unwrap();
        assert_eq!(count(&db, "update").await, 0);
        assert_eq!(count(&db, "insert").await, 0);
        let before = db.read_history(&id).await.unwrap();
        tabs[0]
            .active_buffer_mut()
            .document_mut()
            .clear_operation_history();
        reset(&db).await;
        db.persist(request(&tabs), 4).await.unwrap();
        assert_eq!(count(&db, "delete").await, before.len() as i64 - 1);
        let expected = tabs[0].active_buffer().document().exported_history();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        reset(&db).await;
        db.persist(request(&tabs), 1).await.unwrap();
        assert_eq!(
            count(&db, "insert").await + count(&db, "update").await + count(&db, "delete").await,
            0
        );
        assert_eq!(db.read_history(&id).await.unwrap(), expected);
    });
}
#[test]
fn write_failures_roll_back_reference_manifest_before_retry() {
    block_on(async {
        for (table, operation) in [
            ("history", "INSERT"),
            ("history_spans", "INSERT"),
            ("document_pieces", "INSERT"),
            ("document_tail", "INSERT"),
            ("layout_views", "UPDATE"),
            ("session_state", "INSERT"),
            ("closed_tabs", "INSERT"),
            ("documents", "DELETE"),
        ] {
            let (_root, mut db) = database().await;
            let mut tabs = vec![tab("original"), tab("obsolete")];
            db.persist(request(&tabs), 1).await.unwrap();
            let original = db.read_all_contents().await.unwrap();
            let id = tabs[0].active_buffer().temp_id.clone();
            let history = db.read_history(&id).await.unwrap();
            db.conn.execute_batch(&format!("CREATE TRIGGER fail_write BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT,'injected'); END;")).await.unwrap();
            let removed = tabs.pop().unwrap().active_buffer().temp_id.clone();
            insert(&mut tabs[0], 0, "new");
            let changed = || {
                let mut r = request(&tabs);
                r.active_surface = super::super::SessionActiveSurface::Settings;
                r.removed_documents.push(removed.clone());
                r.tabs[0].session_tab.views[0].show_line_numbers = true;
                if table == "closed_tabs" {
                    r.closed_tabs
                        .push(CapturedClosedTab::capture(&tab("archived"), None));
                }
                r
            };
            assert!(db.persist(changed(), 2).await.is_err(), "{table}");
            assert!(db.conn.is_autocommit().unwrap());
            assert!(db.inventory.is_none());
            assert!(db.history_cache.is_empty());
            assert_eq!(db.latest_request, 1);
            assert_eq!(db.read_all_contents().await.unwrap(), original, "{table}");
            assert_eq!(db.read_history(&id).await.unwrap(), history);
            db.conn
                .execute("DROP TRIGGER fail_write", ())
                .await
                .unwrap();
            db.persist(changed(), 2).await.unwrap();
            assert_eq!(db.read_all_contents().await.unwrap()[&id], "neworiginal");
        }
    });
}
#[test]
fn cold_history_is_lazy_and_survives_metadata_saves_and_quarantine() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("active"), tab("cold")];
        insert(&mut tabs[1], 0, "history");
        let id = tabs[1].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        let mut shells = db
            .load(ReadScope::Shells)
            .await
            .unwrap()
            .unwrap()
            .manifest
            .tabs;
        assert!(shells[1].buffers[0].history_document.is_none());
        let mut captured = request(&tabs);
        captured.tabs[1].session_tab = shells.pop().unwrap();
        captured.tabs[1].preserve_existing = true;
        captured.tabs[1].buffer_snapshots.clear();
        db.persist(captured, 2).await.unwrap();
        let cold = request(&tabs).tabs.pop().unwrap().session_tab;
        assert_eq!(
            db.hydrate(cold.clone()).await.unwrap().buffers[0]
                .history_document
                .as_ref()
                .unwrap()
                .history_entries()
                .len(),
            2
        );
        db.conn
            .execute(
                "UPDATE history SET packed_spans=x'00' WHERE temp_id=?",
                [id.as_str()],
            )
            .await
            .unwrap();
        assert!(db.hydrate(cold).await.is_err());
        assert!(db.quarantined.contains_key(&id));
        db.persist(request(&[]), 3).await.unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM history").await.unwrap(), 3);
    });
}
#[test]
fn references_reuse_original_bodies_after_front_middle_end_edits_and_across_tabs() {
    block_on(async {
        let (_root, mut db) = database().await;
        let text = "unique original λ🦀\r\n".repeat(50_000);
        let mut tabs = vec![tab(&text), tab(&text)];
        db.persist(request(&tabs), 1).await.unwrap();
        assert!(
            db.integer("SELECT count(*) FROM document_text")
                .await
                .unwrap()
                >= 2 * db.integer("SELECT count(*) FROM chunks").await.unwrap()
        );
        audit(&db).await;
        for (generation, at) in [0, text.chars().count() / 2, text.chars().count()]
            .into_iter()
            .enumerate()
        {
            reset(&db).await;
            insert(&mut tabs[0], at, "🦀");
            db.persist(request(&tabs), generation as u64 + 2)
                .await
                .unwrap();
            assert_eq!(count(&db, "body").await, 0);
            let contents = db.read_all_contents().await.unwrap();
            assert_eq!(
                contents[&tabs[0].active_buffer().temp_id],
                tabs[0].active_buffer().text()
            );
            assert_eq!(contents[&tabs[1].active_buffer().temp_id], text);
        }
        reset(&db).await;
        db.persist(request(&tabs), 5).await.unwrap();
        assert_eq!(count(&db, "body").await, 0);
    });
}
#[test]
fn opening_reserves_global_sequences_from_unhydrated_cold_history() {
    block_on(async {
        let (root, mut db) = database().await;
        let tabs = vec![tab("active"), tab("cold")];
        let mut captured = exported_request(&tabs);
        let entry = &mut captured.tabs[1].session_tab.buffers[0].text_history[0];
        let high = entry.global_seq + 1_000_000;
        entry.global_seq = high;
        db.persist(captured, 1).await.unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        let shells = db.load(ReadScope::Shells).await.unwrap().unwrap();
        assert!(
            shells.manifest.tabs[1].buffers[0]
                .history_document
                .is_none()
        );
        assert!(crate::app::domain::buffer::next_text_history_global_seq() > high);
    });
}
#[test]
fn strict_types_and_foreign_keys_are_enforced_by_turso() {
    block_on(async {
        let (_root, mut db) = database().await;
        assert_eq!(db.integer("PRAGMA foreign_keys").await.unwrap(), 1);
        assert!(
            db.conn
                .execute("INSERT INTO history_nodes VALUES('missing',0)", ())
                .await
                .is_err()
        );
        assert!(
            db.conn
                .execute(
                    "INSERT INTO history_spans VALUES('missing',1,0,0,0,0,0,0,1,1)",
                    ()
                )
                .await
                .is_err()
        );
        assert!(
            db.conn
                .execute("INSERT INTO document_text VALUES('missing',0,0,0,1)", ())
                .await
                .is_err()
        );
        db.persist(request(&[tab("durable")]), 1).await.unwrap();
        assert!(
            db.conn
                .execute("UPDATE chunk_bodies SET body='not a blob'", ())
                .await
                .is_err()
        );
        db.conn.execute("DELETE FROM tabs", ()).await.unwrap();
        db.conn.execute("DELETE FROM layouts", ()).await.unwrap();
        db.conn.execute("DELETE FROM documents", ()).await.unwrap();
        for table in [
            "history",
            "history_spans",
            "document_text",
            "document_tail",
            "document_pieces",
            "document_history",
        ] {
            assert_eq!(
                db.integer(&format!("SELECT count(*) FROM {table}"))
                    .await
                    .unwrap(),
                0
            );
        }
    });
}
