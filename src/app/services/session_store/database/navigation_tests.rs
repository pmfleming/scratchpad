use super::{
    Database,
    tests::{database, insert, request, tab},
    versions::HistoryCheckpoint,
};
use crate::app::domain::{TextDocument, WorkspaceTab};
use futures_lite::future::block_on;

fn checkpoint(tab: &WorkspaceTab) -> HistoryCheckpoint {
    let buffer = tab.active_buffer();
    HistoryCheckpoint {
        temp_id: buffer.temp_id.clone(),
        snapshot: buffer.document_snapshot(),
        history: buffer.document().exported_history(),
    }
}

#[test]
fn snapshot_navigation_keeps_native_history_and_deduplicates_return_states_across_restart() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut live = tab("ancient λ\r\n");
        let id = live.active_buffer().temp_id.clone();
        let mut save = request(&[live.clone()]);
        save.version_history = true;
        db.persist(save, 1).await.unwrap();
        let old = db.version_history(&id).await.unwrap()[0].id;
        insert(&mut live, 0, "recent ");
        db.persist(request(&[live.clone()]), 2).await.unwrap();
        let original = checkpoint(&live);
        let result = db.navigate_version(original.clone(), old).await.unwrap();
        assert_eq!(result.text, "ancient λ\r\n");
        assert!(result.history.is_empty());
        let return_id = result.return_version;
        assert_ne!(return_id, old);
        assert_eq!(db.version_history(&id).await.unwrap().len(), 2);
        assert_eq!(
            db.navigate_version(original, old)
                .await
                .unwrap()
                .return_version,
            return_id
        );
        assert_eq!(db.version_history(&id).await.unwrap().len(), 2);
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        let earlier = HistoryCheckpoint {
            temp_id: id.clone(),
            snapshot: TextDocument::new(result.text).snapshot(),
            history: Vec::new(),
        };
        let restored = db.navigate_version(earlier, return_id).await.unwrap();
        assert_eq!(restored.return_version, old);
        assert_eq!(restored.history.len(), 1);
        let mut document = TextDocument::new(restored.text);
        document.restore_exported_history(restored.history);
        assert_eq!(document.extract_text(), "recent ancient λ\r\n");
        document.undo_last_operation().unwrap();
        assert_eq!(document.extract_text(), "ancient λ\r\n");
        document.redo_last_operation().unwrap();
        assert_eq!(document.extract_text(), "recent ancient λ\r\n");
        assert_eq!(db.version_history(&id).await.unwrap().len(), 2);
    });
}

#[test]
fn parked_navigation_roots_survive_tiny_budgets_and_restart() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut live = tab("base");
        let temp_id = live.active_buffer().temp_id.clone();
        let mut save = request(std::slice::from_ref(&live));
        save.version_history = true;
        db.persist(save, 1).await.unwrap();
        let target = db.version_history(&temp_id).await.unwrap()[0].id;
        insert(&mut live, 0, "first ");
        insert(&mut live, 0, "second ");
        live.active_buffer_mut()
            .document_mut()
            .undo_last_operation()
            .unwrap();
        let point = checkpoint(&live);
        let high_water = point
            .history
            .iter()
            .map(|entry| entry.global_seq)
            .max()
            .unwrap();
        let restored = db.navigate_version(point, target).await.unwrap();
        let back = restored.return_version;
        live.active_buffer_mut()
            .install_history_document(TextDocument::new(restored.text));
        live.active_buffer_mut()
            .set_snapshot_navigation(Some((target, back)));
        let mut save = request(std::slice::from_ref(&live));
        save.recovery_policy.snapshot_byte_budget = 1;
        db.persist(save, 2).await.unwrap();
        assert_eq!(db.version_history(&temp_id).await.unwrap().len(), 2);
        assert!(
            db.stored_usage().await.unwrap().undo > 0,
            "saved native history is included in usage"
        );
        assert_eq!(
            db.integer("SELECT max(history_high_water) FROM buffer_versions")
                .await
                .unwrap() as u64,
            high_water
        );
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert!(
            db.navigation_roots.is_empty(),
            "no in-memory protection survives reopening"
        );
        live.active_buffer_mut().name.push_str(" renamed");
        let mut save = request(std::slice::from_ref(&live));
        save.recovery_policy.snapshot_byte_budget = 0;
        db.persist(save, 3).await.unwrap();
        assert_eq!(db.version_history(&temp_id).await.unwrap().len(), 2);
        let restored = db.read_history_version(&temp_id, back).await.unwrap();
        assert_eq!(restored.text, "first base");
        assert_eq!(restored.history.len(), 2);
        let mut document = TextDocument::new(restored.text);
        document.restore_exported_history(restored.history);
        document.redo_last_operation().unwrap();
        assert_eq!(document.extract_text(), "second first base");
    });
}

#[test]
fn failed_or_cross_file_navigation_cannot_discard_the_return_state() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut live = tab("before");
        let mut save = request(&[live.clone()]);
        save.version_history = true;
        db.persist(save, 1).await.unwrap();
        let old = db
            .version_history(&live.active_buffer().temp_id)
            .await
            .unwrap()[0]
            .id;
        insert(&mut live, 0, "after ");
        assert!(
            db.navigate_version(checkpoint(&tab("unrelated")), old)
                .await
                .is_err()
        );
        db.conn.execute_batch("CREATE TRIGGER refuse_navigation BEFORE INSERT ON buffer_versions BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        assert!(db.navigate_version(checkpoint(&live), old).await.is_err());
        assert!(db.conn.is_autocommit().unwrap());
        assert_eq!(
            db.integer("SELECT count(*) FROM buffer_versions")
                .await
                .unwrap(),
            1
        );
        assert_eq!(db.read_version(old).await.unwrap(), "before");
    });
}

#[test]
fn schema_eight_upgrades_and_legacy_snapshot_identity_is_backfilled() {
    block_on(async {
        let (root, mut db) = database().await;
        let live = tab("legacy snapshot");
        let id = live.active_buffer().temp_id.clone();
        let mut save = request(&[live]);
        save.version_history = true;
        db.persist(save, 1).await.unwrap();
        db.conn
            .execute_batch(
                "DROP INDEX version_sequences; ALTER TABLE buffer_versions DROP COLUMN history_high_water;
            DROP INDEX version_sizes; ALTER TABLE buffer_versions DROP COLUMN branch_bytes;
            ALTER TABLE buffer_versions DROP COLUMN text_digest;
            ALTER TABLE buffer_versions DROP COLUMN anchor_seq;
            ALTER TABLE buffer_versions DROP COLUMN anchor_before;
            ALTER TABLE buffer_versions DROP COLUMN branch_history;
            PRAGMA user_version=8;",
            )
            .await
            .unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert_eq!(db.integer("PRAGMA user_version").await.unwrap(), 9);
        let points = db.version_history(&id).await.unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(
            points[0].text_digest,
            blake3::hash(b"legacy snapshot").as_bytes()
        );
        assert_eq!(
            db.read_version(points[0].id).await.unwrap(),
            "legacy snapshot"
        );
    });
}
