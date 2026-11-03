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
        .try_for_each(|entry| validate_entry(entry, &mut previous))?;
    let current: Vec<_> = entries.iter().filter(|e| e.current).collect();
    let tips: Vec<_> = entries.iter().filter(|e| e.selected_tip).collect();
    if current.len() > 1 || tips.len() > 1 || (!entries.is_empty() && tips.is_empty()) {
        return Err(invalid("invalid history cursor"));
    }
    crate::app::domain::buffer::edit_tree::EditTree::restore(
        entries.iter().map(|e| (e.id, e.parent_id)),
        current.first().map_or(0, |e| e.id),
        tips.first().map_or(0, |e| e.id),
    )
    .ok_or_else(|| invalid("invalid history tree"))?;
    Ok(())
}

fn history_entry(row: &turso::Row, previous: &mut i64) -> io::Result<PersistedHistoryEntry> {
    let entry: PersistedHistoryEntry = decode(&row.get::<Vec<u8>>(1).map_err(error)?)?;
    validate_entry(&entry, previous)?;
    if entry.id as i64 != row.get::<i64>(0).map_err(error)?
        || entry.global_seq as i64 != row.get::<i64>(2).map_err(error)?
        || entry.parent_id as i64 != row.get::<i64>(3).map_err(error)?
        || entry.edited_at as i64 != row.get::<i64>(4).map_err(error)?
    {
        return Err(invalid("history key does not match entry ID"));
    }
    Ok(entry)
}

impl Database {
    pub(super) async fn read_history(&self, id: &str) -> io::Result<Vec<PersistedHistoryEntry>> {
        let entries = self.map_history(id, Ok).await?;
        validate(&entries).map_err(|cause| damaged(id, cause))?;
        Ok(entries)
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
            "SELECT seq, entry, global_seq, parent_id, edited_at FROM history WHERE temp_id=? ORDER BY seq",
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
        let now = entries.iter().find(|e| e.current).map_or(0, |e| e.id);
        let tip = entries.iter().find(|e| e.selected_tip).map_or(0, |e| e.id);
        let latest = entries
            .iter()
            .filter(|e| e.source != crate::app::domain::PieceSource::Load)
            .max_by_key(|e| e.global_seq);
        let edited_at = latest.map_or(0, |e| e.edited_at);
        let edit_seq = latest.map_or(0, |e| e.global_seq);
        self.execute_cached("INSERT INTO document_history VALUES(?,?,?,?,?) ON CONFLICT(temp_id) DO UPDATE SET now_id=excluded.now_id,tip_id=excluded.tip_id,edited_at=excluded.edited_at,edit_seq=excluded.edit_seq",
            params![id, now as i64, tip as i64, edited_at as i64, edit_seq as i64]).await?;
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
            self.execute_cached("INSERT INTO history(temp_id, seq, global_seq, entry, bytes, parent_id, edited_at) VALUES(?, ?, ?, ?, ?, ?, ?) ON CONFLICT(temp_id, seq) DO UPDATE SET global_seq=excluded.global_seq, entry=excluded.entry, bytes=excluded.bytes, parent_id=excluded.parent_id, edited_at=excluded.edited_at",
                params![id, entry.id as i64, entry.global_seq as i64, bytes.as_slice(), bytes.len() as i64, entry.parent_id as i64, entry.edited_at as i64]).await?;
        }
        Ok(())
    }
}
