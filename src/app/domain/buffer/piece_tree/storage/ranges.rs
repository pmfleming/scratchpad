//! One indexed walk for logical original/add spans; reading remains explicit.
use super::{
    ByteSpan, ChunkText, OriginalTextStorage, Piece, PieceBuffer, PieceTreeStorage, PieceTreeText,
    io,
};
use std::ops::Range;

enum Body<'a> {
    Owned(&'a str),
    Chunk(&'a ChunkText),
}
pub(super) struct SpanBlock<'a> {
    body: Body<'a>,
    start: usize,
    range: Range<usize>,
    full_len: usize,
    known_chars: Option<u64>,
}
impl SpanBlock<'_> {
    pub(super) fn read(&self) -> io::Result<PieceTreeText<'_>> {
        let invalid = || io::Error::other("history span splits UTF-8");
        match self.body {
            Body::Owned(text) => Ok(PieceTreeText::borrowed(
                text.get(self.range.clone()).ok_or_else(invalid)?,
            )),
            Body::Chunk(text) => {
                let body = text.try_load_for_scan(false)?;
                body.get(self.range.clone()).ok_or_else(invalid)?;
                Ok(PieceTreeText::shared(body, self.range.clone()))
            }
        }
    }
    fn whole_piece(&self, buffer: PieceBuffer) -> io::Result<Option<Piece>> {
        if self.range != (0..self.full_len) {
            return Ok(None);
        }
        let Body::Chunk(ChunkText::Stored(chunk)) = self.body else {
            return Ok(None);
        };
        let Some(reference) = chunk.reference() else {
            return Ok(None);
        };
        let m = reference.metadata;
        Ok(Some(Piece {
            buffer,
            start_byte: self.start,
            byte_len: m.byte_len,
            char_len: usize::try_from(m.char_len).map_err(io::Error::other)?,
            newline_count: usize::try_from(m.newline_count).map_err(io::Error::other)?,
            is_ascii: m.is_ascii,
            ends_with_lf: m.ends_with_lf,
        }))
    }
    pub(super) fn char_len(&self, buffer: PieceBuffer) -> io::Result<u64> {
        if self.range == (0..self.full_len)
            && let Some(chars) = self.known_chars
        {
            return Ok(chars);
        }
        if let Some(piece) = self.whole_piece(buffer)? {
            return Ok(piece.char_len as u64);
        }
        Ok(self.read()?.chars().count() as u64)
    }
    pub(super) fn append_pieces(
        &self,
        buffer: PieceBuffer,
        pieces: &mut Vec<Piece>,
    ) -> io::Result<()> {
        if let Some(piece) = self.whole_piece(buffer)? {
            pieces.push(piece);
        } else {
            let text = self.read()?;
            let start = self.start + self.range.start;
            if text.len() <= super::ADD_TAIL_BYTES {
                pieces.push(Piece::from_slice(buffer, start, &text));
            } else {
                pieces.extend(super::super::build_chunked_pieces(buffer, start, &text));
            }
        }
        Ok(())
    }
}
impl PieceTreeStorage {
    pub(super) fn visit_span_blocks(
        &self,
        span: ByteSpan,
        mut visit: impl FnMut(SpanBlock<'_>) -> io::Result<()>,
    ) -> io::Result<()> {
        if !self.contains_span(span) {
            return Err(io::Error::other("history span exceeds storage"));
        }
        if span.byte_len == 0 {
            return Ok(());
        }
        let start = span.start_byte as usize;
        let end = span.byte_end() as usize;
        let mut chunk = |base: usize, text: &ChunkText, chars: Option<u64>| {
            visit(SpanBlock {
                body: Body::Chunk(text),
                start: base,
                full_len: text.len(),
                range: start.saturating_sub(base)..(end - base).min(text.len()),
                known_chars: chars,
            })
        };
        match (&self.original, span.buffer) {
            (OriginalTextStorage::Owned(text), PieceBuffer::Original) => visit(SpanBlock {
                body: Body::Owned(text),
                start: 0,
                range: start..end,
                full_len: text.len(),
                known_chars: None,
            }),
            (OriginalTextStorage::Chunked(original), PieceBuffer::Original) => {
                for block in original
                    .chunks
                    .iter()
                    .skip(
                        original
                            .chunks
                            .partition_point(|c| c.logical_start + c.byte_len <= start),
                    )
                    .take_while(|c| c.logical_start < end)
                {
                    chunk(
                        block.logical_start,
                        &block.text,
                        Some(block.char_len as u64),
                    )?;
                }
                Ok(())
            }
            (_, PieceBuffer::Add) => {
                for block in self
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
                    chunk(block.start_byte, &block.text, None)?;
                }
                Ok(())
            }
        }
    }
}
