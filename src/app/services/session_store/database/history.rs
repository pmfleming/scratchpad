//! `seq` is the stable per-document entry ID, NOT global_seq (which changes
//! when typing coalesces). Undo flags and payload-budget stripping also mutate
//! existing entries. Compare the complete value, serialize only changed rows.
use super::{Database, decode, error, invalid, quarantine::damaged};
use crate::app::domain::PersistedHistoryEntry;
use crate::app::services::session_store::model::SessionTab;
use std::io;
use turso::params;

pub(super) fn validate(entries: &[PersistedHistoryEntry]) -> io::Result<()> {
    let mut previous = 0;
    for entry in entries {
        let seq = i64::try_from(entry.id).map_err(invalid)?;
        if i64::try_from(entry.global_seq).map_err(invalid)? <= 0 {
            return Err(invalid("history global sequence must be positive"));
        }
        if seq <= previous {
            return Err(invalid("history entry IDs must be positive and increasing"));
        }
        previous = seq;
    }
    Ok(())
}

impl Database {
    pub(super) async fn read_history(&self, id: &str) -> io::Result<Vec<PersistedHistoryEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT seq, entry, global_seq FROM history WHERE temp_id=? ORDER BY seq",
                [id],
            )
            .await
            .map_err(error)?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            let entry: PersistedHistoryEntry = decode(&row.get::<Vec<u8>>(1).map_err(error)?)
                .map_err(|cause| damaged(id, cause))?;
            if i64::try_from(entry.id).ok() != Some(row.get(0).map_err(error)?)
                || i64::try_from(entry.global_seq).ok() != Some(row.get(2).map_err(error)?)
            {
                return Err(damaged(id, "history key does not match entry ID"));
            }
            entries.push(entry);
        }
        validate(&entries).map_err(|cause| damaged(id, cause))?;
        Ok(entries)
    }

    pub(super) async fn read_histories(&self, tabs: &mut [SessionTab]) -> io::Result<()> {
        for buffer in tabs.iter_mut().flat_map(|tab| &mut tab.buffers) {
            if !self.quarantined.contains_key(&buffer.temp_id) {
                buffer.text_history = self.read_history(&buffer.temp_id).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn write_history(
        &self,
        id: &str,
        entries: &[PersistedHistoryEntry],
    ) -> io::Result<()> {
        let old = self
            .history_cache
            .get(id)
            .ok_or_else(|| invalid("history cache not primed"))?;
        if old == entries {
            return Ok(());
        }
        let Some(first) = entries.first() else {
            return self
                .execute_cached("DELETE FROM history WHERE temp_id=?", [id])
                .await;
        };
        let last = entries.last().unwrap().id;
        // Budget evictions and redo-branch truncation are range deletes, with
        // individual deletes for interior holes (selective history operations).
        self.execute_cached(
            "DELETE FROM history WHERE temp_id=? AND seq<?",
            params![id, first.id as i64],
        )
        .await?;
        self.execute_cached(
            "DELETE FROM history WHERE temp_id=? AND seq>?",
            params![id, last as i64],
        )
        .await?;
        for entry in old
            .iter()
            .filter(|entry| entry.id >= first.id && entry.id <= last)
        {
            if entries
                .binary_search_by_key(&entry.id, |entry| entry.id)
                .is_err()
            {
                self.execute_cached(
                    "DELETE FROM history WHERE temp_id=? AND seq=?",
                    params![id, entry.id as i64],
                )
                .await?;
            }
        }
        for entry in entries {
            if old
                .binary_search_by_key(&entry.id, |entry| entry.id)
                .ok()
                .is_some_and(|index| old[index] == *entry)
            {
                continue;
            }
            let bytes = serde_json::to_vec(entry).map_err(invalid)?;
            self.execute_cached("INSERT INTO history VALUES(?, ?, ?, ?) ON CONFLICT(temp_id, seq) DO UPDATE SET global_seq=excluded.global_seq, entry=excluded.entry",
                params![id, entry.id as i64, entry.global_seq as i64, bytes]).await?;
        }
        Ok(())
    }
}
