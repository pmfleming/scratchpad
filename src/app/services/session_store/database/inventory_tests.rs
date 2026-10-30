use super::{
    Database,
    tests::{database, request, tab},
};
use futures_lite::future::block_on;

async fn assert_inventory(db: &Database) {
    let inventory = db.inventory.as_ref().unwrap();
    assert_eq!(
        inventory.buffers,
        db.string_set("SELECT id FROM buffer_ids").await.unwrap()
    );
    assert_eq!(
        inventory.snapshots,
        db.string_set("SELECT id FROM snapshots").await.unwrap()
    );
    assert!(!inventory.full_cleanup_pending);
}

#[test]
fn history_backfill_and_navigation_preserve_inventory_without_hiding_unknown_writes() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut live = tab("original λ");
        let id = live.active_buffer().temp_id.clone();
        let mut save = request(std::slice::from_ref(&live));
        save.version_history = true;
        db.persist(save, 1).await.unwrap();
        let target = db.version_history(&id).await.unwrap()[0].id;
        db.conn
            .execute("UPDATE buffer_versions SET text_digest=NULL", ())
            .await
            .unwrap();
        // Model a legacy row already present when the inventory was loaded.
        db.inventory.as_mut().unwrap().connection_changes =
            Some(db.integer("SELECT total_changes()").await.unwrap());
        assert_eq!(
            db.version_history(&id).await.unwrap()[0].text_digest,
            blake3::hash("original λ".as_bytes()).as_bytes()
        );
        assert_inventory(&db).await;
        assert_eq!(
            db.inventory.as_ref().unwrap().connection_changes,
            Some(db.integer("SELECT total_changes()").await.unwrap())
        );
        super::tests::insert(&mut live, 0, "new ");
        db.navigate_version(
            super::versions::HistoryCheckpoint {
                temp_id: id.clone(),
                snapshot: live.active_buffer().document_snapshot(),
                history: live.active_buffer().document().exported_history(),
            },
            target,
        )
        .await
        .unwrap();
        assert_inventory(&db).await;
        assert!(db.chunk_cache.contains_key(&id));
        assert_eq!(
            db.inventory.as_ref().unwrap().connection_changes,
            Some(db.integer("SELECT total_changes()").await.unwrap())
        );
        db.persist(request(std::slice::from_ref(&live)), 2)
            .await
            .unwrap();
        // Unknown writes preceding a known backfill must not be acknowledged.
        db.conn.execute_batch("INSERT INTO buffer_ids VALUES('unexpected'); UPDATE buffer_versions SET text_digest=NULL;").await.unwrap();
        db.version_history(&id).await.unwrap();
        assert_ne!(
            db.inventory.as_ref().unwrap().connection_changes,
            Some(db.integer("SELECT total_changes()").await.unwrap())
        );
        db.persist(request(std::slice::from_ref(&live)), 3)
            .await
            .unwrap();
        assert_inventory(&db).await;
        assert!(
            !db.inventory
                .as_ref()
                .unwrap()
                .buffers
                .contains("unexpected")
        );
    });
}

#[test]
fn warm_and_reopened_saves_do_not_even_attempt_to_reinsert_known_buffer_ids() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab("one"), tab("two")];
        db.persist(request(&tabs), 1).await.unwrap();
        // BEFORE INSERT observes redundant ON CONFLICT DO NOTHING attempts too.
        db.conn
            .execute_batch(
                "CREATE TRIGGER reject_redundant_buffer_insert BEFORE INSERT ON buffer_ids
            WHEN EXISTS(SELECT 1 FROM buffer_ids WHERE id=new.id)
            BEGIN SELECT RAISE(ABORT, 'redundant buffer insert'); END;",
            )
            .await
            .unwrap();
        tabs[0].buffers.buffer.name = "renamed".into();
        db.persist(request(&tabs), 2).await.unwrap();
        tabs.push(tab("new identity"));
        db.persist(request(&tabs), 3).await.unwrap();
        assert_inventory(&db).await;
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        db.persist(request(&tabs), 1).await.unwrap();
        assert_inventory(&db).await;
    });
}

#[test]
fn targeted_collection_preserves_shared_and_new_references_then_collects_last_reference() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![
            tab("shared body"),
            tab("shared body"),
            tab("old unique body"),
        ];
        db.persist(request(&tabs), 1).await.unwrap();
        tabs.remove(0);
        db.persist(request(&tabs), 2).await.unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM blobs").await.unwrap(), 2);
        // Remove the last old reference while creating a new one in this transaction.
        tabs.remove(0);
        tabs[0].buffers.buffer.replace_text("shared body".into());
        db.persist(request(&tabs), 3).await.unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM blobs").await.unwrap(), 1);
        assert_eq!(
            db.read_all_contents()
                .await
                .unwrap()
                .into_values()
                .next()
                .unwrap(),
            "shared body"
        );
        assert_inventory(&db).await;
        tabs.clear();
        db.persist(request(&tabs), 4).await.unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM blobs").await.unwrap(), 0);
        assert_inventory(&db).await;
    });
}

#[test]
fn failed_cleanup_invalidates_inventory_and_retry_rebuilds_it_from_committed_rows() {
    block_on(async {
        let (_root, mut db) = database().await;
        let original = vec![tab("committed")];
        db.persist(request(&original), 1).await.unwrap();
        db.conn
            .execute_batch(
                "CREATE TRIGGER fail_cleanup BEFORE DELETE ON blobs
            BEGIN SELECT RAISE(ABORT, 'cleanup failure'); END;",
            )
            .await
            .unwrap();
        let replacement = vec![tab("new buffer")];
        assert!(db.persist(request(&replacement), 2).await.is_err());
        assert!(db.inventory.is_none());
        assert_eq!(
            db.read_all_contents()
                .await
                .unwrap()
                .into_values()
                .next()
                .unwrap(),
            "committed"
        );
        db.conn
            .execute("DROP TRIGGER fail_cleanup", ())
            .await
            .unwrap();
        db.persist(request(&replacement), 3).await.unwrap();
        assert_inventory(&db).await;
        assert_eq!(db.integer("SELECT count(*) FROM blobs").await.unwrap(), 1);
        assert_eq!(
            db.read_all_contents()
                .await
                .unwrap()
                .into_values()
                .next()
                .unwrap(),
            "new buffer"
        );
    });
}
