use super::{Database, error};
use std::io;

/// Stored payload bytes. SQLite pages, indexes and the WAL are accounted separately.
#[derive(Clone, Debug, Default)]
pub struct StoredUsage {
    pub session_metadata: u64,
    pub undo: u64,
    pub archive_metadata: u64,
    pub live_text: u64,
    pub unreferenced_text: u64,
    pub reusable_pages: u64,
    pub archive_referenced_bytes: u64,
    pub archives: usize,
}

impl Database {
    pub(in crate::app::services::session_store) async fn stored_usage(
        &self,
    ) -> io::Result<StoredUsage> {
        // Project only archive buffer IDs, not complete SessionTabs, and aggregate
        // their costs in bulk. No query per archived buffer or text payload reads.
        let mut usage = self.query_collect(
            "WITH costs AS (
                SELECT id AS temp_id, bytes FROM snapshots
                UNION ALL SELECT temp_id, bytes FROM history INDEXED BY history_sizes
             ), totals AS (SELECT temp_id, sum(bytes) AS bytes FROM costs GROUP BY temp_id),
             archives AS (SELECT count(*) AS count, coalesce(sum(length(payload)),0) AS bytes FROM closed_tabs)
             SELECT
                (SELECT coalesce(sum(length(payload)),0) FROM tabs) + (SELECT coalesce(sum(length(payload)),0) FROM session_state),
                (SELECT coalesce(sum(bytes),0) FROM history INDEXED BY history_sizes),
                archives.bytes, 0, archives.count, 0,
                archives.bytes + (SELECT coalesce(sum(totals.bytes),0)
                    FROM closed_tabs, json_each(CAST(closed_tabs.payload AS TEXT), '$.buffers') AS buffer
                    LEFT JOIN totals ON totals.temp_id=json_extract(buffer.value, '$.temp_id'))
             FROM archives",
            (),
            |row| Ok(StoredUsage {
                session_metadata: row.get::<i64>(0).map_err(error)? as u64,
                undo: row.get::<i64>(1).map_err(error)? as u64,
                archive_metadata: row.get::<i64>(2).map_err(error)? as u64,
                archives: row.get::<i64>(4).map_err(error)? as usize,
                archive_referenced_bytes: row.get::<i64>(6).map_err(error)? as u64,
                ..StoredUsage::default()
            }),
        ).await?.pop().ok_or_else(|| super::invalid("missing usage result"))?;
        usage.reusable_pages = self.integer("PRAGMA freelist_count").await? as u64
            * self.integer("PRAGMA page_size").await? as u64;
        // Deduplicate roots before joining so shared chunks count once. Return
        // four aggregates instead of sending every blob through the worker.
        for (kind, bytes) in self.query_collect(
            "WITH roots AS (
                SELECT digest, 1 AS live, 0 AS recovery FROM parts
             ), refs AS (SELECT digest, max(live)+2*max(recovery) AS kind FROM roots GROUP BY digest)
             SELECT coalesce(refs.kind,0), sum(blobs.bytes) FROM blobs INDEXED BY blob_sizes
             LEFT JOIN refs ON refs.digest=blobs.digest GROUP BY coalesce(refs.kind,0)",
            (),
            |row| Ok((row.get::<i64>(0).map_err(error)?, row.get::<i64>(1).map_err(error)? as u64)),
        ).await? {
            match kind {
                1 => usage.live_text = bytes,
                _ => usage.unreferenced_text = bytes,
            }
        }
        Ok(usage)
    }
}
