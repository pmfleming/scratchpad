use super::super::parallel;
use super::{Database, error, invalid, quarantine::damaged};
use crate::app::domain::DocumentSnapshot;
use crate::app::services::session_store::model::SessionTab;
use futures_lite::future::block_on;
use std::collections::HashMap;
use std::io;
use turso::params;

const CHUNK_BYTES: usize = 256 * 1024;

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
            "SELECT id, part, body FROM chunks WHERE id>=? AND id<=?",
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
    ) -> io::Result<()> {
        self.execute_cached("DELETE FROM chunks WHERE id=?", [id])
            .await?;
        let (bytes, digest) = self.write_chunks(id, snapshot).await?;
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
    ) -> io::Result<(i64, blake3::Hash)> {
        let mut insert = self
            .conn
            .prepare_cached("INSERT INTO chunks VALUES(?, ?, ?)")
            .await
            .map_err(error)?;
        let mut digest = blake3::Hasher::new();
        let mut bytes = 0i64;
        // Bounded extraction also coalesces fragmented pieces into a small number
        // of rows. Four UTF-8 bytes per scalar is the worst case.
        let chunk_chars = CHUNK_BYTES / 4;
        for (part, start) in (0..snapshot.len_chars()).step_by(chunk_chars).enumerate() {
            let end = (start + chunk_chars).min(snapshot.len_chars());
            let text = snapshot.extract_range(start..end);
            if text.is_empty() || text.len() > CHUNK_BYTES {
                return Err(invalid("invalid extracted recovery chunk"));
            }
            bytes = bytes
                .checked_add(text.len() as i64)
                .ok_or_else(|| invalid("recovery text exceeds database length range"))?;
            digest.update(text.as_bytes());
            insert
                .execute(params![id, part as i64, text.as_bytes()])
                .await
                .map_err(error)?;
        }
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
        if chunk_count as i64 != self.integer("SELECT count(*) FROM chunks").await? {
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
            .prepare_cached("SELECT part, body FROM chunks WHERE id=? ORDER BY part")
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
                if super::super::legacy::needs_snapshot(buffer) {
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
