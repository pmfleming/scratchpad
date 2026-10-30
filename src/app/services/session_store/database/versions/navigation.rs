use super::{Database, error, invalid};
use crate::app::domain::{DocumentSnapshot, PersistedHistoryEntry};
use std::{
    io,
    time::{SystemTime, UNIX_EPOCH},
};
use turso::params;

#[derive(Clone)]
pub(crate) struct HistoryCheckpoint {
    pub temp_id: String,
    pub snapshot: DocumentSnapshot,
    pub history: Vec<PersistedHistoryEntry>,
}

#[derive(Clone)]
pub(crate) struct RestoredVersion {
    pub text: String,
    pub history: Vec<PersistedHistoryEntry>,
    pub return_version: i64,
}

pub(super) fn history_anchor(history: &[PersistedHistoryEntry]) -> Option<(u64, bool)> {
    history
        .iter()
        .rev()
        .find(|entry| !entry.flags.undone)
        .map(|entry| (entry.global_seq, false))
        .or_else(|| history.first().map(|entry| (entry.global_seq, true)))
}

impl Database {
    pub(super) async fn version_text_digest(&mut self, id: i64) -> io::Result<Vec<u8>> {
        use super::super::chunking::{ChunkMeta, MAX_BYTES, hash_part, manifest_hasher};
        let (length, expected) = self
            .query_collect(
                "SELECT bytes,digest FROM buffer_versions WHERE id=?",
                [id],
                |row| {
                    Ok((
                        row.get::<i64>(0).map_err(error)?,
                        row.get::<Vec<u8>>(1).map_err(error)?,
                    ))
                },
            )
            .await?
            .pop()
            .ok_or_else(|| invalid("snapshot expired"))?;
        let mut rows = self.conn.query("SELECT p.part,b.body FROM version_parts p LEFT JOIN blobs b ON b.digest=p.digest WHERE version_id=? ORDER BY part", [id]).await.map_err(error)?;
        let mut manifest = manifest_hasher();
        let mut content = blake3::Hasher::new();
        let mut observed = 0i64;
        let mut part = 0i64;
        while let Some(row) = rows.next().await.map_err(error)? {
            let index = row.get::<i64>(0).map_err(error)?;
            let bytes = row.get::<Vec<u8>>(1).map_err(error)?;
            if index != part || bytes.is_empty() || bytes.len() > MAX_BYTES {
                return Err(invalid("invalid snapshot chunk"));
            }
            observed += bytes.len() as i64;
            if observed > length {
                return Err(invalid("snapshot length mismatch"));
            }
            hash_part(
                &mut manifest,
                ChunkMeta {
                    len: bytes.len(),
                    digest: blake3::hash(&bytes),
                },
            );
            content.update(&bytes);
            part += 1;
        }
        drop(rows);
        if observed != length || manifest.finalize().as_bytes() != expected.as_slice() {
            return Err(invalid("snapshot checksum or length mismatch"));
        }
        let digest = content.finalize().as_bytes().to_vec();
        let before = self.integer("SELECT total_changes()").await?;
        self.execute_cached(
            "UPDATE buffer_versions SET text_digest=? WHERE id=?",
            params![digest.as_slice(), id],
        )
        .await?;
        self.acknowledge_inventory_writes(before).await;
        Ok(digest)
    }

    pub(super) async fn record_version_identity(
        &self,
        id: i64,
        anchor: Option<(u64, bool)>,
        high_water: u64,
        snapshot: &DocumentSnapshot,
    ) -> io::Result<()> {
        let mut digest = blake3::Hasher::new();
        for span in snapshot.spans_for_range(0..snapshot.len_chars()) {
            digest.update(span.text.as_bytes());
        }
        self.execute_cached(
                "UPDATE buffer_versions SET text_digest=?,anchor_seq=?,anchor_before=?,history_high_water=? WHERE id=?",
                params![
                    digest.finalize().as_bytes().as_slice(),
                    anchor.map(|(seq, _)| seq as i64),
                    i64::from(anchor.is_some_and(|(_, before)| before)),
                    high_water.max(anchor.map_or(0, |(seq, _)| seq)) as i64,
                    id
                ],
            )
            .await?;
        Ok(())
    }

    pub(super) async fn write_history_checkpoint(
        &self,
        point: &HistoryCheckpoint,
        token: &str,
        now: i64,
        anchor: Option<(u64, bool)>,
        branch: bool,
    ) -> io::Result<i64> {
        use super::super::chunking::{IncrementalChunks, hash_part, manifest_hasher};
        if let Some(id) = self
            .query_collect(
                "SELECT id FROM buffer_versions WHERE temp_id=? AND source_token=?",
                params![point.temp_id.as_str(), token],
                |row| row.get::<i64>(0).map_err(error),
            )
            .await?
            .first()
        {
            return Ok(*id);
        }
        let mut digest = manifest_hasher();
        let mut bytes = 0i64;
        let mut parts = Vec::new();
        for chunk in IncrementalChunks::new(&point.snapshot, None) {
            let chunk = chunk?;
            bytes += chunk.meta.len as i64;
            hash_part(&mut digest, chunk.meta);
            let hash = chunk.meta.digest.as_bytes().to_vec();
            if let Some(data) = chunk.data {
                self.execute_cached("INSERT INTO blobs(digest,body,bytes) VALUES(?,?,?) ON CONFLICT(digest) DO NOTHING",
                    params![hash.as_slice(), data.as_slice(), data.len() as i64]).await?;
            }
            parts.push(hash);
        }
        let history = branch
            .then(|| serde_json::to_vec(&point.history))
            .transpose()
            .map_err(error)?;
        let branch_bytes = history.as_ref().map_or(0, |bytes| bytes.len() as i64);
        let ids = self.query_collect(
            "INSERT INTO buffer_versions(temp_id,created_at,bytes,digest,source_token,branch_history,branch_bytes) VALUES(?,?,?,?,?,?,?) RETURNING id",
            params![point.temp_id.as_str(), now, bytes, digest.finalize().as_bytes().as_slice(), token, history, branch_bytes],
            |row| row.get::<i64>(0).map_err(error)).await?;
        let id = ids[0];
        for (part, hash) in parts.into_iter().enumerate() {
            self.execute_cached(
                "INSERT INTO version_parts VALUES(?,?,?)",
                params![id, part as i64, hash],
            )
            .await?;
        }
        self.record_version_identity(
            id,
            anchor,
            point
                .history
                .iter()
                .map(|entry| entry.global_seq)
                .max()
                .unwrap_or(0),
            &point.snapshot,
        )
        .await?;
        Ok(id)
    }

    pub(in crate::app::services::session_store) async fn read_history_version(
        &self,
        temp_id: &str,
        target: i64,
    ) -> io::Result<RestoredVersion> {
        self.ensure_healthy()?;
        let history = self
            .query_collect(
                "SELECT branch_history FROM buffer_versions WHERE id=? AND temp_id=?",
                params![target, temp_id],
                |row| row.get::<Option<Vec<u8>>>(0).map_err(error),
            )
            .await?
            .pop()
            .ok_or_else(|| invalid("snapshot is no longer available for this file"))?;
        let history = history
            .map(|bytes| serde_json::from_slice(&bytes).map_err(error))
            .transpose()?
            .unwrap_or_default();
        Ok(RestoredVersion {
            text: self.read_version(target).await?,
            history,
            return_version: target,
        })
    }

    /// Commit the return state before touching the editor. Browsing creates no
    /// replacement undo records; a divergent branch remains recoverable on restart.
    pub(in crate::app::services::session_store) async fn navigate_version(
        &mut self,
        point: HistoryCheckpoint,
        target: i64,
    ) -> io::Result<RestoredVersion> {
        self.ensure_healthy()?;
        let RestoredVersion { text, history, .. } =
            self.read_history_version(&point.temp_id, target).await?;
        let mut hash = blake3::Hasher::new();
        for chunk in point
            .snapshot
            .spans_for_range(0..point.snapshot.len_chars())
        {
            hash.update(chunk.text.as_bytes());
        }
        let content_digest = hash.finalize();
        // Stable across restore/import: runtime generations and persisted flags
        // are not part of a branch's identity.
        for entry in &point.history {
            hash.update(
                &serde_json::to_vec(&(
                    entry.id,
                    entry.global_seq,
                    entry.flags.undone,
                    entry.flags.replayable,
                    &entry.edits,
                ))
                .map_err(error)?,
            );
        }
        let token = format!("branch-{}", hash.finalize().to_hex());
        let existing = if point.history.is_empty() {
            self.query_collect("SELECT id FROM buffer_versions WHERE temp_id=? AND text_digest=? AND branch_bytes=0 LIMIT 1",
                params![point.temp_id.as_str(), content_digest.as_bytes().as_slice()], |row| row.get::<i64>(0).map_err(error)).await?.first().copied()
        } else {
            None
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .min(i64::MAX as u64) as i64;
        let before = self.integer("SELECT total_changes()").await?;
        self.conn
            .execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(error)?;
        let result = match existing {
            Some(id) => Ok(id),
            None => {
                self.write_history_checkpoint(
                    &point,
                    &token,
                    now,
                    history_anchor(&point.history),
                    !point.history.is_empty(),
                )
                .await
            }
        };
        let return_version = result.as_ref().ok().copied();
        self.finish_transaction(result.map(|_| ())).await?;
        self.acknowledge_inventory_writes(before).await;
        let return_version = return_version.ok_or_else(|| invalid("missing return checkpoint"))?;
        self.navigation_roots
            .insert(point.temp_id, (target, return_version));
        Ok(RestoredVersion {
            text,
            history,
            return_version,
        })
    }
}
