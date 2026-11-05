//! Typed history and logical spans. Derived navigation flags are never stored.
use super::history_rows::HistoryRows;
use super::{Database, error, invalid, quarantine::damaged};
#[cfg(test)]
use crate::app::domain::PersistedHistoryEntry;
use crate::app::domain::buffer::PieceBuffer;
use crate::app::domain::buffer::{
    ByteSpan, PieceHistoryEdit, PieceHistoryEntry, PieceHistoryFlags,
};
use crate::app::domain::{CharCursor, CursorRange, TextDocument};
use crate::app::services::session_store::model::SessionTab;
use std::{
    collections::{BTreeMap, HashMap},
    io,
};
use turso::params;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HistoryFingerprint {
    pub id: u64,
    digest: blake3::Hash,
}
fn hash_entry(entry: &PieceHistoryEntry) -> HistoryFingerprint {
    let mut hash = blake3::Hasher::new();
    for value in [
        entry.id,
        entry.parent_id,
        entry.edited_at,
        entry.global_seq,
        u64::from(entry.flags.replayable),
    ] {
        hash.update(&value.to_le_bytes());
    }
    hash.update(
        super::document_rows::enum_name(entry.source)
            .expect("source is a unit enum")
            .as_bytes(),
    );
    hash.update(&(entry.summary.len() as u64).to_le_bytes());
    hash.update(entry.summary.as_bytes());
    for selection in [entry.previous_selection, entry.next_selection] {
        for value in [
            selection.primary.index as u64,
            u64::from(selection.primary.prefer_next_row),
            selection.secondary.index as u64,
            u64::from(selection.secondary.prefer_next_row),
        ] {
            hash.update(&value.to_le_bytes());
        }
    }
    for edit in &entry.edits {
        hash.update(&edit.start_char().to_le_bytes());
        for (role, spans) in [
            (0, edit.deleted_spans().to_vec()),
            (1, edit.inserted_span().into_iter().collect()),
        ] {
            hash.update(&(role as u64).to_le_bytes());
            hash.update(&(spans.len() as u64).to_le_bytes());
            for span in spans {
                for value in [
                    u64::from(span.buffer == PieceBuffer::Add),
                    span.start_byte,
                    span.byte_len,
                    span.char_len,
                ] {
                    hash.update(&value.to_le_bytes());
                }
            }
        }
    }
    HistoryFingerprint {
        id: entry.id,
        digest: hash.finalize(),
    }
}
pub(super) fn fingerprints(rows: &HistoryRows) -> Vec<HistoryFingerprint> {
    (0..rows.len())
        .map(|i| hash_entry(rows.0.reference_entry(i)))
        .collect()
}

#[cfg(test)]
pub(super) fn validate(entries: &[PersistedHistoryEntry]) -> io::Result<()> {
    let mut previous = 0;
    for entry in entries {
        if entry.id <= previous
            || i64::try_from(entry.id).is_err()
            || i64::try_from(entry.global_seq).is_err()
            || entry.global_seq == 0
            || !entry.payloads_valid()
        {
            return Err(invalid("invalid exported history payload or sequence"));
        }
        previous = entry.id;
    }
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
#[derive(Clone)]
struct Span([u64; 8]);
pub(super) struct PreparedHistoryEntry {
    entry: PieceHistoryEntry,
    spans: Vec<Span>,
    packed: Option<Vec<u8>>,
}
impl Database {
    pub(super) async fn prime_history_caches(&mut self, ids: &[String]) -> io::Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let encoded = serde_json::to_string(ids).map_err(invalid)?;
        let rows=self.query_collect("SELECT d.id,h.now_id,h.tip_id,h.edited_at,h.edit_seq,EXISTS(SELECT 1 FROM history e WHERE e.temp_id=d.id) FROM documents d LEFT JOIN document_history h ON h.temp_id=d.id WHERE d.id IN (SELECT value FROM json_each(?))",[encoded.as_str()],|r| {
            let id=r.get::<String>(0).map_err(error)?;
            let number=|index|r.get::<Option<i64>>(index).map_err(error)?.ok_or_else(||damaged(&id,"missing history position")).and_then(super::document_rows::unsigned);
            Ok((id.clone(),(number(1)?,number(2)?,number(3)?,number(4)?),r.get::<i64>(5).map_err(error)?!=0))
        }).await?;
        let mut known = std::collections::HashSet::new();
        for (id, positions, has_entries) in rows {
            known.insert(id.clone());
            let entries = if has_entries {
                self.read_history_fingerprints(&id).await?
            } else {
                Vec::new()
            };
            self.history_cache.insert(id.clone(), entries);
            self.history_positions.insert(id, positions);
        }
        for id in ids.iter().filter(|id| !known.contains(*id)) {
            self.history_cache.insert(id.clone(), Vec::new());
            self.history_positions.insert(id.clone(), (0, 0, 0, 0));
        }
        Ok(())
    }
    pub(super) fn prepare_history(
        &self,
        id: &str,
        rows: &HistoryRows,
        keys: &[HistoryFingerprint],
    ) -> io::Result<Vec<PreparedHistoryEntry>> {
        let old = self
            .history_cache
            .get(id)
            .ok_or_else(|| invalid("history cache not primed"))?;
        let mut changed = Vec::new();
        for (index, key) in keys.iter().enumerate() {
            if old
                .binary_search_by_key(&key.id, |e| e.id)
                .ok()
                .is_some_and(|i| old[i] == *key)
            {
                continue;
            }
            let entry = rows.0.reference_entry(index).clone();
            let mut spans = Vec::new();
            for (edit_index, edit) in entry.edits.iter().enumerate() {
                for (role, parts) in [
                    (0, edit.deleted_spans().to_vec()),
                    (1, edit.inserted_span().into_iter().collect()),
                ] {
                    for (part, span) in parts.into_iter().enumerate() {
                        let measured = rows.0.piece_tree().span_char_len(span)?;
                        if measured != span.char_len {
                            return Err(invalid("history character length mismatch"));
                        }
                        spans.push(Span([
                            edit_index as u64,
                            role,
                            part as u64,
                            edit.start_char(),
                            u64::from(span.buffer == PieceBuffer::Add),
                            span.start_byte,
                            span.byte_len,
                            span.char_len,
                        ]));
                    }
                }
            }
            for span in &spans {
                for value in span.0 {
                    signed(value)?;
                }
            }
            let packed = (spans.len() > 256).then(|| {
                spans
                    .iter()
                    .flat_map(|s| s.0.iter().flat_map(|v| v.to_le_bytes()))
                    .collect()
            });
            changed.push(PreparedHistoryEntry {
                entry,
                spans,
                packed,
            });
        }
        Ok(changed)
    }
    pub(super) async fn read_history_positions(
        &self,
        id: &str,
    ) -> io::Result<(u64, u64, u64, u64)> {
        Ok(self
            .query_collect(
                "SELECT now_id,tip_id,edited_at,edit_seq FROM document_history WHERE temp_id=?",
                [id],
                |r| {
                    Ok((
                        super::document_rows::unsigned(r.get(0).map_err(error)?)?,
                        super::document_rows::unsigned(r.get(1).map_err(error)?)?,
                        super::document_rows::unsigned(r.get(2).map_err(error)?)?,
                        super::document_rows::unsigned(r.get(3).map_err(error)?)?,
                    ))
                },
            )
            .await?
            .pop()
            .unwrap_or_default())
    }
    pub(super) async fn read_history_fingerprints(
        &self,
        id: &str,
    ) -> io::Result<Vec<HistoryFingerprint>> {
        Ok(self
            .read_reference_entries(id)
            .await?
            .iter()
            .map(hash_entry)
            .collect())
    }
    async fn read_reference_entries(&self, id: &str) -> io::Result<Vec<PieceHistoryEntry>> {
        let mut rows=self.query_collect("SELECT seq,parent_id,edited_at,global_seq,source,summary,replayable,previous_primary,previous_primary_next,previous_secondary,previous_secondary_next,next_primary,next_primary_next,next_secondary,next_secondary_next,packed_spans FROM history WHERE temp_id=? ORDER BY seq",[id],|r| {
            let num=|i|super::document_rows::unsigned(r.get::<i64>(i).map_err(error)?);
            let selection=|base|->io::Result<CursorRange>{Ok(CursorRange {primary:CharCursor {index:usize::try_from(num(base)?).map_err(invalid)?,prefer_next_row:bool_field(num(base+1)?)?},secondary:CharCursor {index:usize::try_from(num(base+2)?).map_err(invalid)?,prefer_next_row:bool_field(num(base+3)?)?}})};
            let entry=PieceHistoryEntry {id:num(0)?,parent_id:num(1)?,edited_at:num(2)?,global_seq:num(3)?,source:super::document_rows::parse_enum(r.get::<String>(4).map_err(error)?)?,summary:r.get(5).map_err(error)?,flags:PieceHistoryFlags {replayable:bool_field(num(6)?)?,undone:false},previous_selection:selection(7)?,next_selection:selection(11)?,edits:crate::app::domain::buffer::PieceHistoryEdits::new()};
            Ok((entry,r.get::<Option<Vec<u8>>>(15).map_err(error)?))
        }).await.map_err(|e|damaged(id,e))?;
        let mut spans: HashMap<u64, Vec<Span>> = HashMap::new();
        for (entry, packed) in &rows {
            if let Some(packed) = packed {
                if !packed.len().is_multiple_of(64) {
                    return Err(damaged(id, "invalid packed span length"));
                }
                let mut decoded = Vec::with_capacity(packed.len() / 64);
                for record in packed.chunks_exact(64) {
                    let mut fields = [0; 8];
                    for (field, bytes) in fields.iter_mut().zip(record.chunks_exact(8)) {
                        *field = u64::from_le_bytes(bytes.try_into().expect("eight byte field"));
                    }
                    decoded.push(Span(fields));
                }
                spans.insert(entry.id, decoded);
            }
        }
        let unpacked=self.query_collect("SELECT seq,edit,role,part,start_char,buffer,start_byte,byte_len,char_len FROM history_spans WHERE temp_id=? ORDER BY seq,edit,role,part",[id],|r| {let mut fields=[0;8];for (i,field) in fields.iter_mut().enumerate() {*field=super::document_rows::unsigned(r.get(i+1).map_err(error)?)?;} Ok((super::document_rows::unsigned(r.get(0).map_err(error)?)?,Span(fields)))}).await?;
        let packed_ids: std::collections::HashSet<_> = spans.keys().copied().collect();
        for (seq, span) in unpacked {
            if packed_ids.contains(&seq) {
                return Err(damaged(id, "history has packed and row spans"));
            }
            spans.entry(seq).or_default().push(span);
        }
        for (entry, _) in &mut rows {
            entry.edits = decode_spans(spans.remove(&entry.id).unwrap_or_default())
                .map_err(|e| damaged(id, e))?;
        }
        if !spans.is_empty() {
            return Err(damaged(id, "orphan history spans"));
        }
        Ok(rows.into_iter().map(|(entry, _)| entry).collect())
    }
    pub(super) async fn read_history_document(&self, id: &str) -> io::Result<Option<TextDocument>> {
        let Some(tree) = self.read_reference_tree(id).await? else {
            return Ok(None);
        };
        if self
            .query_collect(
                "SELECT 1 FROM document_history WHERE temp_id=?",
                [id],
                |r| r.get::<i64>(0).map_err(error),
            )
            .await?
            .len()
            != 1
        {
            return Err(damaged(id, "missing history position"));
        }
        let (now, tip, _, _) = self.read_history_positions(id).await?;
        let entries = self.read_reference_entries(id).await?;
        TextDocument::from_references(tree, entries, now, tip)
            .map(Some)
            .map_err(|e| damaged(id, e))
    }
    #[cfg(test)]
    pub(super) async fn read_history(&self, id: &str) -> io::Result<Vec<PersistedHistoryEntry>> {
        self.read_history_document(id)
            .await?
            .map_or(Ok(Vec::new()), |d| d.try_exported_history())
    }
    pub(super) async fn read_histories(&self, tabs: &mut [SessionTab]) -> io::Result<()> {
        for buffer in tabs.iter_mut().flat_map(|t| &mut t.buffers) {
            if !self.quarantined.contains_key(&buffer.temp_id) {
                buffer.history_document = self.read_history_document(&buffer.temp_id).await?;
                if buffer.history_document.is_none() {
                    return Err(damaged(
                        &buffer.temp_id,
                        "required text manifest is missing",
                    ));
                }
            }
        }
        Ok(())
    }
    async fn write_history_position(
        &self,
        id: &str,
        positions: (u64, u64, u64, u64),
    ) -> io::Result<()> {
        if self.history_positions.get(id) != Some(&positions)
            || !self.document_cache.contains_key(id)
        {
            self.execute_cached("INSERT INTO document_history VALUES(?,?,?,?,?) ON CONFLICT(temp_id) DO UPDATE SET now_id=excluded.now_id,tip_id=excluded.tip_id,edited_at=excluded.edited_at,edit_seq=excluded.edit_seq",params![id,signed(positions.0)?,signed(positions.1)?,signed(positions.2)?,signed(positions.3)?]).await?;
        }
        Ok(())
    }
    pub(super) async fn write_history(
        &self,
        id: &str,
        rows: &HistoryRows,
        keys: &[HistoryFingerprint],
        changed: &[PreparedHistoryEntry],
    ) -> io::Result<()> {
        if self.document_cache.contains_key(id)
            && self.history_cache.get(id).is_some_and(|old| old == keys)
        {
            return self.write_history_position(id, rows.positions()).await;
        }
        self.execute_cached(
            "INSERT INTO history_nodes VALUES(?,0) ON CONFLICT DO NOTHING",
            [id],
        )
        .await?;
        for key in keys {
            if self
                .history_cache
                .get(id)
                .is_none_or(|old| old.binary_search_by_key(&key.id, |e| e.id).is_err())
            {
                self.execute_cached(
                    "INSERT INTO history_nodes VALUES(?,?) ON CONFLICT DO NOTHING",
                    params![id, signed(key.id)?],
                )
                .await?;
            }
        }
        self.write_history_position(id, rows.positions()).await?;
        if let Some(old) = self.history_cache.get(id) {
            for key in old {
                if keys.binary_search_by_key(&key.id, |e| e.id).is_err() {
                    self.execute_cached(
                        "DELETE FROM history WHERE temp_id=? AND seq=?",
                        params![id, signed(key.id)?],
                    )
                    .await?;
                }
            }
        }
        for prepared in changed {
            let e = &prepared.entry;
            let p = e.previous_selection;
            let n = e.next_selection;
            let bytes =
                e.edits
                    .iter()
                    .flat_map(PieceHistoryEdit::spans)
                    .try_fold(0u64, |sum, s| {
                        sum.checked_add(s.byte_len)
                            .ok_or_else(|| invalid("history byte count overflow"))
                    })?;
            self.execute_cached("INSERT INTO history(temp_id,seq,parent_id,edited_at,global_seq,source,summary,replayable,bytes,previous_primary,previous_primary_next,previous_secondary,previous_secondary_next,next_primary,next_primary_next,next_secondary,next_secondary_next,packed_spans) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(temp_id,seq) DO UPDATE SET parent_id=excluded.parent_id,edited_at=excluded.edited_at,global_seq=excluded.global_seq,source=excluded.source,summary=excluded.summary,replayable=excluded.replayable,bytes=excluded.bytes,previous_primary=excluded.previous_primary,previous_primary_next=excluded.previous_primary_next,previous_secondary=excluded.previous_secondary,previous_secondary_next=excluded.previous_secondary_next,next_primary=excluded.next_primary,next_primary_next=excluded.next_primary_next,next_secondary=excluded.next_secondary,next_secondary_next=excluded.next_secondary_next,packed_spans=excluded.packed_spans",params![id,signed(e.id)?,signed(e.parent_id)?,signed(e.edited_at)?,signed(e.global_seq)?,super::document_rows::enum_name(e.source)?,e.summary.as_str(),i64::from(e.flags.replayable),signed(bytes)?,i64::try_from(p.primary.index).map_err(invalid)?,i64::from(p.primary.prefer_next_row),i64::try_from(p.secondary.index).map_err(invalid)?,i64::from(p.secondary.prefer_next_row),i64::try_from(n.primary.index).map_err(invalid)?,i64::from(n.primary.prefer_next_row),i64::try_from(n.secondary.index).map_err(invalid)?,i64::from(n.secondary.prefer_next_row),prepared.packed.as_deref()]).await?;
            self.execute_cached(
                "DELETE FROM history_spans WHERE temp_id=? AND seq=?",
                params![id, signed(e.id)?],
            )
            .await?;
            if prepared.packed.is_none() {
                for span in &prepared.spans {
                    let f = span
                        .0
                        .map(signed)
                        .into_iter()
                        .collect::<io::Result<Vec<_>>>()?;
                    self.execute_cached(
                        "INSERT INTO history_spans VALUES(?,?,?,?,?,?,?,?,?,?)",
                        params![
                            id,
                            signed(e.id)?,
                            f[0],
                            f[1],
                            f[2],
                            f[3],
                            f[4],
                            f[5],
                            f[6],
                            f[7]
                        ],
                    )
                    .await?;
                }
            }
        }
        self.execute_cached("DELETE FROM history_nodes WHERE temp_id=? AND node_id<>0 AND node_id NOT IN (SELECT seq FROM history WHERE temp_id=?)",params![id,id]).await
    }
}
fn signed(n: u64) -> io::Result<i64> {
    i64::try_from(n).map_err(invalid)
}
fn bool_field(n: u64) -> io::Result<bool> {
    match n {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("invalid history boolean")),
    }
}
fn decode_spans(spans: Vec<Span>) -> io::Result<crate::app::domain::buffer::PieceHistoryEdits> {
    type EditParts = (u64, Vec<ByteSpan>, Option<ByteSpan>);
    let mut edits: BTreeMap<u64, EditParts> = BTreeMap::new();
    let mut previous = None;
    for Span(f) in spans {
        if previous.is_some_and(|key| key >= (f[0], f[1], f[2]))
            || f.iter().any(|&value| signed(value).is_err())
            || f[1] > 1
            || f[4] > 1
            || f[7] > f[6]
        {
            return Err(invalid("invalid history span ordering or metrics"));
        }
        previous = Some((f[0], f[1], f[2]));
        let edit = edits
            .entry(f[0])
            .or_insert_with(|| (f[3], Vec::new(), None));
        if edit.0 != f[3] {
            return Err(invalid("inconsistent edit start"));
        }
        let span = ByteSpan {
            buffer: if f[4] == 0 {
                PieceBuffer::Original
            } else {
                PieceBuffer::Add
            },
            start_byte: f[5],
            byte_len: f[6],
            char_len: f[7],
        };
        if f[1] == 0 {
            if f[2] != edit.1.len() as u64 {
                return Err(invalid("noncontiguous deleted spans"));
            }
            edit.1.push(span);
        } else {
            if f[2] != 0 || edit.2.is_some() {
                return Err(invalid("duplicate inserted span"));
            }
            edit.2 = Some(span);
        }
    }
    let mut result = crate::app::domain::buffer::PieceHistoryEdits::new();
    for (index, (start_char, deleted, inserted)) in edits {
        if index != result.len() as u64 {
            return Err(invalid("noncontiguous history edits"));
        }
        result.push(match (deleted.is_empty(), inserted) {
            (true, Some(span)) => PieceHistoryEdit::Inserted { start_char, span },
            (false, None) => PieceHistoryEdit::Deleted {
                start_char,
                spans: deleted,
            },
            (false, Some(inserted)) => PieceHistoryEdit::Replaced {
                start_char,
                deleted,
                inserted,
            },
            _ => return Err(invalid("empty history edit")),
        });
    }
    Ok(result)
}
