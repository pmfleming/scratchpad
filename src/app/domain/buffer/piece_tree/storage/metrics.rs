use super::{ByteSpan, Piece, PieceTreeStorage, io};
impl PieceTreeStorage {
    pub(crate) fn pieces_for_span(&self, span: ByteSpan) -> io::Result<Vec<Piece>> {
        let mut pieces = Vec::new();
        self.visit_span_blocks(span, |block| block.append_pieces(span.buffer, &mut pieces))?;
        if pieces.iter().map(|p| p.char_len as u64).sum::<u64>() != span.char_len {
            return Err(io::Error::other("history character length mismatch"));
        }
        Ok(pieces)
    }
}
