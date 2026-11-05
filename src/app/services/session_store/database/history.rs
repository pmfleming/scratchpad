//! `seq` is the stable per-document entry ID, NOT global_seq (which changes
//! when typing coalesces). Position markers also mutate existing entries. Fingerprint the complete serialized row; retain no undo
//! payload copies in the worker's comparison cache.
use super::history_rows::HistoryRows;
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

pub(super) fn fingerprints(entries: &HistoryRows) -> io::Result<Vec<HistoryFingerprint>> {
    (0..entries.len())
        .map(|index| fingerprint(&entries.entry(index)?))
        .collect()
}

fn validate_entry(entry: &PersistedHistoryEntry, previous: &mut i64) -> io::Result<()> {
    let seq = i64::try_from(entry.id).map_err(invalid)?;
    if seq <= *previous || i64::try_from(entry.global_seq).map_err(invalid)? <= 0 {
        return Err(invalid(
            "history IDs must increase and global sequences must be positive",
        ));
    }
    if !entry.payloads_valid() {
        return Err(invalid("history payload is missing or corrupt"));
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
    // Reference contract: node 0 is the implicit empty root, never a history row.
    // Every other parent, Now and tip must refer to a node in this document.
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
        || serde_json::to_value(entry.source)
            .map_err(invalid)?
            .as_str()
            != Some(row.get::<String>(5).map_err(error)?.as_str())
        || entry.summary != row.get::<String>(6).map_err(error)?
    {
        return Err(invalid("history key does not match entry ID"));
    }
    Ok(entry)
}

impl Database {
    #[cfg(test)]
    pub(super) async fn read_history(&self, id: &str) -> io::Result<Vec<PersistedHistoryEntry>> {
        let entries = self.map_history(id, Ok).await?;
        validate(&entries).map_err(|cause| damaged(id, cause))?;
        let positions = self
            .query_collect(
                "SELECT now_id,tip_id FROM document_history WHERE temp_id=?",
                [id],
                |row| {
                    Ok((
                        row.get::<i64>(0).map_err(error)?,
                        row.get::<i64>(1).map_err(error)?,
                    ))
                },
            )
            .await?;
        let expected = (
            entries
                .iter()
                .find(|e| e.current)
                .map_or(0, |e| e.id as i64),
            entries
                .iter()
                .find(|e| e.selected_tip)
                .map_or(0, |e| e.id as i64),
        );
        if positions.is_empty() && entries.is_empty() {
            let known = self
                .query_collect("SELECT id FROM documents WHERE id=?", [id], |row| {
                    row.get::<String>(0).map_err(error)
                })
                .await?;
            if known.is_empty() {
                return Ok(entries);
            }
        }
        if positions.first().copied() != Some(expected) {
            return Err(damaged(id, "history position is missing or inconsistent"));
        }
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
            "SELECT seq, entry, global_seq, parent_id, edited_at, source, summary FROM history WHERE temp_id=? ORDER BY seq",
            [id],
            |row| {
                history_entry(row, &mut previous)
                    .and_then(&mut map)
                    .map_err(|cause| damaged(id, cause))
            },
        )
        .await
    }

    async fn read_history_document(
        &self,
        id: &str,
    ) -> io::Result<Option<crate::app::domain::TextDocument>> {
        let positions = self
            .query_collect(
                "SELECT now_id,tip_id FROM document_history WHERE temp_id=?",
                [id],
                |row| {
                    Ok((
                        row.get::<i64>(0).map_err(error)?,
                        row.get::<i64>(1).map_err(error)?,
                    ))
                },
            )
            .await?;
        let Some(&(now, tip)) = positions.first() else {
            // An untracked cold shell can hydrate directly from its source file.
            let known = self
                .query_collect("SELECT id FROM documents WHERE id=?", [id], |row| {
                    row.get::<String>(0).map_err(error)
                })
                .await?;
            return if known.is_empty() {
                Ok(None)
            } else {
                Err(damaged(id, "missing history position"))
            };
        };
        let mut document = crate::app::domain::TextDocument::new(String::new());
        let mut rows = self.conn.query("SELECT seq,entry,global_seq,parent_id,edited_at,source,summary FROM history WHERE temp_id=? ORDER BY seq", [id]).await.map_err(error)?;
        let mut previous = 0;
        let (mut marked_now, mut marked_tip) = (0, 0);
        while let Some(row) = rows.next().await.map_err(error)? {
            let entry = history_entry(&row, &mut previous).map_err(|cause| damaged(id, cause))?;
            if entry.current {
                if marked_now != 0 {
                    return Err(damaged(id, "multiple history cursors"));
                }
                marked_now = entry.id as i64;
            }
            if entry.selected_tip {
                if marked_tip != 0 {
                    return Err(damaged(id, "multiple selected branches"));
                }
                marked_tip = entry.id as i64;
            }
            document.import_history_row(entry);
        }
        if previous == 0 && now == 0 && tip == 0 {
            return Ok(None);
        }
        if (marked_now, marked_tip) != (now, tip)
            || !document.finish_history_import(now as u64, tip as u64)
        {
            return Err(damaged(id, "invalid history position or replay"));
        }
        Ok(Some(document))
    }

    pub(super) async fn read_histories(&self, tabs: &mut [SessionTab]) -> io::Result<()> {
        for buffer in tabs.iter_mut().flat_map(|tab| &mut tab.buffers) {
            if !self.quarantined.contains_key(&buffer.temp_id) {
                buffer.history_document = self.read_history_document(&buffer.temp_id).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn write_history(
        &self,
        id: &str,
        entries: &HistoryRows,
        fingerprints: &[HistoryFingerprint],
    ) -> io::Result<()> {
        let old = self
            .history_cache
            .get(id)
            .ok_or_else(|| invalid("history cache not primed"))?;
        self.execute_cached(
            "INSERT INTO history_nodes VALUES(?,0) ON CONFLICT DO NOTHING",
            [id],
        )
        .await?;
        if old == fingerprints {
            if entries.len() == 0 {
                self.execute_cached("INSERT INTO document_history VALUES(?,0,0,0,0) ON CONFLICT(temp_id) DO NOTHING", [id]).await?;
            }
            return Ok(());
        }
        let (now, tip, edited_at, edit_seq) = entries.positions();
        for fingerprint in fingerprints {
            if old.binary_search_by_key(&fingerprint.id, |e| e.id).is_err() {
                self.execute_cached(
                    "INSERT INTO history_nodes VALUES(?,?) ON CONFLICT DO NOTHING",
                    params![id, fingerprint.id as i64],
                )
                .await?;
            }
        }
        self.execute_cached("INSERT INTO document_history VALUES(?,?,?,?,?) ON CONFLICT(temp_id) DO UPDATE SET now_id=excluded.now_id,tip_id=excluded.tip_id,edited_at=excluded.edited_at,edit_seq=excluded.edit_seq",
            params![id, now as i64, tip as i64, edited_at as i64, edit_seq as i64]).await?;
        let Some((first, last)) = fingerprints.first().zip(fingerprints.last()) else {
            self.execute_cached("DELETE FROM history WHERE temp_id=?", [id])
                .await?;
            return self
                .execute_cached(
                    "DELETE FROM history_nodes WHERE temp_id=? AND node_id<>0",
                    [id],
                )
                .await;
        };
        let last = last.id;
        // Explicit Forget history can replace the entire tree, including IDs.
        self.execute_cached(
            "DELETE FROM history WHERE temp_id=? AND (seq<? OR seq>?)",
            params![id, first.id as i64, last as i64],
        )
        .await?;
        for entry in old
            .iter()
            .filter(|entry| entry.id >= first.id && entry.id <= last)
        {
            if fingerprints
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
        for (index, fingerprint) in fingerprints.iter().enumerate() {
            if old
                .binary_search_by_key(&fingerprint.id, |entry| entry.id)
                .ok()
                .is_some_and(|index| old[index] == *fingerprint)
            {
                continue;
            }
            let entry = entries.entry(index)?;
            let bytes = serde_json::to_vec(&entry).map_err(invalid)?;
            self.execute_cached("INSERT INTO history(temp_id, seq, global_seq, entry, bytes, parent_id, edited_at, source, summary) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(temp_id, seq) DO UPDATE SET global_seq=excluded.global_seq, entry=excluded.entry, bytes=excluded.bytes, parent_id=excluded.parent_id, edited_at=excluded.edited_at,source=excluded.source,summary=excluded.summary",
                params![id, entry.id as i64, entry.global_seq as i64, bytes.as_slice(), bytes.len() as i64, entry.parent_id as i64, entry.edited_at as i64, serde_json::to_value(entry.source).map_err(invalid)?.as_str().ok_or_else(|| invalid("invalid history source"))?, entry.summary.as_str()]).await?;
        }
        self.execute_cached("DELETE FROM history_nodes WHERE temp_id=? AND node_id<>0 AND node_id NOT IN (SELECT seq FROM history WHERE temp_id=?)",params![id,id]).await
    }
}
