//! Typed history and logical spans. Derived navigation flags are never stored.
use super::history_rows::HistoryRows;
use super::rows::{byte_span, document_row, group_by_id, signed, unsigned};
use super::{Database, error, invalid, quarantine::damaged};
#[cfg(test)]
use crate::app::domain::PersistedHistoryEntry;
use crate::app::domain::buffer::{
    ByteSpan, PieceBuffer, PieceHistoryEdit, PieceHistoryEdits, PieceHistoryEntry,
    PieceHistoryFlags,
};
use crate::app::domain::{CharCursor, CursorRange, TextDocument};
use crate::app::services::session_store::model::SessionTab;
use std::sync::Arc;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io,
};
use turso::params;

// Casting json_each's untyped value lets Turso seek the TEXT primary-key index.
// Without the cast, each requested document scans the whole history table.
const HISTORY_ENTRY_QUERY: &str = "SELECT seq,parent_id,edited_at,global_seq,source,summary,replayable,previous_primary,previous_primary_next,previous_secondary,previous_secondary_next,next_primary,next_primary_next,next_secondary,next_secondary_next,packed_spans,temp_id FROM json_each(?) AS requested CROSS JOIN history ON history.temp_id=CAST(requested.value AS TEXT) ORDER BY temp_id,seq";
const HISTORY_SPAN_QUERY: &str = "SELECT seq,edit,role,part,start_char,buffer,start_byte,byte_len,char_len,temp_id FROM json_each(?) AS requested CROSS JOIN history_spans ON history_spans.temp_id=CAST(requested.value AS TEXT) ORDER BY temp_id,seq,edit,role,part";
const HISTORY_POSITION_QUERY: &str = "SELECT temp_id,now_id,tip_id,edited_at,edit_seq FROM json_each(?) AS requested CROSS JOIN document_history ON document_history.temp_id=CAST(requested.value AS TEXT)";

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
        let inserted = edit.inserted_span();
        for (role, spans) in [(0, edit.deleted_spans()), (1, inserted.as_slice())] {
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
pub(super) fn fingerprints(
    rows: &HistoryRows,
    previous: Option<(
        &crate::app::domain::buffer::PersistentVector<PieceHistoryEntry>,
        &Arc<[HistoryFingerprint]>,
    )>,
) -> Arc<[HistoryFingerprint]> {
    let entries = rows.0.reference_entries();
    if let Some((old, keys)) = previous
        && entries.shares_storage(old)
    {
        return keys.clone();
    }
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            // A changed leaf is copied by the persistent vector. Shared leaves are
            // immutable, so their complete metadata fingerprint remains valid.
            if let Some((old, keys)) = previous
                && old
                    .get(index)
                    .is_some_and(|old_entry| std::ptr::eq(entry, old_entry))
                && let Some(key) = keys.get(index)
            {
                key.clone()
            } else {
                hash_entry(entry)
            }
        })
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
/// One stored span row: `[edit, role, part, start_char, buffer, start_byte,
/// byte_len, char_len]`. Role 0 is a deleted span and role 1 the inserted span.
#[derive(Clone)]
struct Span([u64; 8]);
/// Entries with more spans than this store them as one packed blob.
const PACK_THRESHOLD: usize = 256;
const PACKED_SPAN_BYTES: usize = 64;

impl Span {
    fn order_key(&self) -> (u64, u64, u64) {
        (self.0[0], self.0[1], self.0[2])
    }
    fn is_well_formed(&self) -> bool {
        let f = &self.0;
        f.iter().all(|&value| signed(value).is_ok()) && f[1] <= 1 && f[4] <= 1 && f[7] <= f[6]
    }
    fn byte_span(&self) -> ByteSpan {
        byte_span([self.0[4], self.0[5], self.0[6], self.0[7]])
    }
    fn read_row(r: &turso::Row) -> io::Result<(u64, Self)> {
        let mut fields = [0; 8];
        for (i, field) in fields.iter_mut().enumerate() {
            *field = unsigned(r.get(i + 1).map_err(error)?)?;
        }
        Ok((unsigned(r.get(0).map_err(error)?)?, Self(fields)))
    }
    fn unpack(packed: &[u8]) -> Option<Vec<Self>> {
        if !packed.len().is_multiple_of(PACKED_SPAN_BYTES) {
            return None;
        }
        let record = |bytes: &[u8]| {
            let mut fields = [0; 8];
            for (field, chunk) in fields.iter_mut().zip(bytes.chunks_exact(8)) {
                let mut le = [0; 8];
                le.copy_from_slice(chunk);
                *field = u64::from_le_bytes(le);
            }
            Self(fields)
        };
        Some(packed.chunks_exact(PACKED_SPAN_BYTES).map(record).collect())
    }
    fn pack(spans: &[Self]) -> Vec<u8> {
        spans
            .iter()
            .flat_map(|s| s.0.iter().flat_map(|v| v.to_le_bytes()))
            .collect()
    }
    /// Parameters for `history_spans(temp_id, seq, ...fields)`.
    fn values(&self, id: &str, seq: u64) -> io::Result<[Result<turso::Value, turso::Error>; 10]> {
        let mut f = [0; 8];
        for (out, value) in f.iter_mut().zip(self.0) {
            *out = signed(value)?;
        }
        Ok(params![
            id,
            signed(seq)?,
            f[0],
            f[1],
            f[2],
            f[3],
            f[4],
            f[5],
            f[6],
            f[7]
        ])
    }
}

pub(super) struct PreparedHistoryEntry {
    entry: PieceHistoryEntry,
    spans: Vec<Span>,
    packed: Option<Vec<u8>>,
}

impl PreparedHistoryEntry {
    /// Flattens the entry's edits into span rows, checking each span against the tree.
    fn new(rows: &HistoryRows, entry: PieceHistoryEntry) -> io::Result<Self> {
        let tree = rows.0.piece_tree();
        let mut spans = Vec::new();
        for (edit_index, edit) in entry.edits.iter().enumerate() {
            let inserted = edit.inserted_span();
            for (role, parts) in [(0, edit.deleted_spans()), (1, inserted.as_slice())] {
                for (part, span) in parts.iter().copied().enumerate() {
                    if tree.span_char_len(span)? != span.char_len {
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
        for value in spans.iter().flat_map(|span| span.0) {
            signed(value)?;
        }
        let packed = (spans.len() > PACK_THRESHOLD).then(|| Span::pack(&spans));
        Ok(Self {
            entry,
            spans,
            packed,
        })
    }
    /// Span rows for `history_spans`; packed entries keep their spans in the history row.
    fn span_values<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Iterator<Item = io::Result<[Result<turso::Value, turso::Error>; 10]>> + 'a {
        let rows = if self.packed.is_none() {
            self.spans.as_slice()
        } else {
            &[]
        };
        rows.iter().map(move |span| span.values(id, self.entry.id))
    }
}

fn decode_history_row(r: &turso::Row) -> io::Result<(PieceHistoryEntry, Option<Vec<u8>>)> {
    let num = |i| unsigned(r.get::<i64>(i).map_err(error)?);
    let selection = |base| -> io::Result<CursorRange> {
        Ok(CursorRange {
            primary: CharCursor {
                index: usize::try_from(num(base)?).map_err(invalid)?,
                prefer_next_row: bool_field(num(base + 1)?)?,
            },
            secondary: CharCursor {
                index: usize::try_from(num(base + 2)?).map_err(invalid)?,
                prefer_next_row: bool_field(num(base + 3)?)?,
            },
        })
    };
    let entry = PieceHistoryEntry {
        id: num(0)?,
        parent_id: num(1)?,
        edited_at: num(2)?,
        global_seq: num(3)?,
        source: super::document_rows::parse_enum(r.get::<String>(4).map_err(error)?)?,
        summary: r.get(5).map_err(error)?,
        flags: PieceHistoryFlags {
            replayable: bool_field(num(6)?)?,
            undone: false,
        },
        previous_selection: selection(7)?,
        next_selection: selection(11)?,
        edits: PieceHistoryEdits::new(),
    };
    Ok((entry, r.get::<Option<Vec<u8>>>(15).map_err(error)?))
}
impl Database {
    pub(super) async fn prime_history_caches(&mut self, ids: &[String]) -> io::Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let encoded = serde_json::to_string(ids).map_err(invalid)?;
        let rows=self.query_collect("SELECT d.id,h.now_id,h.tip_id,h.edited_at,h.edit_seq,EXISTS(SELECT 1 FROM history e WHERE e.temp_id=d.id) FROM documents d LEFT JOIN document_history h ON h.temp_id=d.id WHERE d.id IN (SELECT value FROM json_each(?))",[encoded.as_str()],|r| {
            let id=r.get::<String>(0).map_err(error)?;
            let number=|index|r.get::<Option<i64>>(index).map_err(error)?.ok_or_else(||damaged(&id,"missing history position")).and_then(unsigned);
            Ok((id.clone(),(number(1)?,number(2)?,number(3)?,number(4)?),r.get::<i64>(5).map_err(error)?!=0))
        }).await?;
        let mut known = HashSet::new();
        for (id, positions, has_entries) in rows {
            known.insert(id.clone());
            let entries = if has_entries {
                self.read_history_fingerprints(&id).await?
            } else {
                Vec::new()
            };
            self.history_inputs.remove(&id);
            self.history_cache.insert(id.clone(), entries.into());
            self.history_positions.insert(id, positions);
        }
        for id in ids.iter().filter(|id| !known.contains(*id)) {
            self.history_inputs.remove(id);
            self.history_cache.insert(id.clone(), Arc::from([]));
            self.history_positions.insert(id.clone(), (0, 0, 0, 0));
        }
        Ok(())
    }
    pub(super) fn prepare_history(
        &self,
        id: &str,
        rows: &HistoryRows,
        keys: &Arc<[HistoryFingerprint]>,
    ) -> io::Result<Vec<PreparedHistoryEntry>> {
        let old = self
            .history_cache
            .get(id)
            .ok_or_else(|| invalid("history cache not primed"))?;
        if Arc::ptr_eq(old, keys) {
            return Ok(Vec::new());
        }
        let unchanged = |key: &HistoryFingerprint| {
            old.binary_search_by_key(&key.id, |e| e.id)
                .is_ok_and(|i| old[i] == *key)
        };
        keys.iter()
            .enumerate()
            .filter(|(_, key)| !unchanged(key))
            .map(|(index, _)| {
                PreparedHistoryEntry::new(rows, rows.0.reference_entry(index).clone())
            })
            .collect()
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
                        unsigned(r.get(0).map_err(error)?)?,
                        unsigned(r.get(1).map_err(error)?)?,
                        unsigned(r.get(2).map_err(error)?)?,
                        unsigned(r.get(3).map_err(error)?)?,
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
    pub(super) async fn read_reference_entries(
        &self,
        id: &str,
    ) -> io::Result<Vec<PieceHistoryEntry>> {
        Ok(self
            .read_reference_entries_batch(&[id.to_owned()])
            .await?
            .remove(id)
            .unwrap_or_default())
    }
    async fn read_reference_entries_batch(
        &self,
        ids: &[String],
    ) -> io::Result<HashMap<String, Vec<PieceHistoryEntry>>> {
        let encoded = serde_json::to_string(ids).map_err(invalid)?;
        let rows = self
            .query_collect(HISTORY_ENTRY_QUERY, [encoded.as_str()], |r| {
                document_row(r, 16, decode_history_row)
            })
            .await?;
        let unpacked = self
            .query_collect(HISTORY_SPAN_QUERY, [encoded.as_str()], |r| {
                document_row(r, 9, Span::read_row)
            })
            .await?;
        let mut grouped = group_by_id(rows);
        let mut unpacked = group_by_id(unpacked);
        for id in unpacked.keys() {
            grouped.entry(id.clone()).or_default();
        }
        grouped
            .into_iter()
            .map(|(id, rows)| {
                let spans = unpacked.remove(&id).unwrap_or_default();
                let entries = assemble_entries(&id, rows, spans)?;
                Ok((id, entries))
            })
            .collect()
    }
    #[cfg(test)]
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
        let mut buffers: Vec<_> = tabs
            .iter_mut()
            .flat_map(|t| &mut t.buffers)
            .filter(|buffer| !self.quarantined.contains_key(&buffer.temp_id))
            .collect();
        // Bound decoded history lookahead while amortizing SQL work across documents.
        for batch in buffers.chunks_mut(64) {
            let ids: Vec<_> = batch.iter().map(|buffer| buffer.temp_id.clone()).collect();
            let encoded = serde_json::to_string(&ids).map_err(invalid)?;
            let positions: HashMap<_, _> = self
                .query_collect(HISTORY_POSITION_QUERY, [encoded.as_str()], |r| {
                    document_row(r, 0, |r| {
                        for i in [3, 4] {
                            unsigned(r.get(i).map_err(error)?)?;
                        }
                        Ok((
                            unsigned(r.get(1).map_err(error)?)?,
                            unsigned(r.get(2).map_err(error)?)?,
                        ))
                    })
                })
                .await?
                .into_iter()
                .collect();
            let mut entries = self.read_reference_entries_batch(&ids).await?;
            let mut trees = self.read_reference_trees(&ids).await?;
            for buffer in batch {
                let id = &buffer.temp_id;
                let tree = trees
                    .remove(id)
                    .ok_or_else(|| damaged(id, "required text manifest is missing"))?;
                let &(now, tip) = positions
                    .get(id)
                    .ok_or_else(|| damaged(id, "missing history position"))?;
                buffer.history_document = Some(
                    TextDocument::from_references(
                        tree,
                        entries.remove(id).unwrap_or_default(),
                        now,
                        tip,
                    )
                    .map_err(|e| damaged(id, e))?,
                );
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
            && self
                .history_cache
                .get(id)
                .is_some_and(|old| old.as_ref() == keys)
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
            for key in old.iter() {
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
            self.execute_cached("INSERT INTO history(temp_id,seq,parent_id,edited_at,global_seq,source,summary,replayable,bytes,previous_primary,previous_primary_next,previous_secondary,previous_secondary_next,next_primary,next_primary_next,next_secondary,next_secondary_next,packed_spans) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(temp_id,seq) DO UPDATE SET parent_id=excluded.parent_id,edited_at=excluded.edited_at,global_seq=excluded.global_seq,source=excluded.source,summary=excluded.summary,replayable=excluded.replayable,bytes=excluded.bytes,previous_primary=excluded.previous_primary,previous_primary_next=excluded.previous_primary_next,previous_secondary=excluded.previous_secondary,previous_secondary_next=excluded.previous_secondary_next,next_primary=excluded.next_primary,next_primary_next=excluded.next_primary_next,next_secondary=excluded.next_secondary,next_secondary_next=excluded.next_secondary_next,packed_spans=excluded.packed_spans", prepared.values(id)?).await?;
            self.execute_cached(
                "DELETE FROM history_spans WHERE temp_id=? AND seq=?",
                params![id, signed(e.id)?],
            )
            .await?;
            for values in prepared.span_values(id) {
                self.execute_cached(
                    "INSERT INTO history_spans VALUES(?,?,?,?,?,?,?,?,?,?)",
                    values?,
                )
                .await?;
            }
        }
        self.execute_cached("DELETE FROM history_nodes WHERE temp_id=? AND node_id<>0 AND node_id NOT IN (SELECT seq FROM history WHERE temp_id=?)",params![id,id]).await
    }
}
fn bool_field(n: u64) -> io::Result<bool> {
    match n {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("invalid history boolean")),
    }
}

type HistoryRow = (PieceHistoryEntry, Option<Vec<u8>>);

/// Attaches each entry's packed or row spans. Every span must belong to an entry.
fn assemble_entries(
    id: &str,
    rows: Vec<HistoryRow>,
    unpacked: Vec<(u64, Span)>,
) -> io::Result<Vec<PieceHistoryEntry>> {
    let mut spans: HashMap<u64, Vec<Span>> = HashMap::new();
    for (entry, packed) in &rows {
        if let Some(packed) = packed {
            let decoded =
                Span::unpack(packed).ok_or_else(|| damaged(id, "invalid packed span length"))?;
            spans.insert(entry.id, decoded);
        }
    }
    let packed_ids: HashSet<u64> = spans.keys().copied().collect();
    for (seq, span) in unpacked {
        if packed_ids.contains(&seq) {
            return Err(damaged(id, "history has packed and row spans"));
        }
        spans.entry(seq).or_default().push(span);
    }
    let entries = rows
        .into_iter()
        .map(|(mut entry, _)| {
            entry.edits = decode_spans(spans.remove(&entry.id).unwrap_or_default())
                .map_err(|e| damaged(id, e))?;
            Ok(entry)
        })
        .collect::<io::Result<Vec<_>>>()?;
    if !spans.is_empty() {
        return Err(damaged(id, "orphan history spans"));
    }
    Ok(entries)
}

/// The spans of one edit while its rows are decoded.
struct EditParts {
    start_char: u64,
    deleted: Vec<ByteSpan>,
    inserted: Option<ByteSpan>,
}

impl EditParts {
    fn add(&mut self, span: &Span) -> io::Result<()> {
        let [_, role, part, start_char, ..] = span.0;
        if self.start_char != start_char {
            return Err(invalid("inconsistent edit start"));
        }
        if role == 0 {
            if part != self.deleted.len() as u64 {
                return Err(invalid("noncontiguous deleted spans"));
            }
            self.deleted.push(span.byte_span());
        } else {
            if part != 0 || self.inserted.is_some() {
                return Err(invalid("duplicate inserted span"));
            }
            self.inserted = Some(span.byte_span());
        }
        Ok(())
    }
    fn into_edit(self) -> io::Result<PieceHistoryEdit> {
        let Self {
            start_char,
            deleted,
            inserted,
        } = self;
        match (deleted.is_empty(), inserted) {
            (true, Some(span)) => Ok(PieceHistoryEdit::Inserted { start_char, span }),
            (false, None) => Ok(PieceHistoryEdit::Deleted {
                start_char,
                spans: deleted,
            }),
            (false, Some(inserted)) => Ok(PieceHistoryEdit::Replaced {
                start_char,
                deleted,
                inserted,
            }),
            (true, None) => Err(invalid("empty history edit")),
        }
    }
}

fn decode_spans(spans: Vec<Span>) -> io::Result<PieceHistoryEdits> {
    let mut edits: BTreeMap<u64, EditParts> = BTreeMap::new();
    let mut previous = None;
    for span in spans {
        let key = span.order_key();
        if previous.is_some_and(|previous| previous >= key) || !span.is_well_formed() {
            return Err(invalid("invalid history span ordering or metrics"));
        }
        previous = Some(key);
        edits
            .entry(span.0[0])
            .or_insert_with(|| EditParts {
                start_char: span.0[3],
                deleted: Vec::new(),
                inserted: None,
            })
            .add(&span)?;
    }
    let mut result = PieceHistoryEdits::new();
    for (index, parts) in edits {
        if index != result.len() as u64 {
            return Err(invalid("noncontiguous history edits"));
        }
        result.push(parts.into_edit()?);
    }
    Ok(result)
}

impl PreparedHistoryEntry {
    fn values(&self, id: &str) -> io::Result<[Result<turso::Value, turso::Error>; 18]> {
        let prepared = self;
        let e = &prepared.entry;
        let p = e.previous_selection;
        let n = e.next_selection;
        let bytes = e
            .edits
            .iter()
            .flat_map(PieceHistoryEdit::spans)
            .try_fold(0u64, |sum, s| {
                sum.checked_add(s.byte_len)
                    .ok_or_else(|| invalid("history byte count overflow"))
            })?;

        Ok(params![
            id,
            signed(e.id)?,
            signed(e.parent_id)?,
            signed(e.edited_at)?,
            signed(e.global_seq)?,
            super::document_rows::enum_name(e.source)?,
            e.summary.as_str(),
            i64::from(e.flags.replayable),
            signed(bytes)?,
            i64::try_from(p.primary.index).map_err(invalid)?,
            i64::from(p.primary.prefer_next_row),
            i64::try_from(p.secondary.index).map_err(invalid)?,
            i64::from(p.secondary.prefer_next_row),
            i64::try_from(n.primary.index).map_err(invalid)?,
            i64::from(n.primary.prefer_next_row),
            i64::try_from(n.secondary.index).map_err(invalid)?,
            i64::from(n.secondary.prefer_next_row),
            prepared.packed.as_deref()
        ])
    }
}

pub(super) struct NewHistory<'a> {
    pub id: &'a str,
    pub rows: Option<&'a HistoryRows>,
    pub entries: &'a [PreparedHistoryEntry],
}

impl Database {
    pub(super) async fn insert_histories(&self, histories: &[NewHistory<'_>]) -> io::Result<()> {
        if histories
            .iter()
            .any(|h| h.rows.map_or(0, HistoryRows::len) != h.entries.len())
        {
            return Err(invalid("new document history is incomplete"));
        }
        self.insert_rows(
            "INSERT INTO history_nodes",
            histories.iter().flat_map(|h| {
                std::iter::once(Ok(params![h.id, 0i64])).chain(
                    h.entries
                        .iter()
                        .map(|e| Ok(params![h.id, signed(e.entry.id)?])),
                )
            }),
        )
        .await?;
        self.insert_rows(
            "INSERT INTO document_history",
            histories.iter().map(|h| {
                let p = h.rows.map_or((0, 0, 0, 0), HistoryRows::positions);
                Ok(params![
                    h.id,
                    signed(p.0)?,
                    signed(p.1)?,
                    signed(p.2)?,
                    signed(p.3)?
                ])
            }),
        )
        .await?;
        self.insert_rows("INSERT INTO history(temp_id,seq,parent_id,edited_at,global_seq,source,summary,replayable,bytes,previous_primary,previous_primary_next,previous_secondary,previous_secondary_next,next_primary,next_primary_next,next_secondary,next_secondary_next,packed_spans)", histories.iter().flat_map(|h| h.entries.iter().map(|entry| entry.values(h.id)))).await?;
        self.insert_rows(
            "INSERT INTO history_spans",
            histories
                .iter()
                .flat_map(|h| h.entries.iter().flat_map(|e| e.span_values(h.id))),
        )
        .await
    }
}

#[cfg(test)]
mod batch_plan_tests {
    #[test]
    fn batch_history_queries_seek_document_indexes() {
        futures_lite::future::block_on(async {
            use super::super::test_fixture::{assert_queries_seek_indexes, database};
            let (_root, db) = database().await;
            let queries = [
                ("history", super::HISTORY_ENTRY_QUERY),
                ("history_spans", super::HISTORY_SPAN_QUERY),
                ("document_history", super::HISTORY_POSITION_QUERY),
            ];
            assert_queries_seek_indexes(&db, &queries).await;
        });
    }
}

#[cfg(test)]
mod span_decoding_tests {
    use super::{PACKED_SPAN_BYTES, Span, decode_spans};
    use crate::app::domain::buffer::PieceHistoryEdit;

    /// `[edit, role, part, start_char, buffer, start_byte, byte_len, char_len]`.
    fn span(fields: [u64; 8]) -> Span {
        Span(fields)
    }

    #[test]
    fn spans_round_trip_through_packing_and_reject_partial_records() {
        let spans = [
            span([0, 0, 0, 3, 0, 1, 2, 2]),
            span([0, 1, 0, 3, 1, 0, 4, 1]),
        ];
        let packed = Span::pack(&spans);
        assert_eq!(packed.len(), 2 * PACKED_SPAN_BYTES);
        let unpacked = Span::unpack(&packed).unwrap();
        assert_eq!(
            unpacked.iter().map(|s| s.0).collect::<Vec<_>>(),
            spans.iter().map(|s| s.0).collect::<Vec<_>>()
        );
        assert!(Span::unpack(&packed[..PACKED_SPAN_BYTES - 1]).is_none());
    }

    #[test]
    fn decoded_spans_form_inserted_deleted_and_replaced_edits() {
        let edits = decode_spans(vec![
            span([0, 1, 0, 0, 1, 0, 3, 3]),
            span([1, 0, 0, 5, 0, 0, 2, 2]),
            span([1, 0, 1, 5, 0, 4, 1, 1]),
            span([2, 0, 0, 9, 0, 6, 1, 1]),
            span([2, 1, 0, 9, 1, 3, 2, 1]),
        ])
        .unwrap();
        let kinds: Vec<_> = edits
            .iter()
            .map(|edit| match edit {
                PieceHistoryEdit::Inserted { .. } => "inserted",
                PieceHistoryEdit::Deleted { spans, .. } if spans.len() == 2 => "deleted",
                PieceHistoryEdit::Replaced { .. } => "replaced",
                PieceHistoryEdit::Deleted { .. } => "unexpected",
            })
            .collect();
        assert_eq!(kinds, ["inserted", "deleted", "replaced"]);
    }

    #[test]
    fn malformed_span_rows_are_rejected() {
        for (spans, reason) in [
            (vec![span([0, 2, 0, 0, 0, 0, 1, 1])], "role above 1"),
            (vec![span([0, 0, 0, 0, 2, 0, 1, 1])], "buffer above 1"),
            (
                vec![span([0, 0, 0, 0, 0, 0, 1, 2])],
                "more chars than bytes",
            ),
            (
                vec![span([0, 0, 0, u64::MAX, 0, 0, 1, 1])],
                "unsigned overflow",
            ),
            (
                vec![
                    span([0, 0, 1, 0, 0, 0, 1, 1]),
                    span([0, 0, 0, 0, 0, 0, 1, 1]),
                ],
                "out of order",
            ),
            (
                vec![
                    span([0, 0, 0, 0, 0, 0, 1, 1]),
                    span([0, 0, 1, 7, 0, 1, 1, 1]),
                ],
                "inconsistent edit start",
            ),
            (
                vec![span([0, 0, 1, 0, 0, 0, 1, 1])],
                "noncontiguous deleted part",
            ),
            (
                vec![span([0, 1, 1, 0, 1, 0, 1, 1])],
                "inserted part above 0",
            ),
            (vec![span([1, 1, 0, 0, 1, 0, 1, 1])], "noncontiguous edits"),
        ] {
            assert!(decode_spans(spans).is_err(), "{reason}");
        }
    }
}
