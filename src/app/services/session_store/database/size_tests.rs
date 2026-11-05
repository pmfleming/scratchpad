use super::{
    Database,
    tests::{database, insert, tab},
};
use crate::app::services::session_store::{CapturedClosedTab, SessionPersistRequest};
use futures_lite::future::block_on;
fn request(tabs: &[crate::app::domain::WorkspaceTab]) -> SessionPersistRequest {
    SessionPersistRequest::capture(tabs, 0, 14.0, false)
}
async fn assert_sizes(db: &Database) {
    assert_eq!(db.integer("SELECT count(*) FROM chunks c JOIN chunk_bodies b ON b.chunk_id=c.id WHERE c.byte_len!=length(b.body)").await.unwrap(),0);
    let usage = db.stored_usage().await.unwrap();
    assert_eq!(
        usage.undo,
        db.integer("SELECT coalesce(sum(bytes),0) FROM history INDEXED BY history_sizes")
            .await
            .unwrap() as u64
    );
    assert_eq!(usage.live_text+usage.unreferenced_text,db.integer("SELECT (SELECT coalesce(sum(byte_len),0) FROM chunks INDEXED BY chunk_sizes)+(SELECT coalesce(sum(length(body)),0) FROM document_tail)").await.unwrap() as u64);
}
#[test]
fn sizes_track_writes_coalescing_undo_and_shared_recovery_chunks() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut live = vec![tab("base")];
        for generation in 1..=3 {
            insert(&mut live[0], 0, &"λ".repeat(generation as usize * 10_000));
            db.persist(request(&live), generation).await.unwrap();
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
            .push(CapturedClosedTab::capture(&live[0], None));
        db.persist(close, 5).await.unwrap();
        assert_sizes(&db).await;
    });
}
#[test]
fn bulk_usage_matches_reference_accounting_for_archives_and_all_chunk_classes() {
    block_on(async {
        let (_root, mut db) = database().await;
        let live = vec![tab("live"), tab("archived")];
        db.persist(request(&live), 1).await.unwrap();
        let mut close = request(&live[..1]);
        close
            .closed_tabs
            .push(CapturedClosedTab::capture(&live[1], None));
        db.persist(close, 2).await.unwrap();
        let chunk = db
            .chunk_store()
            .unwrap()
            .seal(std::sync::Arc::new("ab".into()), 1);
        db.drain_chunk_batch().await.unwrap();
        drop(chunk);
        let usage = db.stored_usage().await.unwrap();
        assert_eq!(usage.archives, 1);
        assert_eq!(usage.unreferenced_text, 2);
        let archive = db.closed_rows().await.unwrap().remove(0);
        let id = &archive.tab.buffers[0].temp_id;
        let referenced=db.query_collect("SELECT (SELECT bytes FROM document_pieces WHERE id=?1)+(SELECT coalesce(sum(bytes),0) FROM history INDEXED BY history_sizes WHERE temp_id=?1)",[id.as_str()],|r|r.get::<i64>(0).map_err(super::error)).await.unwrap()[0];
        assert_eq!(
            usage.archive_referenced_bytes,
            archive.layout.estimated_bytes() as u64 + referenced as u64
        );
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
                "SELECT byte_len,EXISTS(SELECT 1 FROM document_text WHERE document_text.chunk_id=chunks.id) FROM chunks INDEXED BY chunk_sizes",
                "chunk_sizes",
            ),
        ] {
            let plan = db
                .query_collect(&format!("EXPLAIN QUERY PLAN {sql}"), (), |r| {
                    r.get::<String>(3).map_err(super::error)
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
        let (_root, mut db) = database().await;
        let tabs = vec![tab(&"abcdefgh".repeat(8 * 1024 * 1024))];
        db.persist(request(&tabs), 1).await.unwrap();
        let start = std::time::Instant::now();
        let usage = db.stored_usage().await.unwrap();
        eprintln!(
            "covering accounting: {:?}, logical bytes={}",
            start.elapsed(),
            usage.live_text
        );
        assert_sizes(&db).await;
    });
}
