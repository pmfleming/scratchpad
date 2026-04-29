//! Stable anchors over `PieceTreeLite`.
//!
//! An `AnchorId` is an opaque, `Copy` handle that names a logical position in
//! the document. The tree maintains a registry of live anchors and updates
//! every entry's `char_offset` whenever the document is edited, so a position
//! created at "byte 100 of line 5" continues to identify the same content even
//! after edits above it shift everything downstream.
//!
//! Bias controls what happens when an edit lands exactly at the anchor:
//!
//! * `AnchorBias::Left`  — the anchor sticks to the text on the left side of
//!   an insertion point. Inserting *at* the anchor leaves it unchanged.
//! * `AnchorBias::Right` — the anchor sticks to the text on the right side.
//!   Inserting *at* the anchor moves it forward by the inserted length.
//!
//! Removals: an anchor inside a removed range collapses to the start of the
//! removal regardless of bias.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnchorBias {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnchorId(u64);

impl AnchorId {
    pub fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub(super) struct AnchorEntry {
    pub(super) id: AnchorId,
    pub(super) char_offset: usize,
    pub(super) bias: AnchorBias,
}

#[derive(Clone, Debug, Default)]
pub(super) struct AnchorRegistry {
    next_id: u64,
    entries: Vec<AnchorEntry>,
}

impl AnchorRegistry {
    pub(super) fn create(&mut self, char_offset: usize, bias: AnchorBias) -> AnchorId {
        let id = AnchorId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.entries.push(AnchorEntry {
            id,
            char_offset,
            bias,
        });
        id
    }

    pub(super) fn release(&mut self, id: AnchorId) {
        self.entries.retain(|entry| entry.id != id);
    }

    pub(super) fn position(&self, id: AnchorId) -> Option<usize> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.char_offset)
    }

    pub(super) fn entry(&self, id: AnchorId) -> Option<&AnchorEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub(super) fn shift_for_insert(&mut self, at_chars: usize, inserted_chars: usize) {
        if inserted_chars == 0 {
            return;
        }
        for entry in &mut self.entries {
            if entry.char_offset > at_chars
                || (entry.char_offset == at_chars && matches!(entry.bias, AnchorBias::Right))
            {
                entry.char_offset += inserted_chars;
            }
        }
    }

    pub(super) fn shift_for_remove(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        let removed = range.end - range.start;
        for entry in &mut self.entries {
            if entry.char_offset <= range.start {
                continue;
            }
            if entry.char_offset >= range.end {
                entry.char_offset -= removed;
                continue;
            }
            // Anchor was inside the removed range — collapse to the start.
            entry.char_offset = range.start;
        }
    }

    #[cfg(test)]
    pub(super) fn live_count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_after_anchor_shifts_offset() {
        let mut reg = AnchorRegistry::default();
        let id = reg.create(10, AnchorBias::Left);
        reg.shift_for_insert(5, 3);
        assert_eq!(reg.position(id), Some(13));
    }

    #[test]
    fn insert_before_anchor_does_not_shift() {
        let mut reg = AnchorRegistry::default();
        let id = reg.create(10, AnchorBias::Left);
        reg.shift_for_insert(20, 3);
        assert_eq!(reg.position(id), Some(10));
    }

    #[test]
    fn insert_at_anchor_respects_bias() {
        let mut reg = AnchorRegistry::default();
        let left = reg.create(10, AnchorBias::Left);
        let right = reg.create(10, AnchorBias::Right);
        reg.shift_for_insert(10, 4);
        assert_eq!(reg.position(left), Some(10));
        assert_eq!(reg.position(right), Some(14));
    }

    #[test]
    fn remove_before_anchor_shifts_offset_back() {
        let mut reg = AnchorRegistry::default();
        let id = reg.create(20, AnchorBias::Left);
        reg.shift_for_remove(5..10);
        assert_eq!(reg.position(id), Some(15));
    }

    #[test]
    fn remove_overlapping_anchor_collapses_to_start() {
        let mut reg = AnchorRegistry::default();
        let id = reg.create(15, AnchorBias::Left);
        reg.shift_for_remove(10..20);
        assert_eq!(reg.position(id), Some(10));
    }

    #[test]
    fn remove_at_or_after_anchor_does_not_shift() {
        let mut reg = AnchorRegistry::default();
        let id = reg.create(10, AnchorBias::Left);
        reg.shift_for_remove(10..15);
        assert_eq!(reg.position(id), Some(10));
        reg.shift_for_remove(20..25);
        assert_eq!(reg.position(id), Some(10));
    }

    #[test]
    fn release_removes_anchor_from_registry() {
        let mut reg = AnchorRegistry::default();
        let id = reg.create(5, AnchorBias::Left);
        assert_eq!(reg.live_count(), 1);
        reg.release(id);
        assert_eq!(reg.live_count(), 0);
        assert_eq!(reg.position(id), None);
    }

    #[test]
    fn anchor_ids_are_unique_across_creation_and_release() {
        let mut reg = AnchorRegistry::default();
        let a = reg.create(0, AnchorBias::Left);
        reg.release(a);
        let b = reg.create(0, AnchorBias::Left);
        assert_ne!(a, b);
    }
}
