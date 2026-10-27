use super::super::parallel;
use super::{Database, error, invalid, quarantine::damaged};
use crate::app::domain::DocumentSnapshot;
use crate::app::services::session_store::model::SessionTab;
use futures_lite::future::block_on;
use std::collections::{HashMap, HashSet};
use std::io;
use turso::params;

use super::chunking::{MAX_BYTES as CHUNK_BYTES, chunks};

struct Content {
    parts: Vec<(i64, Vec<u8>)>,
    observed: usize,
    length: usize,
    digest: Vec<u8>,
}
impl Content {
    fn from_row(row: &turso::Row) -> io::Result<Self> {
        Ok(Self {
            parts: Vec::new(),
            observed: 0,
            length: usize::try_from(row.get::<i64>(1).map_err(error)?).map_err(invalid)?,
            digest: row.get(2).map_err(error)?,
        })
    }
    fn append(&mut self, part: i64, bytes: Vec<u8>) -> io::Result<()> {
        if part < 0
            || bytes.is_empty()
            || bytes.len() > CHUNK_BYTES
            || bytes.len() > self.length.saturating_sub(self.observed)
        {
            return Err(invalid("invalid recovery content chunk"));
        }
        self.observed += bytes.len();
        self.parts.push((part, bytes));
        Ok(())
    }
    fn append_row(&mut self, row: &turso::Row, part_column: usize) -> io::Result<()> {
        self.append(
            row.get(part_column).map_err(error)?,
            row.get(part_column + 1).map_err(error)?,
        )
    }
    fn finish(mut self) -> io::Result<String> {
        // Sort only each buffer's small chunk list, not the entire SQL result.
        self.parts.sort_unstable_by_key(|(part, _)| *part);
        let mut bytes = Vec::new();
        for (index, (part, body)) in self.parts.into_iter().enumerate() {
            if part != index as i64 {
                return Err(invalid("missing or duplicate recovery chunk"));
            }
            if index == 0 {
                bytes = body;
            } else {
                bytes.extend(body);
            }
        }
        if bytes.len() != self.length || blake3::hash(&bytes).as_bytes() != self.digest.as_slice() {
            return Err(invalid("recovery content checksum or length mismatch"));
        }
        String::from_utf8(bytes).map_err(invalid)
    }
}

struct ContentBatch {
    contents: HashMap<String, String>,
    chunk_count: usize,
}
async fn read_content_range(
    conn: &turso::Connection,
    metadata: Vec<(String, Content)>,
) -> io::Result<ContentBatch> {
    let mut batch = ContentBatch {
        contents: HashMap::new(),
        chunk_count: 0,
    };
    let Some((first, _)) = metadata.first() else {
        return Ok(batch);
    };
    let last = &metadata
        .last()
        .ok_or_else(|| invalid("missing content range"))?
        .0;
    let mut rows = conn
        .query(
            "SELECT p.temp_id, p.part, b.body FROM parts p LEFT JOIN blobs b ON b.digest=p.digest WHERE p.temp_id>=? AND p.temp_id<=?",
            params![first.as_str(), last.as_str()],
        )
        .await
        .map_err(error)?;
    let mut contents: HashMap<_, _> = metadata.into_iter().collect();
    while let Some(row) = rows.next().await.map_err(error)? {
        let id: String = row.get(0).map_err(error)?;
        let content = contents
            .get_mut(&id)
            .ok_or_else(|| invalid("orphan recovery chunk"))?;
        content
            .append_row(&row, 1)
            .map_err(|cause| damaged(&id, cause))?;
        batch.chunk_count += 1;
    }
    batch.contents = contents
        .into_iter()
        .map(|(id, content)| {
            let text = content.finish().map_err(|cause| damaged(&id, cause))?;
            Ok((id, text))
        })
        .collect::<io::Result<_>>()?;
    Ok(batch)
}

impl Database {
    pub(super) async fn write_content(
        &self,
        id: &str,
        snapshot: &DocumentSnapshot,
        removed_digests: &mut HashSet<Vec<u8>>,
    ) -> io::Result<()> {
        // Establish the FK parent before inserting parts. Placeholder metadata is
        // never visible outside this transaction, including on a failed save.
        self.execute_cached(
            "INSERT INTO snapshots VALUES(?, 0, ?) ON CONFLICT(id) DO NOTHING",
            params![id, blake3::hash(b"").as_bytes().as_slice()],
        )
        .await?;
        let (bytes, digest) = self.write_chunks(id, snapshot, removed_digests).await?;
        self.execute_cached(
            "INSERT INTO snapshots VALUES(?, ?, ?) ON CONFLICT(id) DO UPDATE SET bytes=excluded.bytes, digest=excluded.digest",
            params![id, bytes, digest.as_bytes().as_slice()],
        ).await?;
        Ok(())
    }

    async fn write_chunks(
        &self,
        id: &str,
        snapshot: &DocumentSnapshot,
        removed_digests: &mut HashSet<Vec<u8>>,
    ) -> io::Result<(i64, blake3::Hash)> {
        let mut rows = self
            .conn
            .query("SELECT part, digest FROM parts WHERE temp_id=?", [id])
            .await
            .map_err(error)?;
        let mut old = HashMap::<i64, Vec<u8>>::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            old.insert(row.get(0).map_err(error)?, row.get(1).map_err(error)?);
        }
        drop(rows);
        let known: std::collections::HashSet<_> = old.values().map(Vec::as_slice).collect();
        let mut digest = blake3::Hasher::new();
        let mut bytes = 0i64;
        let mut count = 0i64;
        for chunk in chunks(snapshot) {
            let chunk = chunk.map_err(error)?;
            bytes = bytes
                .checked_add(chunk.data.len() as i64)
                .ok_or_else(|| invalid("recovery text exceeds database length range"))?;
            digest.update(&chunk.data);
            let hash = blake3::hash(&chunk.data);
            // The SELECT prevents inserts (and trigger work) for shared bodies.
            if !known.contains(hash.as_bytes().as_slice()) {
                self.execute_cached("INSERT INTO blobs(digest, body) SELECT ?, ? WHERE NOT EXISTS(SELECT 1 FROM blobs WHERE digest=?)",
                    params![hash.as_bytes().as_slice(), chunk.data, hash.as_bytes().as_slice()]).await?;
            }
            if old.get(&count).map(Vec::as_slice) != Some(hash.as_bytes().as_slice()) {
                if let Some(digest) = old.get(&count) {
                    removed_digests.insert(digest.clone());
                }
                self.execute_cached("INSERT INTO parts VALUES(?, ?, ?) ON CONFLICT(temp_id, part) DO UPDATE SET digest=excluded.digest",
                    params![id, count, hash.as_bytes().as_slice()]).await?;
            }
            count += 1;
        }
        removed_digests.extend(
            old.iter()
                .filter(|(part, _)| **part >= count)
                .map(|(_, digest)| digest.clone()),
        );
        self.execute_cached(
            "DELETE FROM parts WHERE temp_id=? AND part>=?",
            params![id, count],
        )
        .await?;
        Ok((bytes, digest.finalize()))
    }

    pub(super) async fn read_all_contents(&self) -> io::Result<HashMap<String, String>> {
        if let Some(id) = self.quarantined.keys().next() {
            return Err(damaged(id, "original recovery data is protected"));
        }
        let mut metadata = Vec::new();
        let mut rows = self
            .conn
            .query("SELECT id, bytes, digest FROM snapshots ORDER BY id", ())
            .await
            .map_err(error)?;
        while let Some(row) = rows.next().await.map_err(error)? {
            let id: String = row.get(0).map_err(error)?;
            let content = Content::from_row(&row).map_err(|cause| damaged(&id, cause))?;
            metadata.push((id, content));
        }
        drop(rows);
        let workers = parallel::worker_count(8);
        let batches = if metadata.len() < 512 || workers == 1 {
            vec![read_content_range(&self.conn, metadata).await?]
        } else {
            self.read_content_parallel(metadata, workers)?
        };
        let chunk_count: usize = batches.iter().map(|batch| batch.chunk_count).sum();
        if chunk_count as i64 != self.integer("SELECT count(*) FROM parts").await? {
            return Err(invalid("orphan recovery chunks"));
        }
        Ok(batches
            .into_iter()
            .flat_map(|batch| batch.contents)
            .collect())
    }

    fn read_content_parallel(
        &self,
        metadata: Vec<(String, Content)>,
        workers: usize,
    ) -> io::Result<Vec<ContentBatch>> {
        let batch_size = metadata.len().div_ceil(workers);
        let mut metadata = metadata.into_iter();
        let jobs = (0..workers).map(|_| metadata.by_ref().take(batch_size).collect());
        parallel::ordered("session-read", jobs, |batch| {
            let conn = self.database.connect().map_err(error)?;
            block_on(read_content_range(&conn, batch))
        })
    }

    pub(super) async fn read_tab_contents(
        &self,
        tabs: &[SessionTab],
    ) -> io::Result<HashMap<String, String>> {
        let mut contents = HashMap::new();
        let mut metadata = self
            .conn
            .prepare_cached("SELECT id, bytes, digest FROM snapshots WHERE id=?")
            .await
            .map_err(error)?;
        let mut chunks = self
            .conn
            .prepare_cached("SELECT p.part, b.body FROM parts p LEFT JOIN blobs b ON b.digest=p.digest WHERE p.temp_id=? ORDER BY p.part")
            .await
            .map_err(error)?;
        for buffer in tabs.iter().flat_map(|tab| &tab.buffers) {
            if self.quarantined.contains_key(&buffer.temp_id) {
                return Err(damaged(
                    &buffer.temp_id,
                    "original recovery data is protected",
                ));
            }
            let mut rows = metadata
                .query([buffer.temp_id.as_str()])
                .await
                .map_err(error)?;
            let Some(row) = rows.next().await.map_err(error)? else {
                if super::super::validation::needs_snapshot(buffer) {
                    return Err(damaged(
                        &buffer.temp_id,
                        "required recovery content is missing",
                    ));
                }
                continue;
            };
            let mut content =
                Content::from_row(&row).map_err(|cause| damaged(&buffer.temp_id, cause))?;
            drop(rows);
            let mut rows = chunks
                .query([buffer.temp_id.as_str()])
                .await
                .map_err(error)?;
            while let Some(row) = rows.next().await.map_err(error)? {
                content
                    .append_row(&row, 0)
                    .map_err(|cause| damaged(&buffer.temp_id, cause))?;
            }
            let text = content
                .finish()
                .map_err(|cause| damaged(&buffer.temp_id, cause))?;
            contents.insert(buffer.temp_id.clone(), text);
        }
        Ok(contents)
    }
}
