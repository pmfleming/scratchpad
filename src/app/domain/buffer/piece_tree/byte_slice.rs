//! Byte-oriented streaming for persisted chunks, whose boundaries may split UTF-8.
use super::{PieceTreeLite, PieceTreeText};
use std::ops::Range;

pub(crate) struct ByteSlice<'a> {
    tree: &'a PieceTreeLite,
    range: Range<usize>,
    node: usize,
    leaf: usize,
    piece: usize,
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
            node,
            leaf: 0,
            piece: 0,
            offset,
        }
    }
}
impl<'a> Iterator for ByteSlice<'a> {
    type Item = (PieceTreeText<'a>, Range<usize>);
    fn next(&mut self) -> Option<Self::Item> {
        while self.offset < self.range.end {
            let node = self.tree.root.nodes.get(self.node)?;
            let Some(leaf) = node.leaves.get(self.leaf) else {
                self.node += 1;
                self.leaf = 0;
                self.piece = 0;
                continue;
            };
            let Some(piece) = leaf.pieces.get(self.piece) else {
                self.leaf += 1;
                self.piece = 0;
                continue;
            };
            self.piece += 1;
            let start = self.offset;
            self.offset += piece.byte_len;
            if self.offset <= self.range.start {
                continue;
            }
            let range =
                self.range.start.saturating_sub(start)..(self.range.end.min(self.offset) - start);
            return Some((self.tree.piece_text(piece), range));
        }
        None
    }
}
