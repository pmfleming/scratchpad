//! `seq` is the stable per-document entry ID, NOT global_seq (which changes
//! when typing coalesces). Undo flags and payload-budget stripping also mutate
//! existing entries. Fingerprint the complete serialized row; retain no undo
//! payload copies in the worker's comparison cache.
use super::{Database, decode, error, invalid, quarantine::damaged};
use crate::app::domain::PersistedHistoryEntry;
use crate::app::services::session_store::model::SessionTab;
use std::io;
use turso::params;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HistoryFingerprint {
    pub id: u64,
    digest: blake3::Hash,
}

fn fingerprint(entry: &PersistedHistoryEntry) -> io::Result<HistoryFingerprint> {
    let mut hasher = blake3::Hasher::new();
    // Stream into the hash rather than allocate a second serialized payload.
    serde_json::to_writer(&mut hasher, entry).map_err(invalid)?;
    Ok(HistoryFingerprint {
        id: entry.id,
        digest: hasher.finalize(),
    })
}

pub(super) fn fingerprints(
    entries: &[PersistedHistoryEntry],
) -> io::Result<Vec<HistoryFingerprint>> {
    entries.iter().map(fingerprint).collect()
}

fn validate_entry(entry: &PersistedHistoryEntry, previous: &mut i64) -> io::Result<()> {
    let seq = i64::try_from(entry.id).map_err(invalid)?;
    if seq <= *previous || i64::try_from(entry.global_seq).map_err(invalid)? <= 0 {
        return Err(invalid(
            "history IDs must increase and global sequences must be positive",
        ));
    }
    *previous = seq;
    Ok(())
}

pub(super) fn validate(entries: &[PersistedHistoryEntry]) -> io::Result<()> {
    let mut previous = 0;
    entries
        .iter()
        .try_for_each(|entry| validate_entry(entry, &mut previous))
}

fn history_entry(row: &turso::Row, previous: &mut i64) -> io::Result<PersistedHistoryEntry> {
    let entry: PersistedHistoryEntry = decode(&row.get::<Vec<u8>>(1).map_err(error)?)?;
    validate_entry(&entry, previous)?;
    if entry.id as i64 != row.get::<i64>(0).map_err(error)?
        || entry.global_seq as i64 != row.get::<i64>(2).map_err(error)?
    {
        return Err(invalid("history key does not match entry ID"));
    }
    Ok(entry)
}

impl Database {
    pub(super) async fn read_history(&self, id: &str) -> io::Result<Vec<PersistedHistoryEntry>> {
        self.map_history(id, Ok).await
    }

    pub(super) async fn read_history_fingerprints(
        &self,
        id: &str,
    ) -> io::Result<Vec<HistoryFingerprint>> {
        self.map_history(id, |entry| fingerprint(&entry)).await
    }

    async fn map_history<T>(
        &self,
        id: &str,
        mut map: impl FnMut(PersistedHistoryEntry) -> io::Result<T>,
    ) -> io::Result<Vec<T>> {
        let mut previous = 0;
        self.query_collect(
            "SELECT seq, entry, global_seq FROM history WHERE temp_id=? ORDER BY seq",
            [id],
            |row| {
                history_entry(row, &mut previous)
                    .and_then(&mut map)
                    .map_err(|cause| damaged(id, cause))
            },
        )
        .await
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
        fingerprints: &[HistoryFingerprint],
    ) -> io::Result<()> {
        let old = self
            .history_cache
            .get(id)
            .ok_or_else(|| invalid("history cache not primed"))?;
        if old == fingerprints {
            return Ok(());
        }
        let Some((first, last)) = entries.first().zip(entries.last()) else {
            return self
                .execute_cached("DELETE FROM history WHERE temp_id=?", [id])
                .await;
        };
        let last = last.id;
        // Budget evictions and redo-branch truncation are range deletes, with
        // individual deletes for interior holes (selective history operations).
        self.execute_cached(
            "DELETE FROM history WHERE temp_id=? AND (seq<? OR seq>?)",
            params![id, first.id as i64, last as i64],
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
        for (entry, fingerprint) in entries.iter().zip(fingerprints) {
            if old
                .binary_search_by_key(&entry.id, |entry| entry.id)
                .ok()
                .is_some_and(|index| old[index] == *fingerprint)
            {
                continue;
            }
            let bytes = serde_json::to_vec(entry).map_err(invalid)?;
            self.execute_cached("INSERT INTO history(temp_id, seq, global_seq, entry, bytes) VALUES(?, ?, ?, ?, ?) ON CONFLICT(temp_id, seq) DO UPDATE SET global_seq=excluded.global_seq, entry=excluded.entry, bytes=excluded.bytes",
                params![id, entry.id as i64, entry.global_seq as i64, bytes.as_slice(), bytes.len() as i64]).await?;
        }
        Ok(())
    }
}
