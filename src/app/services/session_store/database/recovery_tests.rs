use super::{
    Database, ReadScope,
    quarantine::damaged,
    tests::{database, insert, request, tab},
};
use futures_lite::future::block_on;

#[test]
fn stale_length_repairs_on_restore_with_audit_and_exact_history() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab("before λ")];
        insert(&mut tabs[0], 8, "🦀");
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        db.conn
            .execute("UPDATE document_pieces SET bytes=999 WHERE part=0", ())
            .await
            .unwrap();
        let mut restored = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(restored.text_contents()[&id], "before λ🦀");
        assert!(db.quarantined.is_empty());
        assert_eq!(
            db.integer("SELECT old_bytes FROM recovery_repairs")
                .await
                .unwrap(),
            999
        );
        assert_eq!(db.startup_notices.len(), 1);
        let mut document = restored.manifest.tabs[0].buffers[0]
            .history_document
            .take()
            .unwrap();
        document.undo_last_operation().unwrap();
        assert_eq!(document.try_extract_text().unwrap(), "before λ");
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert_eq!(
            db.load(ReadScope::All)
                .await
                .unwrap()
                .unwrap()
                .text_contents()[&id],
            "before λ🦀"
        );
        assert!(db.startup_notices.is_empty());
        assert_eq!(
            db.integer("SELECT count(*) FROM recovery_repairs")
                .await
                .unwrap(),
            1
        );
    });
}

#[test]
fn existing_length_quarantine_reopens_and_retains_original_archive() {
    block_on(async {
        let (root, mut db) = database().await;
        let tabs = vec![tab("precious")];
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        db.conn
            .execute("UPDATE document_pieces SET bytes=90", ())
            .await
            .unwrap();
        let metadata = request(&tabs).tabs.remove(0).session_tab;
        db.protect_damaged(&damaged(&id, "piece text length mismatch"), &[metadata])
            .await
            .unwrap();
        let archive = db
            .query_collect("SELECT payload FROM quarantine", (), |r| {
                r.get::<Vec<u8>>(0).map_err(super::error)
            })
            .await
            .unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert!(db.quarantined.is_empty());
        assert_eq!(
            db.load(ReadScope::All)
                .await
                .unwrap()
                .unwrap()
                .text_contents()[&id],
            "precious"
        );
        assert_eq!(
            db.query_collect("SELECT quarantine_payload FROM recovery_repairs", (), |r| r
                .get::<Vec<u8>>(0)
                .map_err(super::error))
                .await
                .unwrap(),
            archive
        );
    });
}

#[test]
fn failed_repair_rolls_back_count_archive_and_quarantine_together() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab("precious")];
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        db.conn
            .execute("UPDATE document_pieces SET bytes=90", ())
            .await
            .unwrap();
        db.protect_damaged(
            &damaged(&id, "piece text length mismatch"),
            &[request(&tabs).tabs.remove(0).session_tab],
        )
        .await
        .unwrap();
        db.conn.execute_batch("CREATE TRIGGER refuse_repair BEFORE DELETE ON quarantine BEGIN SELECT RAISE(ABORT,'injected'); END;").await.unwrap();
        db.repair_quarantined_lengths().await.unwrap();
        assert_eq!(
            db.integer("SELECT bytes FROM document_pieces")
                .await
                .unwrap(),
            90
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM quarantine").await.unwrap(),
            1
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM sqlite_master WHERE name='recovery_repairs'")
                .await
                .unwrap(),
            0
        );
        assert_eq!(db.startup_notices.len(), 1);
        assert!(db.startup_notices[0].message.contains("Could not repair"));
        assert!(db.quarantined.contains_key(&id));
        db.conn
            .execute_batch("DROP TRIGGER refuse_repair;")
            .await
            .unwrap();
        db.repair_quarantined_lengths().await.unwrap();
        assert!(db.quarantined.is_empty());
    });
}

#[test]
fn bad_history_offers_verified_text_as_a_new_document_without_consuming_archive() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab("precious λ🦀")];
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        db.conn
            .execute("UPDATE history SET packed_spans=x'00'", ())
            .await
            .unwrap();
        assert!(db.load(ReadScope::All).await.is_err());
        let preview = db.recovery_preview(&id).await.unwrap();
        assert_eq!(preview.gaps, 0);
        assert_eq!(preview.text, "precious λ🦀");
        let copy = preview.editable_copy();
        assert_ne!(copy.active_buffer().temp_id, id);
        assert!(copy.active_buffer().path.is_none());
        assert!(copy.active_buffer().is_dirty);
        db.persist(request(&[copy]), 2).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM quarantine").await.unwrap(),
            1
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM history WHERE packed_spans=x'00'")
                .await
                .unwrap(),
            1
        );
    });
}

#[test]
fn damaged_tail_is_marked_while_verified_original_text_is_recoverable() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("good original")];
        db.persist(request(&tabs), 1).await.unwrap();
        insert(&mut tabs[0], 13, " damaged addition");
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 2).await.unwrap();
        db.conn
            .execute("UPDATE document_tail SET body=x'00' WHERE offset>0", ())
            .await
            .unwrap();
        assert!(db.load(ReadScope::All).await.is_err());
        let preview = db.recovery_preview(&id).await.unwrap();
        assert!(preview.text.starts_with("good original"), "{preview:?}");
        assert!(preview.text.contains("[RECOVERY GAP:"));
        assert!(!preview.text.contains("damaged addition"));
        assert_eq!(preview.gaps, 1);
        assert_eq!(
            db.integer("SELECT count(*) FROM document_tail WHERE body=x'00'")
                .await
                .unwrap(),
            1
        );
    });
}

#[test]
fn length_mismatch_with_damaged_or_disagreeing_history_is_not_repaired() {
    block_on(async {
        for broken in [true, false] {
            let (_root, mut db) = database().await;
            let tabs = vec![tab("unchanged")];
            let id = tabs[0].active_buffer().temp_id.clone();
            db.persist(request(&tabs), 1).await.unwrap();
            db.conn
                .execute("UPDATE document_pieces SET bytes=90", ())
                .await
                .unwrap();
            if broken {
                db.conn
                    .execute("UPDATE history SET packed_spans=x'00'", ())
                    .await
                    .unwrap();
            } else {
                // The empty root is a valid graph position but disagrees with current text.
                db.conn
                    .execute("UPDATE document_history SET now_id=0", ())
                    .await
                    .unwrap();
            }
            assert!(
                !db.repair_length_error(&damaged(&id, "piece text length mismatch"))
                    .await
                    .unwrap()
            );
            assert_eq!(
                db.integer("SELECT bytes FROM document_pieces")
                    .await
                    .unwrap(),
                90
            );
            assert!(db.startup_notices.is_empty());
        }
    });
}

#[test]
fn corrupt_piece_map_never_guesses_text_order() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab("precious")];
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        db.conn
            .execute("UPDATE document_pieces SET body=x'00'", ())
            .await
            .unwrap();
        assert!(db.load(ReadScope::All).await.is_err());
        let preview = db.recovery_preview(&id).await.unwrap();
        assert_eq!(preview.gaps, 1);
        assert!(preview.text.contains("text order cannot be established"));
        assert!(!preview.text.contains("precious"));
    });
}
