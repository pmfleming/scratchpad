//! Bounded, branch-safe byte edits for incremental snapshot persistence.
//! Tokens are process-local identities, never document generation numbers.
use super::PieceTreeLite;
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
const LIMIT: usize = 128;
fn token() -> u64 {
    NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ByteEdit {
    pub start: usize,
    pub removed: usize,
    pub inserted: usize,
}
#[derive(Clone, Debug)]
pub(super) struct SaveChanges {
    current: u64,
    edits: VecDeque<(u64, ByteEdit)>,
}
impl Default for SaveChanges {
    fn default() -> Self {
        Self {
            current: token(),
            edits: VecDeque::new(),
        }
    }
}
impl SaveChanges {
    pub(super) fn record(&mut self, start: usize, removed: usize, inserted: usize) {
        self.edits.push_back((
            self.current,
            ByteEdit {
                start,
                removed,
                inserted,
            },
        ));
        self.current = token();
        if self.edits.len() > LIMIT {
            self.edits.pop_front();
        }
    }
}
impl PieceTreeLite {
    pub(crate) fn save_change_token(&self) -> u64 {
        self.save_changes.current
    }
    pub(crate) fn save_changes_since(&self, token: u64) -> Option<Vec<ByteEdit>> {
        if token == self.save_changes.current {
            return Some(Vec::new());
        }
        let index = self
            .save_changes
            .edits
            .iter()
            .position(|(before, _)| *before == token)?;
        Some(
            self.save_changes
                .edits
                .iter()
                .skip(index)
                .map(|(_, edit)| *edit)
                .collect(),
        )
    }

    // Traverse metadata only until the boundary piece; at most that piece's UTF-8
    // needs scanning. No flattening or reading of the document prefix.
    pub(super) fn byte_offset_for_char(&self, offset: usize) -> usize {
        if offset == self.len_chars() {
            return self.len_bytes();
        }
        let address = self.find_leaf_for_char_offset(offset);
        let mut bytes: usize = self.root.nodes[..address.node_index]
            .iter()
            .map(|node| node.metrics.bytes)
            .sum();
        let node = &self.root.nodes[address.node_index];
        bytes += node.leaves[..address.leaf_index]
            .iter()
            .map(|leaf| leaf.metrics.bytes)
            .sum::<usize>();
        let mut remaining = offset - address.leaf_start_char;
        for piece in &node.leaves[address.leaf_index].pieces {
            if remaining >= piece.char_len {
                remaining -= piece.char_len;
                bytes += piece.byte_len;
            } else {
                bytes += if piece.is_ascii {
                    remaining
                } else {
                    super::byte_index_for_char_offset(&self.piece_text(piece), remaining)
                };
                break;
            }
        }
        bytes
    }
}
