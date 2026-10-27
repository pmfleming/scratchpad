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
        let mut latest = self.conn.query("SELECT created_at, digest FROM buffer_versions WHERE temp_id=? ORDER BY id DESC LIMIT 1", [id]).await.map_err(error)?;
        if let Some(row) = latest.next().await.map_err(error)? {
            let last: i64 = row.get(0).map_err(error)?;
            if now.saturating_sub(last) < VERSION_INTERVAL_SECONDS {
                return Ok(());
            }
            let old_digest: Vec<u8> = row.get(1).map_err(error)?;
            let mut snapshot = self
                .conn
                .query("SELECT digest FROM snapshots WHERE id=?", [id])
                .await
                .map_err(error)?;
            if let Some(row) = snapshot.next().await.map_err(error)?
                && row.get::<Vec<u8>>(0).map_err(error)? == old_digest
            {
                return Ok(());
            }
        }
        drop(latest);
        self.execute_cached("INSERT INTO buffer_versions(temp_id, created_at, bytes, digest) SELECT id, ?, bytes, digest FROM snapshots WHERE id=?", params![now, id]).await?;
        // A snapshot always exists: called only after a successful content write.
        let version = self.integer("SELECT last_insert_rowid()").await?;
        self.execute_cached(
            "INSERT INTO version_parts SELECT ?, part, digest FROM parts WHERE temp_id=?",
            params![version, id],
        )
        .await?;
        let mut expired = self
            .conn
            .query(
                "SELECT id FROM buffer_versions WHERE temp_id=? ORDER BY id DESC LIMIT -1 OFFSET ?",
                params![id, VERSION_LIMIT as i64],
            )
            .await
            .map_err(error)?;
        let mut ids = Vec::new();
        while let Some(row) = expired.next().await.map_err(error)? {
            ids.push(row.get::<i64>(0).map_err(error)?);
        }
        drop(expired);
        for id in ids {
            let mut rows = self
                .conn
                .query("SELECT digest FROM version_parts WHERE version_id=?", [id])
                .await
                .map_err(error)?;
            while let Some(row) = rows.next().await.map_err(error)? {
                removed.insert(row.get(0).map_err(error)?);
            }
            drop(rows);
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
        let mut rows = self.conn.query("SELECT p.digest FROM version_parts p JOIN buffer_versions v ON v.id=p.version_id WHERE v.temp_id=?", [id]).await.map_err(error)?;
        while let Some(row) = rows.next().await.map_err(error)? {
            removed.insert(row.get(0).map_err(error)?);
        }
        Ok(())
    }

    pub(in crate::app::services::session_store) async fn version_history(
        &self,
        id: &str,
    ) -> io::Result<Vec<BufferVersionInfo>> {
        self.ensure_healthy()?;
        let mut rows = self.conn.query("SELECT id, created_at, bytes FROM buffer_versions WHERE temp_id=? ORDER BY id DESC", [id]).await.map_err(error)?;
        let mut versions = Vec::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            versions.push(BufferVersionInfo {
                id: row.get(0).map_err(error)?,
                created_at_unix_seconds: row.get(1).map_err(error)?,
                bytes: usize::try_from(row.get::<i64>(2).map_err(error)?).map_err(invalid)?,
            });
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
