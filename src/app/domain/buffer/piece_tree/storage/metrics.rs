use super::{ByteSpan, ChunkText, OriginalTextStorage, Piece, PieceBuffer, PieceTreeStorage, io};
impl PieceTreeStorage {
    pub(crate) fn pieces_for_span(&self, span: ByteSpan) -> io::Result<Vec<Piece>> {
        if !self.contains_span(span) {
            return Err(io::Error::other("history span exceeds storage"));
        }
        let start = span.start_byte as usize;
        let end = span.byte_end() as usize;
        let mut pieces = Vec::new();
        let mut visit = |base: usize, text: &ChunkText| -> io::Result<()> {
            let lo = start.saturating_sub(base);
            let hi = end.saturating_sub(base).min(text.len());
            if lo >= hi {
                return Ok(());
            }
            if lo == 0
                && hi == text.len()
                && let ChunkText::Stored(chunk) = text
                && let Some(reference) = chunk.reference()
            {
                let m = reference.metadata;
                pieces.push(Piece {
                    buffer: span.buffer,
                    start_byte: base,
                    byte_len: m.byte_len,
                    char_len: usize::try_from(m.char_len).map_err(io::Error::other)?,
                    newline_count: usize::try_from(m.newline_count).map_err(io::Error::other)?,
                    is_ascii: m.is_ascii,
                    ends_with_lf: m.ends_with_lf,
                });
            } else {
                let text = text.try_load_for_scan(false)?;
                pieces.push(Piece::from_slice(
                    span.buffer,
                    base + lo,
                    text.get(lo..hi)
                        .ok_or_else(|| io::Error::other("history span splits UTF-8"))?,
                ));
            }
            Ok(())
        };
        match (&self.original, span.buffer) {
            (OriginalTextStorage::Owned(text), PieceBuffer::Original) => {
                let text = text
                    .get(start..end)
                    .ok_or_else(|| io::Error::other("history span splits UTF-8"))?;
                pieces.extend(super::super::build_chunked_pieces(span.buffer, start, text));
            }
            (OriginalTextStorage::Chunked(original), PieceBuffer::Original) => {
                for chunk in original
                    .chunks
                    .iter()
                    .skip(
                        original
                            .chunks
                            .partition_point(|c| c.logical_start + c.byte_len <= start),
                    )
                    .take_while(|c| c.logical_start < end)
                {
                    visit(chunk.logical_start, &chunk.text)?;
                }
            }
            (_, PieceBuffer::Add) => {
                for chunk in self
                    .add
                    .chunks
                    .iter()
                    .skip(
                        self.add
                            .chunks
                            .partition_point(|c| c.start_byte + c.text.len() <= start),
                    )
                    .take_while(|c| c.start_byte < end)
                {
                    visit(chunk.start_byte, &chunk.text)?;
                }
            }
        }
        if pieces.iter().map(|p| p.char_len as u64).sum::<u64>() != span.char_len {
            return Err(io::Error::other("history character length mismatch"));
        }
        Ok(pieces)
    }
}
