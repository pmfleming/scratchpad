use super::{Database, error, invalid};
use crate::app::domain::DocumentSnapshot;
use crate::app::services::session_store::model::SessionTab;
use std::collections::HashMap;
use std::io;
use turso::params;

const CHUNK_BYTES: usize = 256 * 1024;

struct Content {
    bytes: Vec<u8>,
    length: usize,
    digest: Vec<u8>,
    next_part: i64,
}
impl Content {
    fn from_row(row: &turso::Row) -> io::Result<Self> {
        Ok(Self {
            bytes: Vec::new(),
            length: usize::try_from(row.get::<i64>(1).map_err(error)?).map_err(invalid)?,
            digest: row.get(2).map_err(error)?,
            next_part: 0,
        })
    }
    fn append(&mut self, part: i64, bytes: Vec<u8>) -> io::Result<()> {
        if part != self.next_part
            || bytes.len() > CHUNK_BYTES
            || bytes.len() > self.length.saturating_sub(self.bytes.len())
        {
            return Err(invalid("invalid recovery content chunk"));
        }
        self.next_part += 1;
        self.bytes.extend(bytes);
        Ok(())
    }
    fn finish(self) -> io::Result<String> {
        if self.bytes.len() != self.length
            || blake3::hash(&self.bytes).as_bytes() != self.digest.as_slice()
        {
            return Err(invalid("recovery content checksum or length mismatch"));
        }
        String::from_utf8(self.bytes).map_err(invalid)
    }
}

impl Database {
    pub(super) async fn write_content(
        &self,
        id: &str,
        snapshot: &DocumentSnapshot,
    ) -> io::Result<()> {
        self.conn
            .execute("DELETE FROM chunks WHERE id=?", [id])
            .await
            .map_err(error)?;
        let mut insert = self
            .conn
            .prepare_cached("INSERT INTO chunks VALUES(?, ?, ?)")
            .await
            .map_err(error)?;
        let mut digest = blake3::Hasher::new();
        let mut part = 0i64;
        for span in snapshot.spans_for_range(0..snapshot.len_chars()) {
            for bytes in span.text.as_bytes().chunks(CHUNK_BYTES) {
                digest.update(bytes);
                insert
                    .execute(params![id, part, bytes])
                    .await
                    .map_err(error)?;
                part += 1;
            }
        }
        self.conn.execute(
            "INSERT INTO snapshots VALUES(?, ?, ?) ON CONFLICT(id) DO UPDATE SET bytes=excluded.bytes, digest=excluded.digest",
            params![id, snapshot.len_bytes() as i64, digest.finalize().as_bytes().as_slice()],
        ).await.map_err(error)?;
        Ok(())
    }

    pub(super) async fn read_all_contents(&self) -> io::Result<HashMap<String, String>> {
        let mut contents = HashMap::new();
        let mut rows = self
            .conn
            .query("SELECT id, bytes, digest FROM snapshots", ())
            .await
            .map_err(error)?;
        while let Some(row) = rows.next().await.map_err(error)? {
            contents.insert(
                row.get::<String>(0).map_err(error)?,
                Content::from_row(&row)?,
            );
        }
        drop(rows);
        let mut rows = self
            .conn
            .query("SELECT id, part, body FROM chunks ORDER BY id, part", ())
            .await
            .map_err(error)?;
        while let Some(row) = rows.next().await.map_err(error)? {
            let id: String = row.get(0).map_err(error)?;
            let content = contents
                .get_mut(&id)
                .ok_or_else(|| invalid("orphan recovery chunk"))?;
            content.append(row.get(1).map_err(error)?, row.get(2).map_err(error)?)?;
        }
        contents
            .into_iter()
            .map(|(id, content)| Ok((id, content.finish()?)))
            .collect()
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
            let mut rows = metadata
                .query([buffer.temp_id.as_str()])
                .await
                .map_err(error)?;
            let Some(row) = rows.next().await.map_err(error)? else {
                if super::super::legacy::needs_snapshot(buffer) {
                    return Err(invalid("required recovery content is missing"));
                }
                continue;
            };
            let mut content = Content::from_row(&row)?;
            drop(rows);
            let mut rows = chunks
                .query([buffer.temp_id.as_str()])
                .await
                .map_err(error)?;
            while let Some(row) = rows.next().await.map_err(error)? {
                content.append(row.get(0).map_err(error)?, row.get(1).map_err(error)?)?;
            }
            contents.insert(buffer.temp_id.clone(), content.finish()?);
        }
        Ok(contents)
    }
}
