use super::{
    Database, ReadScope,
    quarantine::damaged,
    test_fixture::{blobs, committed, database, exec, insert, request, scalar, tab, texts},
};
use futures_lite::future::block_on;

const LENGTH_MISMATCH: &str = "piece text length mismatch";

/// A committed tab whose stored length is wrong and which was quarantined for it.
async fn length_quarantined(text: &str) -> (tempfile::TempDir, Database, String) {
    let (root, mut db, tabs, id) = committed(text).await;
    exec(&db, "UPDATE document_pieces SET bytes=90").await;
    let metadata = request(&tabs).tabs.remove(0).session_tab;
    db.protect_damaged(&damaged(&id, LENGTH_MISMATCH), &[metadata])
        .await
        .unwrap();
    (root, db, id)
}

#[test]
fn stale_length_repairs_on_restore_with_audit_and_exact_history() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab("before λ")];
        insert(&mut tabs[0], 8, "🦀");
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        exec(&db, "UPDATE document_pieces SET bytes=999 WHERE part=0").await;
        let mut restored = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(restored.text_contents()[&id], "before λ🦀");
        assert!(db.quarantined.is_empty());
        assert_eq!(
            scalar(&db, "SELECT old_bytes FROM recovery_repairs").await,
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
        assert_eq!(texts(&mut db, ReadScope::All).await[&id], "before λ🦀");
        assert!(db.startup_notices.is_empty());
        assert_eq!(
            scalar(&db, "SELECT count(*) FROM recovery_repairs").await,
            1
        );
    });
}

#[test]
fn existing_length_quarantine_reopens_and_retains_original_archive() {
    block_on(async {
        let (root, db, id) = length_quarantined("precious").await;
        let archive = blobs(&db, "SELECT payload FROM quarantine").await;
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert!(db.quarantined.is_empty());
        assert_eq!(texts(&mut db, ReadScope::All).await[&id], "precious");
        assert_eq!(
            blobs(&db, "SELECT quarantine_payload FROM recovery_repairs").await,
            archive
        );
    });
}

#[test]
fn failed_repair_rolls_back_count_archive_and_quarantine_together() {
    block_on(async {
        let (_root, mut db, id) = length_quarantined("precious").await;
        exec(&db, "CREATE TRIGGER refuse_repair BEFORE DELETE ON quarantine BEGIN SELECT RAISE(ABORT,'injected'); END;").await;
        db.repair_quarantined_lengths().await.unwrap();
        for (sql, expected) in [
            ("SELECT bytes FROM document_pieces", 90),
            ("SELECT count(*) FROM quarantine", 1),
            (
                "SELECT count(*) FROM sqlite_master WHERE name='recovery_repairs'",
                0,
            ),
        ] {
            assert_eq!(scalar(&db, sql).await, expected, "{sql}");
        }
        assert_eq!(db.startup_notices.len(), 1);
        assert!(db.startup_notices[0].message.contains("Could not repair"));
        assert!(db.quarantined.contains_key(&id));
        exec(&db, "DROP TRIGGER refuse_repair;").await;
        db.repair_quarantined_lengths().await.unwrap();
        assert!(db.quarantined.is_empty());
    });
}

#[test]
fn bad_history_offers_verified_text_as_a_new_document_without_consuming_archive() {
    block_on(async {
        let (_root, mut db, _, id) = committed("precious λ🦀").await;
        exec(&db, "UPDATE history SET packed_spans=x'00'").await;
        assert!(db.load(ReadScope::All).await.is_err());
        let preview = db.recovery_preview(&id).await.unwrap();
        assert_eq!(preview.gaps, 0);
        assert_eq!(preview.text, "precious λ🦀");
        let copy = preview.editable_copy();
        assert_ne!(copy.active_buffer().temp_id, id);
        assert!(copy.active_buffer().path.is_none());
        assert!(copy.active_buffer().is_dirty);
        db.persist(request(&[copy]), 2).await.unwrap();
        assert_eq!(scalar(&db, "SELECT count(*) FROM quarantine").await, 1);
        let damaged_rows = "SELECT count(*) FROM history WHERE packed_spans=x'00'";
        assert_eq!(scalar(&db, damaged_rows).await, 1);
    });
}

#[test]
fn damaged_tail_is_marked_while_verified_original_text_is_recoverable() {
    block_on(async {
        let (_root, mut db, mut tabs, id) = committed("good original").await;
        insert(&mut tabs[0], 13, " damaged addition");
        db.persist(request(&tabs), 2).await.unwrap();
        exec(&db, "UPDATE document_tail SET body=x'00' WHERE offset>0").await;
        assert!(db.load(ReadScope::All).await.is_err());
        let preview = db.recovery_preview(&id).await.unwrap();
        assert!(preview.text.starts_with("good original"), "{preview:?}");
        assert!(preview.text.contains("[RECOVERY GAP:"));
        assert!(!preview.text.contains("damaged addition"));
        assert_eq!(preview.gaps, 1);
        let damaged_rows = "SELECT count(*) FROM document_tail WHERE body=x'00'";
        assert_eq!(scalar(&db, damaged_rows).await, 1);
    });
}

#[test]
fn length_mismatch_with_damaged_or_disagreeing_history_is_not_repaired() {
    block_on(async {
        for history_damage in [
            "UPDATE history SET packed_spans=x'00'",
            // The empty root is a valid graph position but disagrees with current text.
            "UPDATE document_history SET now_id=0",
        ] {
            let (_root, mut db, _, id) = committed("unchanged").await;
            exec(&db, "UPDATE document_pieces SET bytes=90").await;
            exec(&db, history_damage).await;
            let error = damaged(&id, LENGTH_MISMATCH);
            assert!(!db.repair_length_error(&error).await.unwrap(), "{history_damage}");
            assert_eq!(scalar(&db, "SELECT bytes FROM document_pieces").await, 90);
            assert!(db.startup_notices.is_empty());
        }
    });
}

#[test]
fn corrupt_piece_map_never_guesses_text_order() {
    block_on(async {
        let (_root, mut db, _, id) = committed("precious").await;
        exec(&db, "UPDATE document_pieces SET body=x'00'").await;
        assert!(db.load(ReadScope::All).await.is_err());
        let preview = db.recovery_preview(&id).await.unwrap();
        assert_eq!(preview.gaps, 1);
        assert!(preview.text.contains("text order cannot be established"));
        assert!(!preview.text.contains("precious"));
    });
}
