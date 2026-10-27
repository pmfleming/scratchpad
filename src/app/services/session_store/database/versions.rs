use super::{Database, content::Content, error, invalid};
use std::{collections::HashSet, io};
use turso::params;

pub(super) const VERSION_LIMIT: usize = 16;
pub(super) const VERSION_INTERVAL_SECONDS: i64 = 300;
pub(super) const SCHEMA: &str = "
CREATE TABLE buffer_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, temp_id TEXT NOT NULL REFERENCES buffer_ids(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL, bytes INTEGER NOT NULL, digest BLOB NOT NULL) STRICT;
CREATE INDEX versions_buffer ON buffer_versions(temp_id, id);
CREATE TABLE version_parts(version_id INTEGER NOT NULL REFERENCES buffer_versions(id) ON DELETE CASCADE,
    part INTEGER NOT NULL, digest BLOB NOT NULL REFERENCES blobs(digest), PRIMARY KEY(version_id, part)) STRICT;
CREATE INDEX version_parts_digest ON version_parts(digest);";

#[derive(Clone, Debug)]
pub struct BufferVersionInfo {
    pub id: i64,
    pub created_at_unix_seconds: i64,
    pub bytes: usize,
}

impl Database {
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
            "SELECT id, created_at, bytes FROM buffer_versions WHERE temp_id=? ORDER BY id DESC",
            [id],
            |row| {
                Ok(BufferVersionInfo {
                    id: row.get(0).map_err(error)?,
                    created_at_unix_seconds: row.get(1).map_err(error)?,
                    bytes: usize::try_from(row.get::<i64>(2).map_err(error)?).map_err(invalid)?,
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
