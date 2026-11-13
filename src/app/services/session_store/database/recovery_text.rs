//! Best-effort extraction trusts only checksummed metadata and text. Gaps stay explicit.
use super::rows::byte_span;
use super::{Database, error, invalid};
use crate::app::domain::buffer::{ByteSpan, PieceBuffer};
use crate::app::services::session_store::recovery::RECOVERY_LIMIT;
use std::{collections::BTreeMap, io};

/// Stored piece maps are split into parts of this many bytes.
const PIECE_PART: usize = 65536;
/// The map starts with a version header (`1`, little-endian) padded to this size.
const MAP_HEADER: usize = 24;
/// Each piece is seven little-endian u64 fields: buffer, start_byte, byte_len,
/// char_len, newline_count, is_ascii, ends_with_lf.
const PIECE_RECORD: usize = 56;
const TRUNCATED: &str = "remaining text omitted from this 32 MiB recovery copy";

/// Recovered pieces: verified text, or `None` for a missing or damaged region.
type Fragments = Vec<Option<String>>;

pub(super) struct RecoveredText {
    pub text: String,
    pub gaps: usize,
    pub truncated: bool,
    segments: [Vec<(usize, String)>; 2],
}

impl RecoveredText {
    /// The verified segments of the span's buffer and the span's byte range in it.
    fn locate(&self, span: ByteSpan) -> Option<(&[(usize, String)], usize, usize)> {
        let start = usize::try_from(span.start_byte).ok()?;
        let len = usize::try_from(span.byte_len).ok()?;
        if len > RECOVERY_LIMIT {
            return None;
        }
        let segments = &self.segments[usize::from(span.buffer == PieceBuffer::Add)];
        Some((segments, start, start.checked_add(len)?))
    }
    fn fragments(&self, span: ByteSpan) -> Option<Fragments> {
        let (segments, start, end) = self.locate(span)?;
        let mut at = start;
        let mut fragments = Vec::new();
        for (base, body) in segments {
            let block_end = base.checked_add(body.len())?;
            if block_end <= at || *base >= end {
                continue;
            }
            if *base > at {
                fragments.push(None);
                at = *base;
            }
            let next = end.min(block_end);
            fragments.push(body.get(at - base..next - base).map(str::to_owned));
            at = next;
            if at == end {
                break;
            }
        }
        if at < end {
            fragments.push(None);
        }
        Some(fragments)
    }
    pub fn span(&self, span: ByteSpan) -> Option<String> {
        let (segments, start, end) = self.locate(span)?;
        let mut at = start;
        let mut text = String::new();
        for (base, body) in segments {
            if at == end {
                break;
            }
            let block_end = base.checked_add(body.len())?;
            if at >= *base && at < block_end {
                let next = end.min(block_end);
                text.push_str(body.get(at - base..next - base)?);
                at = next;
            }
        }
        (at == end && text.chars().count() as u64 == span.char_len).then_some(text)
    }
    /// Text for one piece record, or `None` when the record itself is unusable.
    fn piece_fragments(&self, record: &[u8]) -> Option<Fragments> {
        let f = record
            .chunks_exact(8)
            .map(|b| b.try_into().ok().map(u64::from_le_bytes))
            .collect::<Option<Vec<_>>>()?;
        if f[0] > 1 || f[2] == 0 || f[3] == 0 || f[5] > 1 || f[6] > 1 {
            return None;
        }
        let span = byte_span([f[0], f[1], f[2], f[3]]);
        let Some(text) = self.span(span) else {
            let fragments = self.fragments(span)?;
            // Fully present bytes with wrong character metrics are not a missing
            // region. Do not present them as verified text.
            return fragments.iter().any(Option::is_none).then_some(fragments);
        };
        (text.bytes().filter(|b| *b == b'\n').count() as u64 == f[4]
            && u64::from(text.is_ascii()) == f[5]
            && u64::from(text.ends_with('\n')) == f[6])
            .then_some(vec![Some(text)])
    }
    fn reconstruct_pieces(&mut self, parts: &BTreeMap<usize, Vec<u8>>, end: usize) {
        let bytes_at = |start: usize, len: usize| -> Option<Vec<u8>> {
            (start..start.checked_add(len)?)
                .map(|at| {
                    let part = parts.get(&(at / PIECE_PART * PIECE_PART))?;
                    part.get(at % PIECE_PART).copied()
                })
                .collect()
        };
        let version = bytes_at(0, MAP_HEADER)
            .and_then(|header| Some(u64::from_le_bytes(header.get(..8)?.try_into().ok()?)));
        // A map shorter than its header never has one, so this also covers a missing map.
        if version != Some(1) {
            self.gap("the text map header is missing or damaged; text order cannot be established");
            return;
        }
        if end > RECOVERY_LIMIT {
            self.gap("text map exceeds the recovery inspection limit; original data retained");
            self.truncated = true;
            return;
        }
        let mut unknown = false;
        for at in (MAP_HEADER..end).step_by(PIECE_RECORD) {
            if self.text.len() >= RECOVERY_LIMIT {
                self.truncate();
                return;
            }
            let fragments = bytes_at(at, PIECE_RECORD)
                .and_then(|record| self.piece_fragments(&record))
                .unwrap_or_else(|| vec![None]);
            if !self.append(fragments, &mut unknown) {
                return;
            }
        }
    }
    /// Appends verified text and marks each run of unknown regions once.
    /// Returns false when the recovery limit truncated the copy.
    fn append(&mut self, fragments: Fragments, unknown: &mut bool) -> bool {
        for text in fragments {
            match text {
                Some(text) if self.text.len().saturating_add(text.len()) <= RECOVERY_LIMIT => {
                    self.text.push_str(&text);
                    *unknown = false;
                }
                Some(_) => {
                    self.truncate();
                    return false;
                }
                None if !*unknown => {
                    self.gap("text or its position is missing or damaged");
                    *unknown = true;
                }
                None => {}
            }
        }
        true
    }
    fn truncate(&mut self) {
        self.gap(TRUNCATED);
        self.truncated = true;
    }

    fn gap(&mut self, message: &str) {
        self.gaps += 1;
        self.text.push_str("\n[RECOVERY GAP: ");
        self.text.push_str(message);
        self.text.push_str("]\n");
    }
}

impl Database {
    pub(super) async fn recover_text(&self, id: &str) -> io::Result<RecoveredText> {
        let mut result = RecoveredText {
            text: String::new(),
            gaps: 0,
            truncated: false,
            segments: Default::default(),
        };
        // Bound exceptional recovery work before retrieving any body blobs.
        let sizes = self.query_collect("SELECT coalesce((SELECT sum(length(p.body)) FROM document_pieces p WHERE p.id=?1),0)+coalesce((SELECT sum(length(b.body)) FROM document_text t JOIN chunk_bodies b ON b.chunk_id=t.chunk_id WHERE t.temp_id=?1),0)+coalesce((SELECT sum(length(body)) FROM document_tail WHERE temp_id=?1),0)", [id], |r| r.get::<i64>(0).map_err(error)).await?;
        if sizes[0] > (RECOVERY_LIMIT * 4) as i64 {
            result.gap("recovery exceeds the 128 MiB inspection limit; original data retained");
            result.truncated = true;
            return Ok(result);
        }
        self.recovery_segments(id, &mut result).await?;
        self.recovery_pieces(id, &mut result).await?;
        Ok(result)
    }

    async fn recovery_segments(&self, id: &str, result: &mut RecoveredText) -> io::Result<()> {
        let blocks = self.query_collect("SELECT t.buffer,t.start_byte,c.digest,c.byte_len,c.char_len,c.newline_count,c.is_ascii,c.ends_with_lf,b.body FROM document_text t LEFT JOIN chunks c ON c.id=t.chunk_id LEFT JOIN chunk_bodies b ON b.chunk_id=c.id WHERE t.temp_id=? ORDER BY t.buffer,t.start_byte", [id], |r| {
            Ok((r.get::<i64>(0).map_err(error)?, r.get::<i64>(1).map_err(error)?, r.get::<Option<Vec<u8>>>(2).map_err(error)?, (3..8).map(|i| r.get::<Option<i64>>(i).map_err(error)).collect::<io::Result<Vec<_>>>()?, r.get::<Option<Vec<u8>>>(8).map_err(error)?))
        }).await?;
        for (side, start, digest, metrics, body) in blocks {
            let (Some(digest), Some(body)) = (digest, body) else {
                continue;
            };
            let Ok(text) = String::from_utf8(body) else {
                continue;
            };
            let expected = [
                text.len(),
                text.chars().count(),
                text.bytes().filter(|b| *b == b'\n').count(),
                usize::from(text.is_ascii()),
                usize::from(text.ends_with('\n')),
            ];
            if blake3::hash(text.as_bytes()).as_bytes().as_slice() != digest
                || metrics
                    .iter()
                    .zip(expected)
                    .any(|(a, b)| *a != Some(b as i64))
            {
                continue;
            }
            let side = usize::try_from(side).map_err(invalid)?;
            if side > 1 {
                return Err(invalid("invalid storage buffer"));
            }
            result.segments[side].push((usize::try_from(start).map_err(invalid)?, text));
        }
        let tails = self.query_collect("SELECT start_byte,offset,body,digest FROM document_tail WHERE temp_id=? ORDER BY offset", [id], |r| Ok((r.get::<i64>(0).map_err(error)?,r.get::<i64>(1).map_err(error)?,r.get::<Vec<u8>>(2).map_err(error)?,r.get::<Vec<u8>>(3).map_err(error)?))).await?;
        for (start, offset, body, digest) in tails {
            if blake3::hash(&body).as_bytes().as_slice() != digest {
                continue;
            }
            let Ok(text) = String::from_utf8(body) else {
                continue;
            };
            let address = start
                .checked_add(offset)
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| invalid("invalid tail address"))?;
            result.segments[1].push((address, text));
        }
        for segments in &mut result.segments {
            segments.sort_by_key(|(start, _)| *start);
            if segments
                .windows(2)
                .any(|w| w[0].0.saturating_add(w[0].1.len()) > w[1].0)
            {
                // Overlapping addresses cannot be disambiguated safely.
                segments.clear();
            }
        }
        Ok(())
    }

    async fn recovery_pieces(&self, id: &str, result: &mut RecoveredText) -> io::Result<()> {
        let rows = self
            .query_collect(
                "SELECT part,body,digest FROM document_pieces WHERE id=? ORDER BY part",
                [id],
                |r| {
                    Ok((
                        r.get::<i64>(0).map_err(error)?,
                        r.get::<Vec<u8>>(1).map_err(error)?,
                        r.get::<Vec<u8>>(2).map_err(error)?,
                    ))
                },
            )
            .await?;
        let mut parts = BTreeMap::new();
        let mut end = 0;
        for (part, body, digest) in rows {
            let start = usize::try_from(part)
                .ok()
                .and_then(|p| p.checked_mul(PIECE_PART))
                .ok_or_else(|| invalid("invalid piece part"))?;
            end = end.max(start.saturating_add(body.len()));
            if body.len() <= PIECE_PART && blake3::hash(&body).as_bytes().as_slice() == digest {
                parts.insert(start, body);
            }
        }
        result.reconstruct_pieces(&parts, end);
        Ok(())
    }
}
