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
CREATE TABLE chunks(id INTEGER PRIMARY KEY,digest BLOB NOT NULL UNIQUE CHECK(length(digest)=32),byte_len INTEGER NOT NULL CHECK(byte_len>=0),char_len INTEGER NOT NULL CHECK(char_len>=0),newline_count INTEGER NOT NULL CHECK(newline_count>=0),is_ascii INTEGER NOT NULL CHECK(is_ascii IN (0,1))) STRICT;
CREATE TABLE chunk_bodies(chunk_id INTEGER PRIMARY KEY REFERENCES chunks(id),body BLOB NOT NULL) STRICT;
CREATE INDEX chunk_sizes ON chunks(byte_len,id);";
impl Database {
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
                if batch.len() >= 4 || start.elapsed() >= Duration::from_millis(4) {
                    break;
                }
                let Some(next) = store.pop_pending() else {
                    break;
                };
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
        let metadata = Metadata::measure(&body);
        self.execute_cached("INSERT INTO chunks(digest,byte_len,char_len,newline_count,is_ascii) VALUES(?,?,?,?,?) ON CONFLICT(digest) DO NOTHING",params![metadata.digest.as_slice(),i64::try_from(metadata.byte_len).map_err(invalid)?,super::document_rows::signed(metadata.char_len)?,super::document_rows::signed(metadata.newline_count)?,i64::from(metadata.is_ascii)]).await?;
        let row = self
            .query_collect(
                "SELECT id,byte_len,char_len,newline_count,is_ascii FROM chunks WHERE digest=?",
                [metadata.digest.as_slice()],
                |r| {
                    Ok((
                        r.get::<i64>(0).map_err(error)?,
                        r.get::<i64>(1).map_err(error)?,
                        r.get::<i64>(2).map_err(error)?,
                        r.get::<i64>(3).map_err(error)?,
                        r.get::<i64>(4).map_err(error)?,
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
        {
            return Err(invalid("chunk digest collision or damaged metadata"));
        }
        self.execute_cached(
            "INSERT INTO chunk_bodies(chunk_id,body) VALUES(?,?) ON CONFLICT(chunk_id) DO NOTHING",
            params![row.0, body.as_bytes()],
        )
        .await?;
        Ok(ChunkReference {
            id: row.0,
            metadata,
            epoch: store.epoch(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DatabaseWorker, ReadScope};
    use super::*;
    use futures_lite::future::block_on;

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
