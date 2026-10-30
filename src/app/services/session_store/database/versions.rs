use super::{Database, content::Content, error, invalid};
use std::{collections::HashSet, io};
use turso::params;
mod navigation;
pub(crate) use navigation::{HistoryCheckpoint, RestoredVersion};

pub(super) const NAVIGATION_UPGRADE: &str = "
ALTER TABLE buffer_versions ADD COLUMN text_digest BLOB;
ALTER TABLE buffer_versions ADD COLUMN anchor_seq INTEGER;
ALTER TABLE buffer_versions ADD COLUMN anchor_before INTEGER NOT NULL DEFAULT 0;
ALTER TABLE buffer_versions ADD COLUMN branch_history BLOB;
ALTER TABLE buffer_versions ADD COLUMN branch_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE buffer_versions ADD COLUMN history_high_water INTEGER NOT NULL DEFAULT 0;
CREATE INDEX version_sequences ON buffer_versions(history_high_water);
CREATE INDEX version_sizes ON buffer_versions(temp_id,bytes,branch_bytes,digest);";

pub(super) const VERSION_INTERVAL_SECONDS: i64 = 300;
pub(super) const SCHEMA: &str = "
CREATE TABLE buffer_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, temp_id TEXT NOT NULL REFERENCES buffer_ids(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL, bytes INTEGER NOT NULL, digest BLOB NOT NULL, source_token TEXT,
    text_digest BLOB, anchor_seq INTEGER, anchor_before INTEGER NOT NULL DEFAULT 0, branch_history BLOB,
    branch_bytes INTEGER NOT NULL DEFAULT 0, history_high_water INTEGER NOT NULL DEFAULT 0) STRICT;
CREATE INDEX version_sequences ON buffer_versions(history_high_water);
CREATE INDEX version_sizes ON buffer_versions(temp_id,bytes,branch_bytes,digest);
CREATE UNIQUE INDEX version_source ON buffer_versions(temp_id, source_token);
CREATE INDEX versions_buffer ON buffer_versions(temp_id, id);
CREATE TABLE version_parts(version_id INTEGER NOT NULL REFERENCES buffer_versions(id) ON DELETE CASCADE,
    part INTEGER NOT NULL, digest BLOB NOT NULL REFERENCES blobs(digest), PRIMARY KEY(version_id, part)) STRICT;
CREATE INDEX version_parts_digest ON version_parts(digest);";

#[derive(Clone, Debug)]
pub struct BufferVersionInfo {
    pub id: i64,
    pub created_at_unix_seconds: i64,
    pub bytes: usize,
    pub undo_boundary: bool,
    pub text_digest: Vec<u8>,
    /// (global edit sequence, before rather than after that edit).
    pub anchor: Option<(u64, bool)>,
    pub has_history: bool,
    pub history_bytes: usize,
}

impl Database {
    pub(super) async fn preserve_undo_snapshot(
        &self,
        point: &super::super::capture::CapturedRecoveryPoint,
        now: i64,
    ) -> io::Result<()> {
        self.write_history_checkpoint(
            &HistoryCheckpoint {
                temp_id: point.temp_id.clone(),
                snapshot: point.snapshot.clone(),
                history: Vec::new(),
            },
            &format!("undo-{}", point.sequence),
            now,
            Some((point.sequence, true)),
            false,
        )
        .await
        .map(|_| ())
    }

    pub(super) async fn checkpoint_version(
        &self,
        id: &str,
        now: i64,
        snapshot: &crate::app::domain::DocumentSnapshot,
    ) -> io::Result<()> {
        // Select and create the point together; RETURNING identifies this insert,
        // never an unrelated previous row when no snapshot/eligible version exists.
        let created = self.query_collect(
            "WITH latest AS (SELECT created_at, digest FROM buffer_versions WHERE temp_id=? ORDER BY id DESC LIMIT 1)
             INSERT INTO buffer_versions(temp_id, created_at, bytes, digest)
             SELECT id, ?, bytes, digest FROM snapshots WHERE id=?
             AND NOT EXISTS(SELECT 1 FROM latest WHERE created_at>? OR latest.digest=snapshots.digest)
             RETURNING id",
            params![id, now, id, now.saturating_sub(VERSION_INTERVAL_SECONDS)],
            |row| row.get::<i64>(0).map_err(error),
        ).await?;
        let Some(&version) = created.first() else {
            return Ok(());
        };
        self.execute_cached(
            "INSERT INTO version_parts SELECT ?, part, digest FROM parts WHERE temp_id=?",
            params![version, id],
        )
        .await?;
        let history = self.read_history(id).await?;
        self.record_version_identity(
            version,
            navigation::history_anchor(&history),
            history
                .iter()
                .map(|entry| entry.global_seq)
                .max()
                .unwrap_or(0),
            snapshot,
        )
        .await?;
        Ok(())
    }

    pub(super) async fn collect_version_digests(
        &self,
        id: &str,
        removed: &mut HashSet<Vec<u8>>,
    ) -> io::Result<()> {
        self.collect_digests("SELECT p.digest FROM version_parts p JOIN buffer_versions v ON v.id=p.version_id WHERE v.temp_id=?", [id], removed).await
    }

    pub(in crate::app::services::session_store) async fn version_history(
        &mut self,
        id: &str,
    ) -> io::Result<Vec<BufferVersionInfo>> {
        self.ensure_healthy()?;
        let mut versions = self.query_collect(
            "SELECT id, created_at, bytes, source_token, text_digest, anchor_seq, anchor_before, branch_bytes FROM buffer_versions WHERE temp_id=? ORDER BY id DESC",
            [id],
            |row| {
                let before = row.get::<i64>(6).map_err(error)? != 0;
                let history_bytes = usize::try_from(row.get::<i64>(7).map_err(error)?).map_err(invalid)?;
                Ok(BufferVersionInfo {
                    id: row.get(0).map_err(error)?,
                    created_at_unix_seconds: row.get(1).map_err(error)?,
                    bytes: usize::try_from(row.get::<i64>(2).map_err(error)?).map_err(invalid)?,
                    undo_boundary: row.get::<Option<String>>(3).map_err(error)?.is_some_and(|token| token.starts_with("undo-")),
                    text_digest: row.get::<Option<Vec<u8>>>(4).map_err(error)?.unwrap_or_default(),
                    anchor: row.get::<Option<i64>>(5).map_err(error)?.map(|seq| (seq as u64, before)),
                    has_history: history_bytes != 0,
                    history_bytes,
                })
            },
        )
        .await?;
        // Older databases have no state identity. Backfill on demand, never during
        // startup or a paint frame; the worker owns the checksummed reads.
        for version in &mut versions {
            if version.text_digest.is_empty() {
                version.text_digest = self.version_text_digest(version.id).await?;
            }
        }
        Ok(versions)
    }

    pub(in crate::app::services::session_store) async fn read_version(
        &self,
        id: i64,
    ) -> io::Result<String> {
        self.ensure_healthy()?;
        let mut rows = self
            .conn
            .query(
                "SELECT id, bytes, digest FROM buffer_versions WHERE id=?",
                [id],
            )
            .await
            .map_err(error)?;
        let row = rows
            .next()
            .await
            .map_err(error)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "save point expired"))?;
        let mut content = Content::from_row(&row)?;
        drop(rows);
        let mut rows = self.conn.query("SELECT p.part, b.body FROM version_parts p LEFT JOIN blobs b ON b.digest=p.digest WHERE p.version_id=? ORDER BY p.part", [id]).await.map_err(error)?;
        while let Some(row) = rows.next().await.map_err(error)? {
            content.append_row(&row, 0)?;
        }
        content.finish()
    }
}
