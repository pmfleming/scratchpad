use super::{
    Database,
    tests::{database, insert, request, tab},
};
use futures_lite::future::block_on;

async fn assert_sizes(db: &Database) {
    assert_eq!(
        db.integer("SELECT count(*) FROM history WHERE bytes != length(entry)")
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        db.integer("SELECT count(*) FROM blobs WHERE bytes != length(body)")
            .await
            .unwrap(),
        0
    );
    let usage = db.stored_usage().await.unwrap();
    assert_eq!(
        usage.undo,
        db.integer("SELECT coalesce(sum(length(entry)),0) FROM history")
            .await
            .unwrap() as u64
    );
    assert_eq!(
        usage.live_text + usage.recovery_text + usage.shared_text + usage.unreferenced_text,
        db.integer("SELECT coalesce(sum(length(body)),0) FROM blobs")
            .await
            .unwrap() as u64
    );
}

#[test]
fn sizes_track_writes_coalescing_undo_and_shared_recovery_chunks() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut live = vec![tab("base")];
        for generation in 1..=3 {
            insert(&mut live[0], 0, &"λ".repeat(generation as usize * 10_000));
            let mut save = request(&live);
            save.version_history = true;
            db.persist(save, generation).await.unwrap();
            assert_sizes(&db).await;
        }
        live[0]
            .active_buffer_mut()
            .document_mut()
            .undo_last_operation()
            .unwrap();
        db.persist(request(&live), 4).await.unwrap();
        assert_sizes(&db).await;
        let mut close = request(&[]);
        close
            .closed_tabs
            .push(crate::app::services::session_store::CapturedClosedTab::capture(&live[0], None));
        db.persist(close, 5).await.unwrap();
        assert_sizes(&db).await;
    });
}

#[test]
fn schema_seven_backfills_byte_counts_and_preserves_text_history_and_versions() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut live = vec![tab("migration")];
        insert(&mut live[0], 0, &"λ".repeat(20_000));
        let id = live[0].active_buffer().temp_id.clone();
        let mut save = request(&live);
        save.version_history = true;
        db.persist(save, 1).await.unwrap();
        db.conn
            .execute_batch(
                "DROP INDEX history_sizes; DROP INDEX blob_sizes;
            ALTER TABLE history DROP COLUMN bytes; ALTER TABLE blobs DROP COLUMN bytes;
            PRAGMA user_version=7;",
            )
            .await
            .unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert_eq!(db.integer("PRAGMA user_version").await.unwrap(), 8);
        assert_sizes(&db).await;
        assert_eq!(db.read_history(&id).await.unwrap().len(), 1);
        let points = db.version_history(&id).await.unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(
            db.read_version(points[0].id).await.unwrap(),
            live[0].active_buffer().text()
        );
        db.persist(request(&live), 1).await.unwrap();
        assert_sizes(&db).await;
    });
}

#[test]
fn failed_size_backfill_rolls_back_the_whole_schema_upgrade() {
    block_on(async {
        let (root, mut db) = database().await;
        db.persist(request(&[tab("retained")]), 1).await.unwrap();
        db.conn.execute_batch("DROP INDEX history_sizes; DROP INDEX blob_sizes;
            ALTER TABLE history DROP COLUMN bytes; ALTER TABLE blobs DROP COLUMN bytes;
            PRAGMA user_version=7;
            CREATE TRIGGER fail_backfill BEFORE UPDATE ON blobs BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        drop(db);
        assert!(Database::open(root.path()).await.is_err());
        let raw = turso::Builder::new_local(root.path().join("session.db").to_str().unwrap())
            .build()
            .await
            .unwrap();
        let conn = raw.connect().unwrap();
        let version = conn
            .query("PRAGMA user_version", ())
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<i64>(0)
            .unwrap();
        assert_eq!(version, 7);
        assert!(conn.query("SELECT bytes FROM blobs", ()).await.is_err());
        conn.execute("DROP TRIGGER fail_backfill", ())
            .await
            .unwrap();
        drop(conn);
        drop(raw);
        let db = Database::open(root.path()).await.unwrap();
        assert_sizes(&db).await;
    });
}

#[test]
fn accounting_queries_use_covering_indexes_not_payload_rows() {
    block_on(async {
        let (_root, db) = database().await;
        for (sql, index) in [
            (
                "SELECT coalesce(sum(bytes),0) FROM history INDEXED BY history_sizes WHERE temp_id='test'",
                "history_sizes",
            ),
            (
                "SELECT coalesce(sum(bytes),0) FROM history INDEXED BY history_sizes",
                "history_sizes",
            ),
            (
                "SELECT bytes, EXISTS(SELECT 1 FROM parts WHERE parts.digest=blobs.digest), EXISTS(SELECT 1 FROM version_parts WHERE version_parts.digest=blobs.digest) FROM blobs INDEXED BY blob_sizes",
                "blob_sizes",
            ),
        ] {
            let plan = db
                .query_collect(&format!("EXPLAIN QUERY PLAN {sql}"), (), |row| {
                    row.get::<String>(3).map_err(super::error)
                })
                .await
                .unwrap()
                .join("\n");
            assert!(plan.contains(&format!("COVERING INDEX {index}")), "{plan}");
        }
    });
}

#[test]
#[ignore = "manual accounting scan profile; allocates a 64 MiB fixture"]
fn profile_payload_lengths_against_covering_byte_counts() {
    block_on(async {
        let (_root, db) = database().await;
        db.conn.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        let body = vec![b'x'; 512 * 1024];
        for id in 0..128u64 {
            db.conn
                .execute(
                    "INSERT INTO blobs(digest,body,bytes) VALUES(?,?,?)",
                    turso::params![
                        id.to_le_bytes().as_slice(),
                        body.as_slice(),
                        body.len() as i64
                    ],
                )
                .await
                .unwrap();
        }
        db.conn.execute("COMMIT", ()).await.unwrap();
        for _ in 0..3 {
            for sql in [
                "SELECT sum(length(body)) FROM blobs",
                "SELECT sum(bytes) FROM blobs INDEXED BY blob_sizes",
            ] {
                let start = std::time::Instant::now();
                assert_eq!(db.integer(sql).await.unwrap(), 64 << 20);
                eprintln!("{sql}: {:?}", start.elapsed());
            }
        }
    });
}
