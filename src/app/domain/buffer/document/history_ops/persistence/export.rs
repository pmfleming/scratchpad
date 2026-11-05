//! Recovery JSON escapes at most one chunk at a time, including large history entries.
use super::{HistorySnapshot, TextDocument, persist_cursor_range};
use crate::app::domain::buffer::{ByteSpan, PieceHistoryEdit};
use std::io::{self, Write};
impl TextDocument {
    pub(crate) fn write_recovery_history(&self, writer: &mut impl Write) -> io::Result<()> {
        self.history_snapshot().write_json(writer)
    }
}
impl HistorySnapshot {
    fn write_json(&self, writer: &mut impl Write) -> io::Result<()> {
        writer.write_all(b"[")?;
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                writer.write_all(b",")?;
            }
            let mut metadata = serde_json::json!({"id":entry.id,"parent_id":entry.parent_id,"edited_at":entry.edited_at,"current":self.now==entry.id,"selected_tip":self.tip==entry.id,"global_seq":entry.global_seq,"source":entry.source,"summary":entry.summary,"flags":entry.flags,"previous_selection":persist_cursor_range(entry.previous_selection),"next_selection":persist_cursor_range(entry.next_selection)});
            let prefix = serde_json::to_vec(&metadata.take()).map_err(io::Error::other)?;
            writer.write_all(&prefix[..prefix.len() - 1])?;
            writer.write_all(b",\"edits\":[")?;
            let mut hasher = blake3::Hasher::new();
            for (edit_index, edit) in entry.edits.iter().enumerate() {
                if edit_index > 0 {
                    writer.write_all(b",")?;
                }
                match edit {
                    PieceHistoryEdit::Inserted { start_char, span } => {
                        write!(
                            writer,
                            "{{\"inserted\":{{\"start_char\":{start_char},\"inserted_len\":{},\"inserted_payload\":",
                            span.char_len
                        )?;
                        self.write_payload(writer, std::slice::from_ref(span), &mut hasher)?;
                    }
                    PieceHistoryEdit::Deleted { start_char, spans } => {
                        write!(
                            writer,
                            "{{\"deleted\":{{\"start_char\":{start_char},\"deleted_len\":{},\"deleted_payload\":",
                            spans.iter().map(|s| s.char_len).sum::<u64>()
                        )?;
                        self.write_payload(writer, spans, &mut hasher)?;
                    }
                    PieceHistoryEdit::Replaced {
                        start_char,
                        deleted,
                        inserted,
                    } => {
                        write!(
                            writer,
                            "{{\"replaced\":{{\"start_char\":{start_char},\"deleted_len\":{},\"inserted_len\":{},\"deleted_payload\":",
                            deleted.iter().map(|s| s.char_len).sum::<u64>(),
                            inserted.char_len
                        )?;
                        self.write_payload(writer, deleted, &mut hasher)?;
                        writer.write_all(b",\"inserted_payload\":")?;
                        self.write_payload(writer, std::slice::from_ref(inserted), &mut hasher)?;
                    }
                }
                writer.write_all(b"}}")?;
            }
            let fingerprint = u64::from_le_bytes(
                hasher.finalize().as_bytes()[..8]
                    .try_into()
                    .expect("eight digest bytes"),
            );
            write!(writer, "],\"fingerprint\":{fingerprint}}}")?;
        }
        writer.write_all(b"]")
    }
    fn write_payload(
        &self,
        writer: &mut impl Write,
        spans: &[ByteSpan],
        hasher: &mut blake3::Hasher,
    ) -> io::Result<()> {
        writer.write_all(b"\"")?;
        for span in spans {
            self.tree.visit_history_span(*span, |text| {
                hasher.update(text.as_bytes());
                let quoted = serde_json::to_vec(text).map_err(io::Error::other)?;
                writer.write_all(&quoted[1..quoted.len() - 1])
            })?;
        }
        hasher.update(&[255]);
        writer.write_all(b"\"")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::domain::{CharCursor, CursorRange, PersistedHistoryEntry};
    #[test]
    fn streamed_json_matches_export_and_replays_unicode_with_bounded_payload_windows() {
        let mut doc = TextDocument::new("λ\0🦀\r\n".repeat(100_000));
        doc.replace_char_ranges_with_undo(
            &[(0..20, "\"\\\n新".into())],
            CursorRange::default(),
            CursorRange::one(CharCursor::new(4)),
        )
        .unwrap();
        let mut bytes = Vec::new();
        doc.write_recovery_history(&mut bytes).unwrap();
        let rows: Vec<PersistedHistoryEntry> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(rows, doc.try_exported_history().unwrap());
        assert!(rows.iter().all(PersistedHistoryEntry::payloads_valid));
        let mut restored = TextDocument::new(doc.extract_text());
        assert!(restored.restore_exported_history(rows));
        assert_eq!(restored.extract_text(), doc.extract_text());
    }
}
