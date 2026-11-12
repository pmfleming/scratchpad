//! Best-effort extraction trusts only checksummed metadata and text. Gaps stay explicit.
use super::{Database, error, invalid};
use crate::app::domain::buffer::{ByteSpan, PieceBuffer};
use crate::app::services::session_store::recovery::RECOVERY_LIMIT;
use std::{collections::BTreeMap, io};

pub(super) struct RecoveredText {
    pub text: String,
    pub gaps: usize,
    pub truncated: bool,
    segments: [Vec<(usize, String)>; 2],
}

impl RecoveredText {
    fn fragments(&self, span: ByteSpan) -> Option<Vec<Option<String>>> {
        let start = usize::try_from(span.start_byte).ok()?;
        let len = usize::try_from(span.byte_len).ok()?;
        if len > RECOVERY_LIMIT {
            return None;
        }
        let end = start.checked_add(len)?;
        let mut at = start;
        let mut fragments = Vec::new();
        for (base, body) in &self.segments[usize::from(span.buffer == PieceBuffer::Add)] {
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
        let side = usize::from(span.buffer == PieceBuffer::Add);
        let start = usize::try_from(span.start_byte).ok()?;
        let len = usize::try_from(span.byte_len).ok()?;
        if len > RECOVERY_LIMIT {
            return None;
        }
        let end = start.checked_add(len)?;
        let mut at = start;
        let mut text = String::new();
        for (base, body) in &self.segments[side] {
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
    fn reconstruct_pieces(&mut self, parts: &BTreeMap<usize, Vec<u8>>, end: usize) {
        let bytes_at = |start: usize, len: usize| -> Option<Vec<u8>> {
            (start..start.checked_add(len)?)
                .map(|at| parts.get(&(at / 65536 * 65536))?.get(at % 65536).copied())
                .collect()
        };
        let header = bytes_at(0, 24);
        if header.as_ref().is_none_or(|h| {
            h.get(..8)
                .and_then(|b| b.try_into().ok())
                .map(u64::from_le_bytes)
                != Some(1)
        }) {
            self.gap("the text map header is missing or damaged; text order cannot be established");
            return;
        }
        if end > RECOVERY_LIMIT {
            self.gap("text map exceeds the recovery inspection limit; original data retained");
            self.truncated = true;
            return;
        }
        let mut unknown = false;
        'pieces: for at in (24..end).step_by(56) {
            if self.text.len() >= RECOVERY_LIMIT {
                self.gap("remaining text omitted from this 32 MiB recovery copy");
                self.truncated = true;
                break;
            }
            let text = bytes_at(at, 56).and_then(|bytes| {
                let f = bytes
                    .chunks_exact(8)
                    .map(|b| b.try_into().ok().map(u64::from_le_bytes))
                    .collect::<Option<Vec<_>>>()?;
                if f[0] > 1 || f[2] == 0 || f[3] == 0 || f[5] > 1 || f[6] > 1 {
                    return None;
                }
                let span = ByteSpan {
                    buffer: if f[0] == 0 {
                        PieceBuffer::Original
                    } else {
                        PieceBuffer::Add
                    },
                    start_byte: f[1],
                    byte_len: f[2],
                    char_len: f[3],
                };
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
            });
            for text in text.unwrap_or_else(|| vec![None]) {
                match text {
                    Some(text) if self.text.len().saturating_add(text.len()) <= RECOVERY_LIMIT => {
                        self.text.push_str(&text);
                        unknown = false;
                    }
                    Some(_) => {
                        self.gap("remaining text omitted from this 32 MiB recovery copy");
                        self.truncated = true;
                        break 'pieces;
                    }
                    None if !unknown => {
                        self.gap("text or its position is missing or damaged");
                        unknown = true;
                    }
                    None => {}
                }
            }
        }
        if end < 24 {
            self.gap("text map missing");
        }
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
                .and_then(|p| p.checked_mul(65536))
                .ok_or_else(|| invalid("invalid piece part"))?;
            end = end.max(start.saturating_add(body.len()));
            if body.len() <= 65536 && blake3::hash(&body).as_bytes().as_slice() == digest {
                parts.insert(start, body);
            }
        }
        result.reconstruct_pieces(&parts, end);
        Ok(())
    }
}
