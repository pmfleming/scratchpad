//! The session actor is the only chunk writer. Collection starts with stage-4 roots.
use super::{Database, error, invalid};
use crate::app::services::chunk_store::{
    Chunk, ChunkReference, ChunkStoreHandle, Health, Metadata,
};
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};
use turso::params;
pub(super) const SCHEMA:&str="
CREATE TABLE chunks(id INTEGER PRIMARY KEY,digest BLOB NOT NULL UNIQUE CHECK(length(digest)=32),byte_len INTEGER NOT NULL CHECK(byte_len>=0),char_len INTEGER NOT NULL CHECK(char_len>=0),newline_count INTEGER NOT NULL CHECK(newline_count>=0),is_ascii INTEGER NOT NULL CHECK(is_ascii IN (0,1)),ends_with_lf INTEGER NOT NULL CHECK(ends_with_lf IN (0,1))) STRICT;
CREATE TABLE chunk_bodies(chunk_id INTEGER PRIMARY KEY REFERENCES chunks(id),body BLOB NOT NULL) STRICT;
CREATE INDEX chunk_sizes ON chunks(byte_len,id);";
const MAX_CHUNK_BATCH_BYTES: usize = 1024 * 1024;
const COLLECTION_IDLE_DELAY: Duration = Duration::from_secs(5);
// Scan widely but delete narrowly: rows held only by live handles cost one id
// read each, not one worker loop iteration per four rows.
const COLLECTION_SCAN_LIMIT: usize = 128;
const COLLECTION_DELETE_LIMIT: usize = 4;
/// Picks deletable rows from one ascending scan, stopping at the delete limit.
/// Also returns how many scanned rows were examined, so the cursor never moves
/// past a row that was fetched but not considered.
fn collection_candidates(
    unreferenced: &[i64],
    live: &std::collections::HashSet<i64>,
) -> (Vec<i64>, usize) {
    let mut candidates = Vec::with_capacity(COLLECTION_DELETE_LIMIT);
    for (index, id) in unreferenced.iter().enumerate() {
        if !live.contains(id) {
            candidates.push(*id);
            if candidates.len() == COLLECTION_DELETE_LIMIT {
                return (candidates, index + 1);
            }
        }
    }
    (candidates, unreferenced.len())
}
impl Database {
    pub(super) async fn collect_idle_chunk_batch(&mut self, now: Instant) -> io::Result<()> {
        if now < self.collection_after {
            return Ok(());
        }
        if self.collect_chunk_batch().await? {
            // Revisit live-only rows after their handles may have been dropped,
            // even if no further database writes occur.
            self.collection_after = now + COLLECTION_IDLE_DELAY;
        }
        Ok(())
    }
    /// Deletes at most `COLLECTION_DELETE_LIMIT` unreferenced chunks. Returns
    /// true once this batch finished a sweep of every unreferenced row.
    pub(super) async fn collect_chunk_batch(&mut self) -> io::Result<bool> {
        self.ensure_healthy()?;
        let store = self.chunk_store()?;
        let live = store.live_row_ids();
        let unreferenced = self
            .query_collect(
                "SELECT id FROM chunks WHERE id>? AND id NOT IN (SELECT chunk_id FROM document_text) ORDER BY id LIMIT ?",
                params![self.collection_cursor, COLLECTION_SCAN_LIMIT as i64],
                |r| r.get::<i64>(0).map_err(error),
            )
            .await?;
        let (candidates, examined) = collection_candidates(&unreferenced, &live);
        // A short scan that was fully examined reached the end of the table.
        let swept = examined == unreferenced.len() && unreferenced.len() < COLLECTION_SCAN_LIMIT;
        self.collection_cursor = if swept { 0 } else { unreferenced[examined - 1] };
        if candidates.is_empty() {
            return Ok(swept);
        }
        let start = Instant::now();
        let count = candidates.len();
        self.conn
            .execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(error)?;
        let result = async {
            for id in candidates {
                self.execute_cached("DELETE FROM chunk_bodies WHERE chunk_id=?", [id])
                    .await?;
                self.execute_cached("DELETE FROM chunks WHERE id=?", [id])
                    .await?;
            }
            Ok(())
        }
        .await;
        self.finish_transaction(result).await?;
        let changes = self.integer("SELECT total_changes()").await?;
        if let Some(inventory) = &mut self.inventory {
            inventory.connection_changes = Some(changes);
        }
        if start.elapsed() > Duration::from_millis(50) {
            log::warn!(
                "chunk collection I/O scheduling overrun: epoch={} chunks={} elapsed_us={}",
                store.epoch().id,
                count,
                start.elapsed().as_micros()
            );
        }
        Ok(swept)
    }
    pub(in crate::app::services::session_store) fn chunk_store(
        &self,
    ) -> io::Result<ChunkStoreHandle> {
        self.chunks
            .clone()
            .ok_or_else(|| invalid("chunk readers have not opened"))
    }
    pub(in crate::app::services::session_store) async fn drain_chunk_batch(
        &mut self,
    ) -> io::Result<()> {
        let store = self.chunk_store()?;
        self.ensure_healthy()?;
        let Some(first) = store.pop_pending() else {
            return Ok(());
        };
        let start = Instant::now();
        let mut batch_bytes = first.len();
        let mut batch = vec![first];
        let mut references = Vec::new();
        // NORMAL is confined to immutable body commits. Manifest commits remain FULL.
        let result = async {
            self.conn
                .execute("PRAGMA synchronous=NORMAL", ())
                .await
                .map_err(error)?;
            self.conn
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(error)?;
            loop {
                let chunk = &batch[batch.len() - 1];
                references.push(self.write_chunk(chunk, &store).await?);
                if batch_bytes >= MAX_CHUNK_BATCH_BYTES
                    || start.elapsed() >= Duration::from_millis(4)
                {
                    break;
                }
                let Some(next) = store.pop_pending() else {
                    break;
                };
                if batch_bytes + next.len() > MAX_CHUNK_BATCH_BYTES {
                    store.retry(vec![next]);
                    break;
                }
                batch_bytes += next.len();
                batch.push(next);
            }
            self.finish_transaction(Ok(())).await
        }
        .await;
        if let Err(cause) = &result
            && !self.conn.is_autocommit().unwrap_or(false)
        {
            let _ = self.finish_transaction(Err(error(cause.to_string()))).await;
        }
        let full = self
            .conn
            .execute("PRAGMA synchronous=FULL", ())
            .await
            .map_err(error);
        store.record_batch(start.elapsed());
        if let Err(cause) = result {
            if let Err(restore) = &full {
                self.blocked = Some(restore.to_string());
            }
            store.retry(batch);
            let health = if let Some(blocked) = &self.blocked {
                Health::Unavailable(blocked.clone())
            } else {
                Health::Failing(cause.to_string())
            };
            store.set_health(health);
            return Err(cause);
        }
        // COMMIT completed, so handles can safely become evictable even if restoring
        // FULL failed. In that case block all future manifests before returning.
        for (chunk, reference) in batch.iter().zip(references) {
            store.committed(chunk, reference);
        }
        if let Err(cause) = full {
            self.blocked = Some(cause.to_string());
            store.set_health(Health::Unavailable(cause.to_string()));
            return Err(cause);
        }
        store.set_health(Health::Healthy);
        let changes = self.integer("SELECT total_changes()").await?;
        if let Some(inventory) = &mut self.inventory {
            inventory.connection_changes = Some(changes);
        }
        if start.elapsed() > Duration::from_millis(50) {
            log::warn!(
                "chunk batch I/O scheduling overrun: epoch={} chunks={} elapsed_us={}",
                store.epoch().id,
                batch.len(),
                start.elapsed().as_micros()
            );
        }
        Ok(())
    }
    async fn write_chunk(
        &self,
        chunk: &Arc<Chunk>,
        store: &ChunkStoreHandle,
    ) -> io::Result<ChunkReference> {
        if let Some(reference) = chunk.reference() {
            return Ok(reference);
        }
        let body = chunk
            .pending_body()
            .ok_or_else(|| invalid("chunk state changed during a single-writer batch"))?;
        store.record_hashed(body.len());
        let metadata = Metadata::measure(&body);
        let inserted=self.execute_count_cached("INSERT INTO chunks(digest,byte_len,char_len,newline_count,is_ascii,ends_with_lf) VALUES(?,?,?,?,?,?) ON CONFLICT(digest) DO NOTHING",params![metadata.digest.as_slice(),i64::try_from(metadata.byte_len).map_err(invalid)?,super::document_rows::signed(metadata.char_len)?,super::document_rows::signed(metadata.newline_count)?,i64::from(metadata.is_ascii),i64::from(metadata.ends_with_lf)]).await?;
        let row = self
            .query_collect(
                "SELECT id,byte_len,char_len,newline_count,is_ascii,ends_with_lf FROM chunks WHERE digest=?",
                [metadata.digest.as_slice()],
                |r| {
                    Ok((
                        r.get::<i64>(0).map_err(error)?,
                        r.get::<i64>(1).map_err(error)?,
                        r.get::<i64>(2).map_err(error)?,
                        r.get::<i64>(3).map_err(error)?,
                        r.get::<i64>(4).map_err(error)?,
                        r.get::<i64>(5).map_err(error)?,
                    ))
                },
            )
            .await?
            .pop()
            .ok_or_else(|| invalid("missing newly stored chunk"))?;
        if row.1 != i64::try_from(metadata.byte_len).map_err(invalid)?
            || row.2 != super::document_rows::signed(metadata.char_len)?
            || row.3 != super::document_rows::signed(metadata.newline_count)?
            || row.4 != i64::from(metadata.is_ascii)
            || row.5 != i64::from(metadata.ends_with_lf)
        {
            return Err(invalid("chunk digest collision or damaged metadata"));
        }
        if inserted == 0 {
            self.verify_deduplicated_body(row.0, body.as_bytes())
                .await?;
        } else {
            self.execute_cached(
                "INSERT INTO chunk_bodies(chunk_id,body) VALUES(?,?)",
                params![row.0, body.as_bytes()],
            )
            .await?;
        }
        Ok(ChunkReference {
            id: row.0,
            metadata,
            epoch: store.epoch(),
        })
    }
    async fn verify_deduplicated_body(&self, id: i64, expected: &[u8]) -> io::Result<()> {
        let body = self
            .query_collect(
                "SELECT body FROM chunk_bodies WHERE chunk_id=?",
                [id],
                |r| r.get::<Vec<u8>>(0).map_err(error),
            )
            .await?
            .pop()
            .ok_or_else(|| invalid("deduplicated chunk body is missing"))?;
        if body != expected {
            return Err(invalid(
                "deduplicated chunk body is corrupt; accepted text retained in RAM",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DatabaseWorker, ReadScope};
    use super::*;
    use futures_lite::future::block_on;

    #[test]
    fn bounded_scan_cache_keeps_viewport_separate_and_eviction_preserves_pins() {
        block_on(async {
            let (_root, mut db) = super::super::test_fixture::database().await;
            let store = db.chunk_store().unwrap();
            store.set_limits(0, 0);
            store.set_scan_limits(8192, 8192);
            let visible = store.seal(Arc::new("a".repeat(4096)), 1);
            let first = store.seal(Arc::new("λ".repeat(2048)), 2);
            let second = store.seal(Arc::new("b".repeat(4096)), 2);
            while store.pending_bytes() > 0 {
                db.drain_chunk_batch().await.unwrap();
            }
            store.set_limits(4096, 4096);
            drop(visible.load(false, true).unwrap());
            drop(first.load(true, false).unwrap());
            let pinned = second.load(true, false).unwrap();
            assert_eq!(store.cached_bytes(), 4096);
            assert_eq!(store.scan_observation().0, 8192);
            let reads = store.epoch().read_count();
            drop(first.load(true, false).unwrap());
            assert_eq!(store.epoch().read_count(), reads);
            store.set_scan_limits(0, 0);
            assert_eq!(store.scan_observation().0, 0);
            assert_eq!(pinned.as_str(), "b".repeat(4096));
            drop(visible.load(false, true).unwrap());
            assert_eq!(store.epoch().read_count(), reads);
            drop(first.load(true, false).unwrap());
            assert_eq!(store.epoch().read_count(), reads + 1);
            assert_eq!(store.cached_bytes(), 4096);
        });
    }

    #[test]
    fn mixed_chunk_batches_preserve_fifo_and_bound_committed_bytes() {
        block_on(async {
            let (_root, mut db) = super::super::test_fixture::database().await;
            let store = db.chunk_store().unwrap();
            let chunks: Vec<_> = (0..20)
                .map(|index| {
                    let size = if index % 3 == 0 { 256 * 1024 } else { 3 * 1024 };
                    store.seal(Arc::new(format!("{index:04}").repeat(size / 4)), index)
                })
                .collect();
            while store.pending_bytes() > 0 {
                let before = store.pending_bytes();
                db.drain_chunk_batch().await.unwrap();
                let committed = before - store.pending_bytes();
                assert!(committed > 0 && committed <= MAX_CHUNK_BATCH_BYTES);
                let first_pending = chunks
                    .iter()
                    .position(|chunk| chunk.reference().is_none())
                    .unwrap_or(chunks.len());
                assert!(
                    chunks[first_pending..]
                        .iter()
                        .all(|chunk| chunk.reference().is_none())
                );
            }
            for (index, chunk) in chunks.iter().enumerate() {
                assert!(
                    chunk
                        .load(false, true)
                        .unwrap()
                        .starts_with(&format!("{index:04}"))
                );
            }
        });
    }

    #[test]
    fn deduplication_never_releases_accepted_ram_against_a_corrupt_body() {
        block_on(async {
            let (_root, mut db) = super::super::test_fixture::database().await;
            let store = db.chunk_store().unwrap();
            store.set_limits(0, 0);
            let text = "accepted λ🦀".repeat(4096);
            let first = store.seal(Arc::new(text.clone()), 1);
            db.drain_chunk_batch().await.unwrap();
            let id = first.reference().unwrap().id;
            db.execute_cached("UPDATE chunk_bodies SET body=x'00' WHERE chunk_id=?", [id])
                .await
                .unwrap();
            let duplicate = store.seal(Arc::new(text.clone()), 2);
            assert!(db.drain_chunk_batch().await.is_err());
            assert!(duplicate.reference().is_none());
            assert_eq!(duplicate.pending_body().unwrap().as_str(), text);
            assert_eq!(duplicate.load(false, true).unwrap().as_str(), text);
            assert!(matches!(store.health(), Health::Failing(_)));
            db.execute_cached(
                "UPDATE chunk_bodies SET body=? WHERE chunk_id=?",
                params![text.as_bytes(), id],
            )
            .await
            .unwrap();
            db.drain_chunk_batch().await.unwrap();
            assert_eq!(duplicate.reference().unwrap().id, id);
            assert_eq!(store.pending_bytes(), 0);
        });
    }

    #[test]
    fn pending_committed_evicted_and_pinned_chunks_keep_exact_text() {
        let root = tempfile::tempdir().unwrap();
        let worker = DatabaseWorker::open(root.path().into()).unwrap();
        let store = worker.chunk_store();
        store.set_limits(0, 0);
        let text = "λ\r\n🦀".repeat(4096);
        let chunk = store.seal(Arc::new(text.clone()), 1);
        let pin = chunk.load(false, true).unwrap();
        assert_eq!(*pin, text);
        worker
            .call(|db| {
                block_on(async {
                    while db.chunk_store()?.pending_bytes() > 0 {
                        db.drain_chunk_batch().await?;
                    }
                    Ok(())
                })
            })
            .unwrap();
        assert_eq!(store.pending_bytes(), 0);
        assert_eq!(chunk.len(), text.len());
        assert!(!chunk.is_empty());
        assert_eq!(*chunk.load(true, false).unwrap(), text);
        let reference = chunk.reference().unwrap();
        let restored = store.restored(reference.clone(), 2);
        assert_eq!(*restored.load(true, false).unwrap(), text);
        let id = reference.id;
        worker
            .call(move |db| {
                block_on(async {
                    db.execute_cached("UPDATE chunk_bodies SET body=x'00' WHERE chunk_id=?", [id])
                        .await
                })
            })
            .unwrap();
        assert_eq!(
            *chunk.load(false, true).unwrap(),
            text,
            "pin survives eviction and backing damage"
        );
        drop(pin);
        assert!(chunk.load(false, true).is_err());
        // A crash before the first manifest can leave committed body rows only.
        assert!(
            worker
                .call(|db| block_on(db.load(ReadScope::All)))
                .unwrap()
                .is_none()
        );
        drop(restored);
        drop(chunk);
        drop(reference);
        drop(store);
        drop(worker);
        let worker = DatabaseWorker::open(root.path().into()).unwrap();
        assert!(
            worker
                .call(|db| block_on(db.load(ReadScope::Shells)))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn failed_chunk_transaction_retains_ram_restores_full_and_retries_idempotently() {
        let root = tempfile::tempdir().unwrap();
        let worker = DatabaseWorker::open(root.path().into()).unwrap();
        worker.call(|db|block_on(db.execute_cached("CREATE TRIGGER fail_body BEFORE INSERT ON chunk_bodies BEGIN SELECT RAISE(ABORT,'injected full disk'); END",()))).unwrap();
        let store = worker.chunk_store();
        let chunk = store.seal(Arc::new("accepted λ".repeat(4096)), 1);
        assert!(worker.call(|db| block_on(db.drain_chunk_batch())).is_err());
        assert!(chunk.reference().is_none());
        assert_eq!(
            chunk.pending_body().unwrap().as_str(),
            "accepted λ".repeat(4096)
        );
        assert!(matches!(store.health(), Health::Failing(_)));
        worker
            .call(|db| {
                block_on(async {
                    assert_eq!(db.integer("PRAGMA synchronous").await?, 2);
                    assert_eq!(db.integer("SELECT count(*) FROM chunks").await?, 0);
                    db.execute_cached("DROP TRIGGER fail_body", ()).await?;
                    db.drain_chunk_batch().await
                })
            })
            .unwrap();
        assert!(matches!(store.health(), Health::Healthy));
        assert_eq!(store.pending_bytes(), 0);
        let duplicate = store.seal(Arc::new("accepted λ".repeat(4096)), 2);
        worker.call(|db| block_on(db.drain_chunk_batch())).unwrap();
        assert_eq!(
            chunk.reference().unwrap().id,
            duplicate.reference().unwrap().id
        );
        store.wait_for_capacity().unwrap();
    }
    #[test]
    fn reused_reader_statement_recovers_from_missing_rows_and_rebinds() {
        let root = tempfile::tempdir().unwrap();
        let worker = DatabaseWorker::open(root.path().into()).unwrap();
        let store = worker.chunk_store();
        store.set_limits(0, 0);
        let first = store.seal(Arc::new("first λ".into()), 1);
        let second = store.seal(Arc::new("second 東京".into()), 2);
        worker.call(|db| block_on(db.drain_chunk_batch())).unwrap();
        let mut missing = first.reference().unwrap();
        missing.id = i64::MAX;
        assert!(store.restored(missing, 3).load(false, false).is_err());
        for (chunk, expected) in [
            (&first, "first λ"),
            (&second, "second 東京"),
            (&first, "first λ"),
        ] {
            assert_eq!(
                *store
                    .restored(chunk.reference().unwrap(), 4)
                    .load(false, false)
                    .unwrap(),
                expected
            );
        }
        let before = store.epoch().foreground_physical_read_bytes();
        assert_eq!(
            *store
                .restored(second.reference().unwrap(), 5)
                .load(true, false)
                .unwrap(),
            "second 東京"
        );
        assert_eq!(
            store.epoch().foreground_physical_read_bytes(),
            before,
            "background I/O must not alter the foreground reader's counters"
        );
    }

    #[test]
    fn a_reader_panic_is_an_error_and_background_reads_still_work() {
        let root = tempfile::tempdir().unwrap();
        let worker = DatabaseWorker::open(root.path().into()).unwrap();
        let store = worker.chunk_store();
        store.set_limits(0, 0);
        let chunk = store.seal(Arc::new("exact Unicode λ".into()), 1);
        worker.call(|db| block_on(db.drain_chunk_batch())).unwrap();
        let cold = store.restored(chunk.reference().unwrap(), 2);
        store.epoch().panic_foreground_for_test();
        assert!(cold.load(false, false).is_err());
        assert_eq!(*cold.load(true, false).unwrap(), "exact Unicode λ");
        assert!(!worker.writes_blocked());
    }
}
