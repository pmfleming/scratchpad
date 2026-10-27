use super::{Database, content::Content, error, invalid};
use std::{collections::HashSet, io};
use turso::params;

pub(super) const VERSION_LIMIT: usize = 16;
pub(super) const VERSION_INTERVAL_SECONDS: i64 = 300;
pub(super) const SCHEMA: &str = "
CREATE TABLE buffer_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, temp_id TEXT NOT NULL REFERENCES buffer_ids(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL, bytes INTEGER NOT NULL, digest BLOB NOT NULL, source_token TEXT) STRICT;
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
}

impl Database {
    pub(super) async fn preserve_undo_snapshot(
        &self,
        point: &super::super::capture::CapturedRecoveryPoint,
        now: i64,
    ) -> io::Result<()> {
        use super::chunking::{IncrementalChunks, hash_part, manifest_hasher};
        let token = format!("undo-{}", point.sequence);
        let existing = self.query_collect(
            "SELECT id FROM buffer_versions WHERE temp_id=? AND source_token=?",
            params![point.temp_id.as_str(), token.as_str()],
            |row| row.get::<i64>(0).map_err(error),
        ).await?;
        if !existing.is_empty() { return Ok(()); }
        let mut digest = manifest_hasher();
        let mut bytes = 0i64;
        let mut parts = Vec::new();
        for chunk in IncrementalChunks::new(&point.snapshot, None) {
            let chunk = chunk?;
            bytes += chunk.meta.len as i64;
            hash_part(&mut digest, chunk.meta);
            let hash = chunk.meta.digest.as_bytes().to_vec();
            if let Some(data) = chunk.data {
                self.execute_cached("INSERT INTO blobs(digest, body) SELECT ?, ? WHERE NOT EXISTS(SELECT 1 FROM blobs WHERE digest=?)",
                    params![hash.as_slice(), data, hash.as_slice()]).await?;
            }
            parts.push(hash);
        }
        let ids = self.query_collect(
            "INSERT INTO buffer_versions(temp_id,created_at,bytes,digest,source_token) VALUES(?,?,?,?,?) RETURNING id",
            params![point.temp_id.as_str(), now, bytes, digest.finalize().as_bytes().as_slice(), token],
            |row| row.get::<i64>(0).map_err(error),
        ).await?;
        let id = ids[0];
        for (part, hash) in parts.into_iter().enumerate() {
            self.execute_cached("INSERT INTO version_parts VALUES(?,?,?)", params![id, part as i64, hash]).await?;
        }
        Ok(())
    }

    pub(super) async fn checkpoint_version(
        &self,
        id: &str,
        now: i64,
        removed: &mut HashSet<Vec<u8>>,
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
        let expired = self
            .query_collect(
                "SELECT id FROM buffer_versions WHERE temp_id=? ORDER BY id DESC LIMIT -1 OFFSET ?",
                params![id, VERSION_LIMIT as i64],
                |row| row.get::<i64>(0).map_err(error),
            )
            .await?;
        for id in expired {
            self.collect_digests(
                "SELECT digest FROM version_parts WHERE version_id=?",
                [id],
                removed,
            )
            .await?;
            self.execute_cached("DELETE FROM buffer_versions WHERE id=?", [id])
                .await?;
        }
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
        &self,
        id: &str,
    ) -> io::Result<Vec<BufferVersionInfo>> {
        self.ensure_healthy()?;
        self.query_collect(
            "SELECT id, created_at, bytes, source_token FROM buffer_versions WHERE temp_id=? ORDER BY id DESC",
            [id],
            |row| {
                Ok(BufferVersionInfo {
                    id: row.get(0).map_err(error)?,
                    created_at_unix_seconds: row.get(1).map_err(error)?,
                    bytes: usize::try_from(row.get::<i64>(2).map_err(error)?).map_err(invalid)?,
                    undo_boundary: row.get::<Option<String>>(3).map_err(error)?.is_some(),
                })
            },
        )
        .await
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
