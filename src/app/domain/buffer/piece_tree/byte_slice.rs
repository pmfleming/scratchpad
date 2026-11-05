//! Byte-oriented streaming for persisted chunks, whose boundaries may split UTF-8.
use super::{PieceTreeLite, PieceTreeText, slice::Pieces};
use std::{io, ops::Range};

pub(crate) struct ByteSlice<'a> {
    tree: &'a PieceTreeLite,
    range: Range<usize>,
    pieces: Pieces<'a>,
    offset: usize,
}
impl PieceTreeLite {
    pub(crate) fn byte_spans(&self, range: Range<usize>) -> ByteSlice<'_> {
        let mut node = 0;
        let mut offset = 0;
        while let Some(next) = self.root.nodes.get(node) {
            if offset + next.metrics.bytes > range.start {
                break;
            }
            offset += next.metrics.bytes;
            node += 1;
        }
        ByteSlice {
            tree: self,
            range,
            pieces: Pieces::new(self, node, 0),
            offset,
        }
    }
}
impl<'a> Iterator for ByteSlice<'a> {
    type Item = io::Result<(PieceTreeText<'a>, Range<usize>)>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.offset < self.range.end {
            let piece = self.pieces.next()?;
            let start = self.offset;
            self.offset += piece.byte_len;
            if self.offset <= self.range.start {
                continue;
            }
            let range =
                self.range.start.saturating_sub(start)..(self.range.end.min(self.offset) - start);
            return Some(
                self.tree
                    .storage
                    .try_text_for_span(
                        super::ByteSpan {
                            buffer: piece.buffer,
                            start_byte: piece.start_byte as u64,
                            byte_len: piece.byte_len as u64,
                        },
                        false,
                    )
                    .map(|text| (text, range)),
            );
        }
        None
    }
}
