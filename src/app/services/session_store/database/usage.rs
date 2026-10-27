use super::{Database, error};
use std::io;

/// Stored payload bytes. SQLite pages, indexes and the WAL are accounted separately.
#[derive(Clone, Debug, Default)]
pub struct StoredUsage {
    pub session_metadata: u64,
    pub undo: u64,
    pub archive_metadata: u64,
    pub snapshot_metadata: u64,
    pub live_text: u64,
    pub recovery_text: u64,
    pub shared_text: u64,
    pub unreferenced_text: u64,
    pub reusable_pages: u64,
    pub archive_referenced_bytes: u64,
    pub archives: usize,
    pub recovery_points: u64,
}

impl Database {
    pub(in crate::app::services::session_store) async fn stored_usage(
        &self,
    ) -> io::Result<StoredUsage> {
        let mut usage = StoredUsage {
            session_metadata: self.integer("SELECT coalesce(sum(length(payload)),0) FROM tabs").await? as u64
                + self.integer("SELECT coalesce(sum(length(payload)),0) FROM session_state").await? as u64,
            undo: self.integer("SELECT coalesce(sum(bytes),0) FROM history INDEXED BY history_sizes").await? as u64,
            archive_metadata: self.integer("SELECT coalesce(sum(length(payload)),0) FROM closed_tabs").await? as u64,
            snapshot_metadata: self.integer("SELECT coalesce(sum(length(temp_id)+length(digest)+24),0) FROM buffer_versions").await? as u64,
            reusable_pages: self.integer("PRAGMA freelist_count").await? as u64
                * self.integer("PRAGMA page_size").await? as u64,
            ..StoredUsage::default()
        };
        let archives = self.closed_rows().await?;
        usage.archives = archives.len();
        usage.recovery_points = self.integer("SELECT count(*) FROM buffer_versions").await? as u64;
        for archive in archives {
            let tab = archive.tab;
            usage.archive_referenced_bytes += archive.payload.len() as u64;
            for buffer in tab.buffers {
                let bytes = self.query_collect(
                    "SELECT coalesce((SELECT bytes FROM snapshots WHERE id=?1),0) + coalesce((SELECT sum(bytes) FROM history INDEXED BY history_sizes WHERE temp_id=?1),0) + coalesce((SELECT sum(bytes) FROM buffer_versions WHERE temp_id=?1),0)",
                    [buffer.temp_id.as_str()], |row| row.get::<i64>(0).map_err(error),
                ).await?;
                usage.archive_referenced_bytes += bytes[0] as u64;
            }
        }
        let mut rows = self.conn.query(
            "SELECT bytes, EXISTS(SELECT 1 FROM parts WHERE parts.digest=blobs.digest), EXISTS(SELECT 1 FROM version_parts WHERE version_parts.digest=blobs.digest) FROM blobs INDEXED BY blob_sizes", ()
        ).await.map_err(error)?;
        while let Some(row) = rows.next().await.map_err(error)? {
            let bytes = row.get::<i64>(0).map_err(error)? as u64;
            match (
                row.get::<i64>(1).map_err(error)? != 0,
                row.get::<i64>(2).map_err(error)? != 0,
            ) {
                (true, false) => usage.live_text += bytes,
                (false, true) => usage.recovery_text += bytes,
                (true, true) => usage.shared_text += bytes,
                (false, false) => usage.unreferenced_text += bytes,
            }
        }
        Ok(usage)
    }
}
