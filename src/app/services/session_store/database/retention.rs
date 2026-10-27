//! Retention decisions are prepared without writes; compaction and GC commit
//! together with the live session. Pins and quarantine are always recovery roots.
use super::{Database, closed::ClosedRow, decode, error};
use crate::app::{
    domain::TextDocument,
    services::{
        session_store::{
            capture::{CapturedClosedTab, CapturedRecoveryPoint},
            model::SessionTab,
        },
        settings_store::RecoverySettings,
    },
};
use std::{collections::HashSet, io};

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}

impl Database {
    pub(super) async fn apply_archive_policy(
        &self,
        archives: &mut Vec<CapturedClosedTab>,
        policy: &RecoverySettings,
    ) -> io::Result<Vec<i64>> {
        let cutoff = now_seconds().saturating_sub(i64::from(policy.archive_retention_days) * 86400);
        let protected = |archive: &CapturedClosedTab| {
            policy.pinned_archives.contains(&archive.token)
                || archive
                    .tab
                    .session_tab
                    .buffers
                    .iter()
                    .any(|buffer| self.quarantined.contains_key(&buffer.temp_id))
        };
        archives.retain(|archive| {
            protected(archive) || policy.archive_retention_days == 0 || archive.closed_at >= cutoff
        });
        let mut costs = Vec::new();
        for archive in archives.iter() {
            costs.push(self.archive_cost(archive).await?);
        }
        let mut total: u64 = costs.iter().sum();
        // Prefer reducing granularity over deleting whole archives.
        for (index, archive) in archives.iter_mut().enumerate() {
            if total <= policy.archive_byte_budget {
                break;
            }
            if protected(archive) {
                continue;
            }
            let saved = self.compact_archived_undo(archive).await?;
            costs[index] = costs[index].saturating_sub(saved);
            total = total.saturating_sub(saved);
        }
        let mut expired_versions = Vec::new();
        for (index, archive) in archives.iter().enumerate() {
            if total <= policy.archive_byte_budget {
                break;
            }
            if protected(archive) {
                continue;
            }
            for buffer in &archive.tab.session_tab.buffers {
                let points = self.version_history(&buffer.temp_id).await?;
                // Thin interior points before giving up an entire archive. Keep
                // its earliest and latest state until whole-archive expiry.
                for point in points
                    .iter()
                    .skip(1)
                    .take(points.len().saturating_sub(2))
                    .rev()
                {
                    if total <= policy.archive_byte_budget {
                        break;
                    }
                    expired_versions.push(point.id);
                    costs[index] = costs[index].saturating_sub(point.bytes as u64);
                    total = total.saturating_sub(point.bytes as u64);
                }
            }
        }
        let mut index = 0;
        archives.retain(|archive| {
            let cost = costs[index];
            index += 1;
            if total > policy.archive_byte_budget && !protected(archive) {
                total = total.saturating_sub(cost);
                false
            } else {
                true
            }
        });
        Ok(expired_versions)
    }

    async fn archive_cost(&self, archive: &CapturedClosedTab) -> io::Result<u64> {
        let mut bytes = serde_json::to_vec(&archive.tab.session_tab)
            .map_err(error)?
            .len() as u64;
        for buffer in &archive.tab.session_tab.buffers {
            let rows = self.query_collect(
                "SELECT coalesce((SELECT bytes FROM snapshots WHERE id=?1),0), coalesce((SELECT sum(length(entry)) FROM history WHERE temp_id=?1),0), coalesce((SELECT sum(bytes) FROM buffer_versions WHERE temp_id=?1),0)",
                [buffer.temp_id.as_str()], |row| Ok((row.get::<i64>(0).map_err(error)? as u64, row.get::<i64>(1).map_err(error)? as u64, row.get::<i64>(2).map_err(error)? as u64)),
            ).await?;
            let (current, history, versions) = rows[0];
            // A supplied snapshot replaces the existing live snapshot; it isn't
            // a second logical copy. New history is already in captured metadata.
            bytes += archive
                .tab
                .buffer_snapshots
                .iter()
                .find(|snapshot| snapshot.temp_id == buffer.temp_id)
                .map_or(current, |snapshot| snapshot.snapshot.len_bytes() as u64);
            bytes += versions;
            if archive.tab.preserve_existing {
                bytes += history;
            }
        }
        Ok(bytes)
    }

    async fn compact_archived_undo(&self, archive: &mut CapturedClosedTab) -> io::Result<u64> {
        // Pending captures retain their complete history until the next pass.
        if !archive.tab.preserve_existing {
            return Ok(0);
        }
        let mut saved = 0u64;
        let contents = self
            .read_tab_contents(std::slice::from_ref(&archive.tab.session_tab))
            .await?;
        for buffer in &mut archive.tab.session_tab.buffers {
            let entries = self.read_history(&buffer.temp_id).await?;
            if entries.is_empty() {
                continue;
            }
            let Some(text) = contents.get(&buffer.temp_id) else {
                return Ok(0);
            };
            let bytes = serde_json::to_vec(&entries).map_err(error)?.len() as u64;
            let mut document = TextDocument::new(text.clone());
            document.restore_exported_history(entries);
            document.drop_oldest_history_entry();
            if let Some((sequence, snapshot)) = document.recovery_snapshot() {
                saved += bytes.saturating_sub(snapshot.len_bytes() as u64);
                archive.tab.recovery_points.push(CapturedRecoveryPoint {
                    temp_id: buffer.temp_id.clone(),
                    sequence,
                    snapshot,
                });
            }
        }
        archive.tab.discard_history = true;
        Ok(saved)
    }

    pub(super) async fn prune_recovery_points(
        &self,
        policy: &RecoverySettings,
        archives: &[ClosedRow],
        removed: &mut HashSet<Vec<u8>>,
    ) -> io::Result<()> {
        let mut protected: HashSet<String> = self.quarantined.keys().cloned().collect();
        for archive in archives
            .iter()
            .filter(|archive| policy.pinned_archives.contains(&archive.token))
        {
            let tab: SessionTab = decode(&archive.payload)?;
            protected.extend(tab.buffers.into_iter().map(|buffer| buffer.temp_id));
        }
        // Interior periodic points go first; retain boundary/endpoint coverage
        // longer. Only when necessary do oldest endpoints expire as well.
        let points = self.query_collect(
            "SELECT id,temp_id,bytes FROM buffer_versions v ORDER BY (id=(SELECT min(id) FROM buffer_versions WHERE temp_id=v.temp_id) OR id=(SELECT max(id) FROM buffer_versions WHERE temp_id=v.temp_id)), source_token IS NOT NULL, created_at, id",
            (), |row| Ok((row.get::<i64>(0).map_err(error)?, row.get::<String>(1).map_err(error)?, row.get::<i64>(2).map_err(error)? as u64)),
        ).await?;
        let mut total: u64 = points.iter().map(|(_, _, bytes)| bytes + 96).sum();
        for (id, buffer, bytes) in points {
            if total <= policy.snapshot_byte_budget {
                break;
            }
            if protected.contains(&buffer) {
                continue;
            }
            self.collect_digests(
                "SELECT digest FROM version_parts WHERE version_id=?",
                [id],
                removed,
            )
            .await?;
            self.execute_cached("DELETE FROM buffer_versions WHERE id=?", [id])
                .await?;
            total = total.saturating_sub(bytes + 96);
        }
        Ok(())
    }
}
