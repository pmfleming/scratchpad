use super::{
    Database,
    tests::{database, request, tab},
    versions::VERSION_LIMIT,
};
use crate::app::domain::WorkspaceTab;
use crate::app::services::session_store::SessionPersistRequest;
use futures_lite::future::block_on;

fn captured(tabs: &[WorkspaceTab]) -> SessionPersistRequest {
    let mut request = request(tabs);
    request.version_history = true;
    request
}

#[test]
fn versions_are_periodic_bounded_durable_and_gc_roots() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab("initial")];
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(captured(&tabs), 1).await.unwrap();
        let points = db.version_history(&id).await.unwrap();
        let initial = points[0].id;
        let now = points[0].created_at_unix_seconds;
        tabs[0]
            .buffers
            .buffer
            .replace_text("before interval".into());
        db.persist(captured(&tabs), 2).await.unwrap();
        assert_eq!(db.version_history(&id).await.unwrap().len(), 1);
        assert_eq!(db.read_version(initial).await.unwrap(), "initial");
        let mut removed = std::collections::HashSet::new();
        // Clock rollback, just before/at the boundary, and unchanged content after it.
        for (time, count) in [(now - 1, 1), (now + 299, 1), (now + 300, 2), (now + 600, 2)] {
            db.checkpoint_version(&id, time, &mut removed)
                .await
                .unwrap();
            assert_eq!(
                db.version_history(&id).await.unwrap().len(),
                count,
                "time {time}"
            );
        }
        db.checkpoint_version("missing snapshot", now, &mut removed)
            .await
            .unwrap();
        assert!(
            db.version_history("missing snapshot")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(removed.is_empty());
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert_eq!(db.read_version(initial).await.unwrap(), "initial");
        for generation in 3..VERSION_LIMIT as u64 + 7 {
            db.conn
                .execute("UPDATE buffer_versions SET created_at=0", ())
                .await
                .unwrap();
            tabs[0]
                .buffers
                .buffer
                .replace_text(format!("version {generation}"));
            db.persist(captured(&tabs), generation).await.unwrap();
        }
        assert_eq!(db.version_history(&id).await.unwrap().len(), VERSION_LIMIT);
        assert!(db.read_version(initial).await.is_err());
        assert_eq!(
            db.integer("SELECT count(*) FROM blobs").await.unwrap(),
            VERSION_LIMIT as i64
        );
        // Expiring the last live/closed buffer also expires its save points.
        db.persist(captured(&[]), 100).await.unwrap();
        assert!(db.version_history(&id).await.unwrap().is_empty());
        assert_eq!(db.integer("SELECT count(*) FROM blobs").await.unwrap(), 0);
    });
}

#[test]
fn undo_boundary_is_durable_idempotent_and_rolls_back_with_history() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("before")];
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(captured(&tabs), 1).await.unwrap();
        let document = tabs[0].buffers.buffer.document_mut();
        document.replace_char_ranges_with_undo(&[(0..6, "after".into())], Default::default(), Default::default()).unwrap();
        document.drop_oldest_history_entry();
        db.conn.execute_batch("CREATE TRIGGER fail_boundary BEFORE INSERT ON buffer_versions WHEN NEW.source_token IS NOT NULL BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        assert!(db.persist(captured(&tabs), 2).await.is_err());
        assert!(tabs[0].active_buffer().document().recovery_snapshot().is_some());
        db.conn.execute("DROP TRIGGER fail_boundary", ()).await.unwrap();
        db.persist(captured(&tabs), 3).await.unwrap();
        db.persist(captured(&tabs), 4).await.unwrap();
        let points = db.version_history(&id).await.unwrap();
        let boundaries: Vec<_> = points.iter().filter(|point| point.undo_boundary).collect();
        assert_eq!(boundaries.len(), 1);
        assert_eq!(db.read_version(boundaries[0].id).await.unwrap(), "before");
        assert_eq!(db.load(super::ReadScope::All).await.unwrap().unwrap().contents[&id], "after");
    });
}

#[test]
fn version_references_share_existing_chunks_and_failures_roll_back() {
    block_on(async {
        let (_root, mut db) = database().await;
        let original = "x".repeat(768 * 1024);
        let mut tabs = vec![tab(&original)];
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(captured(&tabs), 1).await.unwrap();
        let initial = db.version_history(&id).await.unwrap()[0].id;
        let blobs = db.integer("SELECT count(*) FROM blobs").await.unwrap();
        db.conn
            .execute("UPDATE buffer_versions SET created_at=0", ())
            .await
            .unwrap();
        tabs[0]
            .buffers
            .buffer
            .replace_text(format!("{original}tail"));
        db.conn.execute_batch("CREATE TRIGGER fail_version BEFORE INSERT ON version_parts BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        assert!(db.persist(captured(&tabs), 2).await.is_err());
        assert_eq!(db.version_history(&id).await.unwrap().len(), 1);
        assert_eq!(db.read_version(initial).await.unwrap(), original);
        db.conn
            .execute("DROP TRIGGER fail_version", ())
            .await
            .unwrap();
        db.persist(captured(&tabs), 3).await.unwrap();
        assert_eq!(db.version_history(&id).await.unwrap().len(), 2);
        assert_eq!(
            db.integer("SELECT count(*) FROM blobs").await.unwrap(),
            blobs + 1
        );
        let latest = db.version_history(&id).await.unwrap()[0].id;
        assert_eq!(
            db.read_version(latest).await.unwrap(),
            format!("{original}tail")
        );
        db.conn
            .execute("UPDATE blobs SET body=x'00'", ())
            .await
            .unwrap();
        assert!(db.read_version(latest).await.is_err());
    });
}
