use super::{
    Database,
    tests::{database, request, tab},
};
use futures_lite::future::block_on;

async fn assert_inventory(db: &Database) {
    let inventory = db.inventory.as_ref().unwrap();
    assert_eq!(
        inventory.buffers,
        db.string_set("SELECT id FROM documents").await.unwrap()
    );
    assert_eq!(
        inventory.snapshots,
        db.string_set("SELECT id FROM snapshots").await.unwrap()
    );
    assert!(!inventory.full_cleanup_pending);
}

#[test]
fn navigation_preserves_inventory_without_hiding_unknown_writes() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut live = tab("original λ");
        db.persist(request(std::slice::from_ref(&live)), 1)
            .await
            .unwrap();
        super::tests::insert(&mut live, 0, "new ");
        live.active_buffer_mut().navigate_history(1).unwrap();
        db.persist(request(std::slice::from_ref(&live)), 2)
            .await
            .unwrap();
        assert_inventory(&db).await;
        db.conn
            .execute("INSERT INTO documents(id) VALUES('unexpected')", ())
            .await
            .unwrap();
        db.persist(request(std::slice::from_ref(&live)), 3)
            .await
            .unwrap();
        assert_inventory(&db).await;
        assert!(
            db.inventory
                .as_ref()
                .unwrap()
                .buffers
                .contains("unexpected")
        );
    });
}

#[test]
fn warm_and_reopened_saves_do_not_even_attempt_to_reinsert_known_documents() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab("one"), tab("two")];
        db.persist(request(&tabs), 1).await.unwrap();
        // BEFORE INSERT observes redundant ON CONFLICT DO NOTHING attempts too.
        db.conn
            .execute_batch(
                "CREATE TRIGGER reject_redundant_buffer_insert BEFORE INSERT ON documents
            WHEN EXISTS(SELECT 1 FROM documents WHERE id=new.id)
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
        let removed = tabs.remove(0);
        let mut save = request(&tabs);
        save.removed_documents
            .push(removed.active_buffer().temp_id.clone());
        db.persist(save, 2).await.unwrap();
        assert_eq!(db.integer("SELECT count(*) FROM blobs").await.unwrap(), 2);
        // Remove the last old reference while creating a new one in this transaction.
        let removed = tabs.remove(0);
        tabs[0].buffers.buffer.replace_text("shared body".into());
        let mut save = request(&tabs);
        save.removed_documents
            .push(removed.active_buffer().temp_id.clone());
        db.persist(save, 3).await.unwrap();
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
        let mut save = request(&[]);
        save.removed_documents
            .extend(tabs.iter().map(|t| t.active_buffer().temp_id.clone()));
        db.persist(save, 4).await.unwrap();
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
        let mut save = request(&replacement);
        save.removed_documents
            .push(original[0].active_buffer().temp_id.clone());
        assert!(db.persist(save, 2).await.is_err());
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
        let mut save = request(&replacement);
        save.removed_documents
            .push(original[0].active_buffer().temp_id.clone());
        db.persist(save, 3).await.unwrap();
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
