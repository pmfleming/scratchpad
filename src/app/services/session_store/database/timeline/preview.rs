//! Optional display text only: never hydrate a document or decode a replay log.
use super::{Database, TimelineKind, TimelineRow, error, invalid};
use crate::app::domain::buffer::HistoryPreview;
use std::io;
use turso::params;

impl Database {
    pub(super) async fn timeline_preview(
        &self,
        row: &TimelineRow,
    ) -> io::Result<Option<HistoryPreview>> {
        let TimelineKind::Edit(source) = row.kind else {
            return Ok(None);
        };
        if !HistoryPreview::needed(source, &row.summary) {
            return Ok(None);
        }
        let id = row.cursor.document.as_str();
        // Two rows suffice to reject replacements, multi-range edits and split
        // deletion spans. Packed edits are always larger than this display case.
        let spans = self
            .query_collect(
                "SELECT s.edit,s.part,s.role,s.buffer,s.start_byte,s.byte_len
             FROM history h JOIN history_spans s ON s.temp_id=h.temp_id AND s.seq=h.seq
             WHERE h.temp_id=?1 AND h.seq=?2 AND h.packed_spans IS NULL
             ORDER BY s.edit,s.role,s.part LIMIT 2",
                params![id, i64::try_from(row.cursor.node).map_err(invalid)?],
                |r| {
                    let mut fields = [0i64; 6];
                    for (i, field) in fields.iter_mut().enumerate() {
                        *field = r.get(i).map_err(error)?;
                    }
                    Ok(fields)
                },
            )
            .await?;
        let [[0, 0, role @ (0 | 1), buffer @ (0 | 1), start, len]] = spans.as_slice() else {
            return Ok(None);
        };
        if *start < 0 || !(1..=HistoryPreview::MAX_BYTES as i64).contains(len) {
            return Ok(None);
        }
        let Some(end) = start.checked_add(*len) else {
            return Ok(None);
        };
        // Fetch only the intersecting bytes, including a span crossing sealed
        // chunks or appended tail segments. SQL substr operates on BLOB bytes.
        // Even malformed overlapping manifests cannot return unbounded output.
        let mut parts = self.query_collect(
            "SELECT max(t.start_byte,?3),
                    substr(b.body,max(?3-t.start_byte,0)+1,min(?4,t.start_byte+c.byte_len)-max(?3,t.start_byte))
             FROM document_text t JOIN chunks c ON c.id=t.chunk_id
             JOIN chunk_bodies b ON b.chunk_id=c.id
             WHERE t.temp_id=?1 AND t.buffer=?2 AND t.start_byte<?4 AND t.start_byte+c.byte_len>?3
             ORDER BY t.position LIMIT 257",
            params![id, *buffer, *start, end],
            |r| Ok((r.get::<i64>(0).map_err(error)?,r.get::<Vec<u8>>(1).map_err(error)?)),
        ).await?;
        if *buffer == 1 {
            parts.extend(self.query_collect(
                "SELECT max(start_byte+offset,?2),
                        substr(body,max(?2-start_byte-offset,0)+1,min(?3,start_byte+offset+length(body))-max(?2,start_byte+offset))
                 FROM document_tail
                 WHERE temp_id=?1 AND start_byte+offset<?3 AND start_byte+offset+length(body)>?2
                 ORDER BY offset LIMIT 257",
                params![id, *start, end],
                |r| Ok((r.get::<i64>(0).map_err(error)?,r.get::<Vec<u8>>(1).map_err(error)?)),
            ).await?);
        }
        parts.sort_by_key(|(at, _)| *at);
        let mut bytes = Vec::with_capacity(*len as usize);
        let mut next = *start;
        for (at, part) in parts {
            if at != next || part.is_empty() || part.len() > (end - next) as usize {
                return Ok(None);
            }
            next += part.len() as i64;
            bytes.extend(part);
        }
        if next != end {
            return Ok(None);
        }
        Ok(String::from_utf8(bytes).ok().map(|text| HistoryPreview {
            inserted: *role == 1,
            text,
        }))
    }
}
