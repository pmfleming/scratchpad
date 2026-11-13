use super::recovery_text::RecoveredText;
use super::{Database, error, invalid, quarantine::length_damage_id};
use crate::app::domain::TextDocument;
use crate::app::domain::buffer::{PieceHistoryEdit, PieceHistoryEntry};
use crate::app::services::session_store::{
    RestoreStatus, RestoreStatusLevel,
    recovery::{RECOVERY_LIMIT, RecoveryItem, RecoveryPreview},
};
use std::{collections::HashMap, io};
use turso::params;

impl Database {
    pub(in crate::app::services::session_store) async fn recovery_items(
        &self,
    ) -> io::Result<Vec<RecoveryItem>> {
        self.query_collect("SELECT q.id,d.name,d.path,q.reason FROM quarantine q JOIN documents d ON d.id=q.id ORDER BY d.name,q.id", (), |r| Ok(RecoveryItem {
            id: r.get(0).map_err(error)?, name: r.get(1).map_err(error)?,
            path: r.get::<Option<String>>(2).map_err(error)?.as_deref().map(crate::app::path_identity::encoding::decode).transpose()?, reason: r.get(3).map_err(error)?,
        })).await
    }

    pub(in crate::app::services::session_store) async fn recovery_preview(
        &self,
        id: &str,
    ) -> io::Result<RecoveryPreview> {
        let item = self
            .recovery_items()
            .await?
            .into_iter()
            .find(|i| i.id == id)
            .ok_or_else(|| invalid("This document no longer needs recovery."))?;
        let recovered = self.recover_text(id).await?;
        let mut preview = RecoveryPreview {
            item,
            text: recovered.text,
            gaps: recovered.gaps,
            truncated: recovered.truncated,
        };
        let lengths = self
            .query_collect(
                "SELECT bytes FROM document_pieces WHERE id=? AND part=0",
                [id],
                |r| r.get::<i64>(0).map_err(error),
            )
            .await?;
        if preview.gaps == 0 && lengths.first().copied() != Some(preview.text.len() as i64) {
            preview.gaps += 1;
            preview.text.push_str(
                "\n[RECOVERY GAP: stored length differs; completeness could not be verified]\n",
            );
        }
        Ok(preview)
    }

    pub(super) async fn repair_quarantined_lengths(&mut self) -> io::Result<()> {
        for item in self.recovery_items().await? {
            if item.reason == format!("recovery buffer {}: piece text length mismatch", item.id) {
                self.attempt_length_repair(&item.id).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn repair_length_error(&mut self, cause: &io::Error) -> io::Result<bool> {
        let Some(id) = length_damage_id(cause) else {
            return Ok(false);
        };
        self.attempt_length_repair(id).await
    }

    async fn attempt_length_repair(&mut self, id: &str) -> io::Result<bool> {
        match self.repair_length(id).await {
            Err(cause) if self.blocked.is_none() && self.conn.is_autocommit().unwrap_or(false) => {
                let name = self
                    .quarantined
                    .get(id)
                    .map_or(id, |buffer| buffer.name.as_str());
                self.startup_notices.push(RestoreStatus {
                    level: RestoreStatusLevel::Warning,
                    message: format!("Could not repair recovery metadata for {name}: {cause}. Original data remains protected. Open Status → Review recovery."),
                });
                // A successfully rolled-back repair must not prevent healthy documents saving.
                Ok(false)
            }
            result => result,
        }
    }

    async fn verified_length(&self, id: &str) -> io::Result<i64> {
        let recovered = self.recover_text(id).await?;
        if recovered.gaps != 0 || recovered.truncated {
            return Err(invalid("text is incomplete"));
        }
        let tree = self.read_recovery_tree(id).await?;
        if tree.len_bytes() != recovered.text.len() {
            return Err(invalid("piece metrics disagree with text"));
        }
        let positions = self
            .query_collect(
                "SELECT now_id,tip_id FROM document_history WHERE temp_id=?",
                [id],
                |r| {
                    Ok((
                        r.get::<i64>(0).map_err(error)?,
                        r.get::<i64>(1).map_err(error)?,
                    ))
                },
            )
            .await?;
        let (now, tip) = positions
            .first()
            .copied()
            .ok_or_else(|| invalid("history position missing"))?;
        let entries = self.read_reference_entries(id).await?;
        TextDocument::from_references(
            tree,
            entries.clone(),
            u64::try_from(now).map_err(invalid)?,
            u64::try_from(tip).map_err(invalid)?,
        )?;
        verify_history_spans(&recovered, &entries)?;
        verify_replay(&recovered, &entries, now as u64)?;
        Ok(recovered.text.len() as i64)
    }

    async fn repair_length(&mut self, id: &str) -> io::Result<bool> {
        // Verification is read-only. A second fault must never be hidden by fixing a count.
        let verified = self.verified_length(id).await;
        let Ok(bytes) = verified else {
            return Ok(false);
        };
        let rows = self.query_collect("SELECT p.bytes,d.name FROM document_pieces p JOIN documents d ON d.id=p.id WHERE p.id=? AND p.part=0", [id], |r| Ok((r.get::<i64>(0).map_err(error)?,r.get::<String>(1).map_err(error)?))).await?;
        let Some((old_bytes, name)) = rows.first() else {
            return Ok(false);
        };
        if *old_bytes == bytes {
            return Ok(false);
        }
        if !self.conn.is_autocommit().map_err(error)? {
            return Err(invalid("cannot repair inside an unfinished transaction"));
        }
        self.conn
            .execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(error)?;
        let result = async {
            // Additive audit storage: older readers can still open the session. No text,
            // history, or original archive bytes are discarded during repair.
            self.conn.execute_batch("CREATE TABLE IF NOT EXISTS recovery_repairs(document_id TEXT NOT NULL,old_bytes INTEGER NOT NULL,new_bytes INTEGER NOT NULL,piece_body BLOB NOT NULL,piece_digest BLOB NOT NULL,quarantine_payload BLOB,quarantine_reason TEXT,repaired_at INTEGER NOT NULL) STRICT;").await.map_err(error)?;
            self.execute_cached("INSERT INTO recovery_repairs SELECT p.id,p.bytes,?2,p.body,p.digest,q.payload,q.reason,?3 FROM document_pieces p LEFT JOIN quarantine q ON q.id=p.id WHERE p.id=?1 AND p.part=0", params![id,bytes,chrono::Utc::now().timestamp()]).await?;
            self.execute_cached("UPDATE document_pieces SET bytes=?2 WHERE id=?1 AND part=0", params![id,bytes]).await?;
            self.execute_cached("DELETE FROM quarantine WHERE id=?", [id]).await?;
            Ok(())
        }.await;
        self.finish_transaction(result).await?;
        self.quarantined.remove(id);
        self.inventory = None;
        self.startup_notices.push(RestoreStatus { level: RestoreStatusLevel::Info, message: format!("Repaired recovery metadata for {name}. Text and undo history verified; original metadata retained.") });
        Ok(true)
    }
}

/// Bounds the total bytes read while verifying or replaying one document's history.
const VERIFICATION_BUDGET: usize = RECOVERY_LIMIT * 32;

/// Every span referenced by the history must be present as verified text.
fn verify_history_spans(
    recovered: &RecoveredText,
    entries: &[PieceHistoryEntry],
) -> io::Result<()> {
    let mut verified_bytes = 0u64;
    for span in entries
        .iter()
        .flat_map(|e| &e.edits)
        .flat_map(|e| e.spans())
    {
        verified_bytes = verified_bytes.saturating_add(span.byte_len);
        if verified_bytes > VERIFICATION_BUDGET as u64 {
            return Err(invalid("history verification budget exceeded"));
        }
        recovered
            .span(span)
            .ok_or_else(|| invalid("history text could not be verified"))?;
    }
    Ok(())
}

fn verify_replay(
    recovered: &RecoveredText,
    entries: &[PieceHistoryEntry],
    now: u64,
) -> io::Result<()> {
    // Independently replay the route from the empty root to Now. This rules out a
    // missing final metadata part that happens to leave structurally valid pieces.
    let mut text = String::new();
    let mut work = 0usize;
    for entry in route_to(entries, now)?.into_iter().rev() {
        if !entry.flags.replayable {
            return Err(invalid("history is not replayable"));
        }
        for edit in &entry.edits {
            work = work.saturating_add(text.len());
            if work > VERIFICATION_BUDGET {
                return Err(invalid("history verification budget exceeded"));
            }
            replay_edit(recovered, &mut text, edit)?;
        }
    }
    if text != recovered.text {
        return Err(invalid("history and current text disagree"));
    }
    Ok(())
}

/// Entries from `now` back to (excluding) the empty root.
fn route_to(entries: &[PieceHistoryEntry], now: u64) -> io::Result<Vec<&PieceHistoryEntry>> {
    let by_id: HashMap<_, _> = entries.iter().map(|e| (e.id, e)).collect();
    let mut route = Vec::new();
    let mut at = now;
    while at != 0 {
        let entry = by_id
            .get(&at)
            .ok_or_else(|| invalid("history route missing"))?;
        if route.len() >= entries.len() {
            return Err(invalid("history cycle"));
        }
        route.push(*entry);
        at = entry.parent_id;
    }
    Ok(route)
}

fn replay_edit(
    recovered: &RecoveredText,
    text: &mut String,
    edit: &PieceHistoryEdit,
) -> io::Result<()> {
    let mut deleted = String::new();
    for span in edit.deleted_spans() {
        let part = recovered
            .span(*span)
            .ok_or_else(|| invalid("missing deleted text"))?;
        deleted.push_str(&part);
        if deleted.len() > RECOVERY_LIMIT {
            return Err(invalid("history edit exceeds recovery limit"));
        }
    }
    let inserted = match edit.inserted_span() {
        Some(span) => recovered
            .span(span)
            .ok_or_else(|| invalid("missing inserted text"))?,
        None => String::new(),
    };
    let char_start = usize::try_from(edit.start_char()).map_err(invalid)?;
    let start = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .nth(char_start)
        .ok_or_else(|| invalid("history range outside text"))?;
    let end = start
        .checked_add(deleted.len())
        .ok_or_else(|| invalid("history range overflow"))?;
    if text.get(start..end) != Some(deleted.as_str()) {
        return Err(invalid("history does not match text"));
    }
    if text.len() - deleted.len() + inserted.len() > RECOVERY_LIMIT {
        return Err(invalid("history exceeds recovery limit"));
    }
    text.replace_range(start..end, &inserted);
    Ok(())
}
