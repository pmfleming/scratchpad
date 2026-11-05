use super::super::{ByteSpan, PieceSource, PieceTreeLite};

impl PieceTreeLite {
    pub fn append_history_text(&mut self, text: &str, source: PieceSource) -> ByteSpan {
        self.storage
            .append_add_text(text, source, self.runtime.generation())
            .byte_span()
    }
}
