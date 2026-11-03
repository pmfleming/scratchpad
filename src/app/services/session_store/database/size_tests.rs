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
        usage.live_text + usage.unreferenced_text,
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
            let save = request(&live);
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
fn bulk_usage_matches_reference_accounting_for_archives_and_all_blob_classes() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut live = vec![tab("recovery λ"), tab("live only"), tab("shared unchanged")];
        let save = request(&[live[0].clone(), live[2].clone()]);
        db.persist(save, 1).await.unwrap();
        live[0].active_buffer_mut().replace_text("new live".into());
        db.persist(request(&live), 2).await.unwrap();
        let mut close = request(&live[1..]);
        close
            .closed_tabs
            .push(crate::app::services::session_store::CapturedClosedTab::capture(&live[0], None));
        db.persist(close, 3).await.unwrap();
        db.conn
            .execute(
                "INSERT INTO blobs(digest,body,bytes) VALUES(x'01',x'6162',2)",
                (),
            )
            .await
            .unwrap();
        let usage = db.stored_usage().await.unwrap();
        let archives = db.closed_rows().await.unwrap();
        let mut referenced = 0;
        for archive in &archives {
            referenced += archive.payload.len() as u64;
            for buffer in &archive.tab.buffers {
                referenced += db.query_collect(
                    "SELECT coalesce((SELECT bytes FROM snapshots WHERE id=?1),0) + coalesce((SELECT sum(bytes) FROM history WHERE temp_id=?1),0)",
                    [buffer.temp_id.as_str()], |row| row.get::<i64>(0).map_err(super::error),
                ).await.unwrap()[0] as u64;
            }
        }
        assert_eq!(usage.archives, archives.len());
        assert_eq!(usage.archive_referenced_bytes, referenced);
        let mut classes = [0u64; 4];
        for (bytes, kind) in db.query_collect(
            "SELECT bytes, EXISTS(SELECT 1 FROM parts WHERE parts.digest=blobs.digest) FROM blobs",
            (), |row| Ok((row.get::<i64>(0).map_err(super::error)?, row.get::<i64>(1).map_err(super::error)?)),
        ).await.unwrap() {
            classes[kind as usize] += bytes as u64;
        }
        assert_eq!([usage.unreferenced_text, usage.live_text, 0, 0], classes);
        assert_eq!(usage.unreferenced_text, 2);
        assert!(classes[1] > 0);
        assert_eq!(classes[2..], [0, 0]);
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
                "SELECT bytes, EXISTS(SELECT 1 FROM parts WHERE parts.digest=blobs.digest) FROM blobs INDEXED BY blob_sizes",
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
