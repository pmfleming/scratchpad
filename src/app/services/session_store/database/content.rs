//! Reference manifests. Body reads/encoding finish before BEGIN IMMEDIATE.
use super::{Database, error, invalid, quarantine::damaged};
use crate::app::domain::DocumentSnapshot;
use crate::app::domain::buffer::{PieceBuffer, PieceTreeLite, StorageBlock};
use crate::app::services::{
    chunk_store::{ChunkReference, Metadata},
    session_store::model::SessionTab,
};
use std::{
    collections::{HashMap, HashSet},
    io,
};
use turso::params;

#[derive(Clone)]
pub(super) struct TailCheckpoint {
    start: usize,
    len: usize,
    digest: blake3::Hash,
}
pub(super) struct PreparedContent {
    blocks: Vec<(i64, i64, i64, i64)>,
    old_blocks: Vec<(i64, i64, i64, i64)>,
    tail: Option<(usize, usize, Vec<u8>, [u8; 32])>,
    replace_tail: bool,
    pub checkpoint: Option<TailCheckpoint>,
    pieces: Vec<(Vec<u8>, blake3::Hash)>,
    old_pieces: Vec<blake3::Hash>,
    bytes: i64,
}
fn number(value: usize) -> io::Result<i64> {
    i64::try_from(value).map_err(invalid)
}

impl Database {
    pub(super) async fn prepare_content(
        &mut self,
        id: &str,
        tree: &PieceTreeLite,
    ) -> io::Result<PreparedContent> {
        let store = self.chunk_store()?;
        let capture = tree.capture_storage(&store)?;
        // The actor drains directly: submitting a job to itself would deadlock.
        while capture.blocks.iter().any(|b| b.chunk.reference().is_none()) {
            self.drain_chunk_batch().await?;
        }
        let mut positions = [0i64; 2];
        let mut blocks = Vec::with_capacity(capture.blocks.len());
        for block in capture.blocks {
            let buffer = usize::from(block.buffer == PieceBuffer::Add);
            let reference = block
                .chunk
                .reference()
                .ok_or_else(|| invalid("capture body did not commit"))?;
            blocks.push((
                buffer as i64,
                positions[buffer],
                number(block.start)?,
                reference.id,
            ));
            positions[buffer] += 1;
        }
        let old_blocks = self.query_collect("SELECT buffer,position,start_byte,chunk_id FROM document_text WHERE temp_id=? ORDER BY buffer,position",[id],|r|Ok((r.get(0).map_err(error)?,r.get(1).map_err(error)?,r.get(2).map_err(error)?,r.get(3).map_err(error)?))).await?;
        if !self.tail_cache.contains_key(id)
            && let Some((start, text)) = self.read_tail(id).await?
        {
            self.tail_cache.insert(
                id.to_owned(),
                TailCheckpoint {
                    start,
                    len: text.len(),
                    digest: blake3::hash(text.as_bytes()),
                },
            );
        }
        let old = self.tail_cache.get(id);
        let checkpoint = capture.tail.as_ref().map(|(start, text)| TailCheckpoint {
            start: *start,
            len: text.len(),
            digest: blake3::hash(text.as_bytes()),
        });
        let append = capture
            .tail
            .as_ref()
            .zip(old)
            .filter(|((start, text), old)| {
                *start == old.start
                    && text.len() >= old.len
                    && blake3::hash(&text.as_bytes()[..old.len]) == old.digest
            })
            .map(|(_, old)| old.len);
        let tail = capture.tail.map(|(start, text)| {
            let offset = append.unwrap_or(0);
            let body = text.as_bytes()[offset..].to_vec();
            let digest = *blake3::hash(&body).as_bytes();
            (start, offset, body, digest)
        });
        let old_pieces = self
            .read_piece_parts(id)
            .await?
            .map_or_else(Vec::new, |(_, _, hashes)| hashes);
        let pieces = tree
            .encode_pieces()
            .chunks(65536)
            .map(|part| (part.to_vec(), blake3::hash(part)))
            .collect();
        Ok(PreparedContent {
            blocks,
            old_blocks,
            tail,
            replace_tail: append.is_none(),
            checkpoint,
            pieces,
            old_pieces,
            bytes: number(tree.len_bytes())?,
        })
    }
    pub(super) async fn write_prepared_content(
        &self,
        id: &str,
        content: &PreparedContent,
    ) -> io::Result<()> {
        let common = content
            .blocks
            .iter()
            .zip(&content.old_blocks)
            .take_while(|(a, b)| a == b)
            .count();
        for &(buffer, position, _, _) in &content.old_blocks[common..] {
            self.execute_cached(
                "DELETE FROM document_text WHERE temp_id=? AND buffer=? AND position=?",
                params![id, buffer, position],
            )
            .await?;
        }
        for &(buffer, position, start, chunk) in &content.blocks[common..] {
            self.execute_cached(
                "INSERT INTO document_text VALUES(?,?,?,?,?)",
                params![id, buffer, position, start, chunk],
            )
            .await?;
        }
        if content.replace_tail {
            self.execute_cached("DELETE FROM document_tail WHERE temp_id=?", [id])
                .await?;
        }
        if let Some((start, offset, body, digest)) = &content.tail
            && !body.is_empty()
        {
            self.execute_cached(
                "INSERT INTO document_tail VALUES(?,?,?,?,?)",
                params![
                    id,
                    number(*start)?,
                    number(*offset)?,
                    body.as_slice(),
                    digest.as_slice()
                ],
            )
            .await?;
        }
        for (part, (body, digest)) in content.pieces.iter().enumerate() {
            if content.old_pieces.get(part) == Some(digest) {
                continue;
            }
            self.execute_cached("INSERT INTO document_pieces VALUES(?,?,?,?,?) ON CONFLICT(id,part) DO UPDATE SET bytes=excluded.bytes,digest=excluded.digest,body=excluded.body",params![id,number(part)?,if part==0 {content.bytes}else{0},digest.as_bytes().as_slice(),body.as_slice()]).await?;
        }
        if content.old_pieces.len() > content.pieces.len() {
            self.execute_cached(
                "DELETE FROM document_pieces WHERE id=? AND part>=?",
                params![id, number(content.pieces.len())?],
            )
            .await?;
        }
        Ok(())
    }
    pub(super) async fn read_tail(&self, id: &str) -> io::Result<Option<(usize, String)>> {
        let rows = self.query_collect("SELECT start_byte,offset,body,digest FROM document_tail WHERE temp_id=? ORDER BY offset",[id],|r|Ok((usize::try_from(r.get::<i64>(0).map_err(error)?).map_err(invalid)?,usize::try_from(r.get::<i64>(1).map_err(error)?).map_err(invalid)?,r.get::<Vec<u8>>(2).map_err(error)?,r.get::<Vec<u8>>(3).map_err(error)?))).await?;
        let mut result = None;
        for (start, offset, body, digest) in rows {
            let (base, text) = result.get_or_insert_with(|| (start, String::new()));
            if start != *base
                || offset != text.len()
                || blake3::hash(&body).as_bytes().as_slice() != digest
                || text.len() + body.len() > 256 * 1024
            {
                return Err(damaged(id, "invalid tail segments"));
            }
            text.push_str(std::str::from_utf8(&body).map_err(|e| damaged(id, e))?);
        }
        Ok(result)
    }
    async fn read_piece_parts(
        &self,
        id: &str,
    ) -> io::Result<Option<(i64, Vec<u8>, Vec<blake3::Hash>)>> {
        let rows = self
            .query_collect(
                "SELECT part,bytes,digest,body FROM document_pieces WHERE id=? ORDER BY part",
                [id],
                |r| {
                    Ok((
                        r.get::<i64>(0).map_err(error)?,
                        r.get::<i64>(1).map_err(error)?,
                        r.get::<Vec<u8>>(2).map_err(error)?,
                        r.get::<Vec<u8>>(3).map_err(error)?,
                    ))
                },
            )
            .await?;
        if rows.is_empty() {
            return Ok(None);
        }
        let mut pieces = Vec::new();
        let mut hashes = Vec::new();
        let mut length = 0;
        for (index, (part, bytes, digest, body)) in rows.into_iter().enumerate() {
            let hash = blake3::hash(&body);
            if part != number(index)?
                || body.len() > 65536
                || body.is_empty()
                || (part > 0 && bytes != 0)
                || bytes < 0
                || hash.as_bytes().as_slice() != digest
            {
                return Err(damaged(
                    id,
                    "invalid piece metadata part, length or checksum",
                ));
            }
            if part == 0 {
                length = bytes;
            }
            pieces.extend(body);
            hashes.push(hash);
        }
        Ok(Some((length, pieces, hashes)))
    }
    pub(super) async fn read_reference_tree(&self, id: &str) -> io::Result<Option<PieceTreeLite>> {
        let Some((bytes, pieces, _)) = self.read_piece_parts(id).await? else {
            return Ok(None);
        };
        let store = self.chunk_store()?;
        let owner = crate::app::domain::buffer::text_cache::new_owner();
        let blocks = self.query_collect("SELECT t.buffer,t.position,t.start_byte,c.id,c.digest,c.byte_len,c.char_len,c.newline_count,c.is_ascii FROM document_text t LEFT JOIN chunks c ON c.id=t.chunk_id WHERE t.temp_id=? ORDER BY t.buffer,t.position",[id],|r| {
            let buffer = match r.get::<i64>(0).map_err(error)? {0=>PieceBuffer::Original,1=>PieceBuffer::Add,_=>return Err(invalid("invalid storage buffer"))};
            let position = r.get::<i64>(1).map_err(error)?;
            let start = usize::try_from(r.get::<i64>(2).map_err(error)?).map_err(invalid)?;
            let metadata=Metadata {digest:r.get::<Vec<u8>>(4).map_err(error)?.try_into().map_err(|_|invalid("invalid chunk digest"))?,byte_len:usize::try_from(r.get::<i64>(5).map_err(error)?).map_err(invalid)?,char_len:super::document_rows::unsigned(r.get(6).map_err(error)?)?,newline_count:super::document_rows::unsigned(r.get(7).map_err(error)?)?,is_ascii:r.get::<i64>(8).map_err(error)?==1};
            Ok((position,StorageBlock {buffer,start,chunk:store.restored(ChunkReference {id:r.get(3).map_err(error)?,metadata,epoch:store.epoch()},owner)}))
        }).await.map_err(|e|damaged(id,e))?;
        let mut positions = [0; 2];
        let blocks = blocks
            .into_iter()
            .map(|(position, block)| {
                let side = usize::from(block.buffer == PieceBuffer::Add);
                if positions[side] != position {
                    return Err(damaged(id, "noncontiguous storage positions"));
                }
                positions[side] += 1;
                Ok(block)
            })
            .collect::<io::Result<Vec<_>>>()?;
        let tree =
            PieceTreeLite::restore_references(store, blocks, self.read_tail(id).await?, &pieces)
                .map_err(|e| damaged(id, e))?;
        if number(tree.len_bytes())? != bytes {
            return Err(damaged(id, "piece text length mismatch"));
        }
        Ok(Some(tree))
    }
    /// Materialization is reserved for explicit recovery export and test probes.
    #[cfg(test)]
    pub(super) async fn read_all_contents(&self) -> io::Result<HashMap<String, String>> {
        let mut contents = HashMap::new();
        for id in self.string_set("SELECT id FROM document_pieces").await? {
            let tree = self
                .read_reference_tree(&id)
                .await?
                .ok_or_else(|| damaged(&id, "missing text metadata"))?;
            let mut text = String::with_capacity(tree.len_bytes());
            for span in tree.try_spans_for_range(0..tree.len_chars()) {
                text.push_str(&span.map_err(|e| damaged(&id, e))?.text);
            }
            contents.insert(id, text);
        }
        Ok(contents)
    }
    pub(super) async fn read_tab_contents(
        &self,
        tabs: &[SessionTab],
    ) -> io::Result<HashMap<String, String>> {
        if let Some(buffer) = tabs
            .iter()
            .flat_map(|t| &t.buffers)
            .find(|b| self.quarantined.contains_key(&b.temp_id))
        {
            return Err(damaged(
                &buffer.temp_id,
                "original recovery data is protected",
            ));
        }
        let required: Vec<_> = tabs
            .iter()
            .flat_map(|t| &t.buffers)
            .filter(|b| super::super::validation::needs_snapshot(b))
            .map(|b| b.temp_id.as_str())
            .collect();
        if !required.is_empty() {
            let encoded = serde_json::to_string(&required).map_err(invalid)?;
            let present:HashSet<_>=self.query_collect("SELECT id FROM document_pieces WHERE part=0 AND id IN (SELECT value FROM json_each(?))",[encoded.as_str()],|r|r.get::<String>(0).map_err(error)).await?.into_iter().collect();
            if let Some(id) = required.into_iter().find(|id| !present.contains(*id)) {
                return Err(damaged(id, "required text manifest is missing"));
            }
        }
        Ok(HashMap::new())
    }
    pub(super) async fn write_content(
        &self,
        id: &str,
        snapshot: &DocumentSnapshot,
        _removed: &mut HashSet<Vec<u8>>,
        _profile: &mut super::SessionPersistProfile,
    ) -> io::Result<()> {
        // Save As path release may retain an empty document inside the manifest.
        if snapshot.len_bytes() != 0 {
            return Err(invalid(
                "content must be prepared before the manifest transaction",
            ));
        }
        let pieces = PieceTreeLite::from_string(String::new()).encode_pieces();
        self.execute_cached(
            "INSERT INTO document_pieces VALUES(?,0,0,?,?) ON CONFLICT(id,part) DO NOTHING",
            params![
                id,
                blake3::hash(&pieces).as_bytes().as_slice(),
                pieces.as_slice()
            ],
        )
        .await
    }
}
