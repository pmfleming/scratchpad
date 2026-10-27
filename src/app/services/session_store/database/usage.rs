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
}

impl Database {
    pub(in crate::app::services::session_store) async fn stored_usage(&self) -> io::Result<StoredUsage> {
        let mut usage = StoredUsage {
            session_metadata: self.integer("SELECT coalesce(sum(length(payload)),0) FROM tabs").await? as u64
                + self.integer("SELECT coalesce(sum(length(payload)),0) FROM session_state").await? as u64,
            undo: self.integer("SELECT coalesce(sum(length(entry)),0) FROM history").await? as u64,
            archive_metadata: self.integer("SELECT coalesce(sum(length(payload)),0) FROM closed_tabs").await? as u64,
            snapshot_metadata: self.integer("SELECT coalesce(sum(length(temp_id)+length(digest)+24),0) FROM buffer_versions").await? as u64,
            reusable_pages: self.integer("PRAGMA freelist_count").await? as u64
                * self.integer("PRAGMA page_size").await? as u64,
            ..StoredUsage::default()
        };
        let mut rows = self.conn.query(
            "SELECT length(body), EXISTS(SELECT 1 FROM parts WHERE parts.digest=blobs.digest), EXISTS(SELECT 1 FROM version_parts WHERE version_parts.digest=blobs.digest) FROM blobs", ()
        ).await.map_err(error)?;
        while let Some(row) = rows.next().await.map_err(error)? {
            let bytes = row.get::<i64>(0).map_err(error)? as u64;
            match (row.get::<i64>(1).map_err(error)? != 0, row.get::<i64>(2).map_err(error)? != 0) {
                (true, false) => usage.live_text += bytes,
                (false, true) => usage.recovery_text += bytes,
                (true, true) => usage.shared_text += bytes,
                (false, false) => usage.unreferenced_text += bytes,
            }
        }
        Ok(usage)
    }
}
