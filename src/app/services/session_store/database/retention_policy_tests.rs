use super::{
    Database,
    tests::{database, insert, request, tab},
};
use crate::app::services::{session_store::CapturedClosedTab, settings_store::RecoverySettings};
use futures_lite::future::block_on;

#[test]
fn age_and_budget_cleanup_preserve_pins_and_roll_back_on_failure() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut old = CapturedClosedTab::capture(&tab("old"), None);
        old.closed_at = 0;
        let mut pinned = CapturedClosedTab::capture(&tab("protected"), None);
        pinned.closed_at = 0;
        let token = pinned.token.clone();
        let mut initial = request(&[]);
        initial.recovery_policy.archive_retention_days = 0;
        initial.closed_tabs = vec![old, pinned];
        db.persist(initial, 1).await.unwrap();
        let policy = RecoverySettings {
            archive_retention_days: 1,
            archive_byte_budget: 1,
            pinned_archives: vec![token.clone()],
            ..Default::default()
        };
        db.conn.execute_batch("CREATE TRIGGER fail_cleanup BEFORE DELETE ON closed_tabs BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        let mut cleanup = request(&[]);
        cleanup.recovery_policy = policy.clone();
        assert!(db.persist(cleanup, 2).await.is_err());
        assert_eq!(db.closed_rows().await.unwrap().len(), 2);
        db.conn
            .execute("DROP TRIGGER fail_cleanup", ())
            .await
            .unwrap();
        let mut cleanup = request(&[]);
        cleanup.recovery_policy = policy;
        db.persist(cleanup, 3).await.unwrap();
        let archives = db.closed_rows().await.unwrap();
        assert_eq!(archives.len(), 1);
        assert_eq!(archives[0].token, token);
    });
}

#[test]
fn snapshot_budget_is_global_not_a_per_buffer_count() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab(&"a".repeat(600_000)), tab(&"b".repeat(600_000))];
        let mut capture = request(&tabs);
        capture.version_history = true;
        capture.recovery_policy.snapshot_byte_budget = 1_048_576;
        db.persist(capture, 1).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM buffer_versions")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM snapshots").await.unwrap(),
            2,
            "live recovery cannot expire"
        );
    });
}

#[test]
fn archive_compaction_preserves_earlier_text_before_discarding_undo() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut workspace = tab("earliest");
        insert(&mut workspace, 0, &"x".repeat(20_000));
        let id = workspace.active_buffer().temp_id.clone();
        let closed = CapturedClosedTab::capture(&workspace, None);
        let token = closed.token.clone();
        let mut capture = request(&[]);
        capture.closed_tabs.push(closed);
        db.persist(capture, 1).await.unwrap();
        let mut capture = request(&[]);
        capture.recovery_policy.archive_byte_budget = 25_000;
        db.persist(capture, 2).await.unwrap();
        assert!(
            db.closed_rows()
                .await
                .unwrap()
                .iter()
                .any(|row| row.token == token)
        );
        assert!(db.read_history(&id).await.unwrap().is_empty());
        let point = db
            .version_history(&id)
            .await
            .unwrap()
            .into_iter()
            .find(|point| point.undo_boundary)
            .unwrap();
        assert_eq!(db.read_version(point.id).await.unwrap(), "earliest");
    });
}

#[test]
fn closing_live_content_does_not_count_the_same_snapshot_twice() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab(&"x".repeat(10_000))];
        db.persist(request(&tabs), 1).await.unwrap();
        let mut close = request(&[]);
        close.recovery_policy.archive_byte_budget = 15_000;
        close
            .closed_tabs
            .push(CapturedClosedTab::capture(&tabs[0], None));
        db.persist(close, 2).await.unwrap();
        assert_eq!(db.closed_rows().await.unwrap().len(), 1);
    });
}

#[test]
fn archive_budget_thins_snapshots_before_deleting_the_archive() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab(&"0".repeat(1000))];
        let id = tabs[0].active_buffer().temp_id.clone();
        for generation in 1..=5 {
            tabs[0]
                .active_buffer_mut()
                .replace_text(generation.to_string().repeat(1000));
            let mut capture = request(&tabs);
            capture.version_history = true;
            db.persist(capture, generation).await.unwrap();
            db.conn
                .execute("UPDATE buffer_versions SET created_at=0", ())
                .await
                .unwrap();
        }
        let mut close = request(&[]);
        close
            .closed_tabs
            .push(CapturedClosedTab::capture(&tabs[0], None));
        db.persist(close, 6).await.unwrap();
        let mut cleanup = request(&[]);
        cleanup.recovery_policy.archive_byte_budget = 4000;
        db.persist(cleanup, 7).await.unwrap();
        assert_eq!(db.closed_rows().await.unwrap().len(), 1);
        assert_eq!(db.version_history(&id).await.unwrap().len(), 2);
    });
}

#[test]
fn schema_six_migrates_without_discarding_existing_versions() {
    block_on(async {
        let (root, mut db) = database().await;
        let tabs = vec![tab("migration")];
        let mut capture = request(&tabs);
        capture.version_history = true;
        db.persist(capture, 1).await.unwrap();
        db.conn.execute_batch("DROP INDEX version_source; ALTER TABLE buffer_versions DROP COLUMN source_token; PRAGMA user_version=6;").await.unwrap();
        drop(db);
        let db = Database::open(root.path()).await.unwrap();
        assert_eq!(db.integer("PRAGMA user_version").await.unwrap(), 7);
        let point = db
            .version_history(&tabs[0].active_buffer().temp_id)
            .await
            .unwrap()
            .remove(0);
        assert_eq!(db.read_version(point.id).await.unwrap(), "migration");
    });
}
