//! Stable anchors over `PieceTreeLite`.
//!
//! An `AnchorId` is an opaque, `Copy` handle that names a logical position in
//! the document. Anchors are stored on the piece-tree leaves that contain their
//! point positions. Edits above an untouched leaf shift anchors naturally via
//! the tree's prefix metrics; edits inside rebuilt leaves redistribute only the
//! anchors attached to those leaves.
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

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnchorBias {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AnchorOwnerKind {
    #[default]
    Unspecified,
    ViewScroll,
    Cursor,
    SelectionEndpoint,
    SearchEndpoint,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AnchorOwner {
    kind: AnchorOwnerKind,
    id: Option<u64>,
}

impl AnchorOwner {
    pub const fn new(kind: AnchorOwnerKind, id: Option<u64>) -> Self {
        Self { kind, id }
    }

    pub const fn unspecified() -> Self {
        Self::new(AnchorOwnerKind::Unspecified, None)
    }

    pub const fn view_scroll(view_id: u64) -> Self {
        Self::new(AnchorOwnerKind::ViewScroll, Some(view_id))
    }

    pub const fn cursor(id: u64) -> Self {
        Self::new(AnchorOwnerKind::Cursor, Some(id))
    }

    pub const fn selection_endpoint(id: u64) -> Self {
        Self::new(AnchorOwnerKind::SelectionEndpoint, Some(id))
    }

    pub const fn search_endpoint(id: u64) -> Self {
        Self::new(AnchorOwnerKind::SearchEndpoint, Some(id))
    }

    pub const fn kind(self) -> AnchorOwnerKind {
        self.kind
    }

    pub const fn id(self) -> Option<u64> {
        self.id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnchorId(u64);

impl AnchorId {
    pub fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(super) struct LeafId(u64);

impl LeafId {
    pub(super) fn next(next_id: &mut u64) -> Self {
        let id = Self(*next_id);
        *next_id = next_id.wrapping_add(1).max(1);
        id
    }

    pub(super) fn is_unassigned(self) -> bool {
        self.0 == 0
    }
}

#[derive(Clone, Debug)]
pub(super) struct AnchorEntry {
    pub(super) bias: AnchorBias,
    pub(super) leaf_id: LeafId,
    pub(super) owner: AnchorOwner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LeafAnchor {
    pub(super) id: AnchorId,
    pub(super) local_offset: usize,
}

#[derive(Clone, Debug, Default)]
pub(super) struct AnchorRegistry {
    next_id: u64,
    entries: HashMap<AnchorId, AnchorEntry>,
}

impl AnchorRegistry {
    pub(super) fn create(
        &mut self,
        leaf_id: LeafId,
        bias: AnchorBias,
        owner: AnchorOwner,
    ) -> AnchorId {
        let id = AnchorId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.entries.insert(
            id,
            AnchorEntry {
                bias,
                leaf_id,
                owner,
            },
        );
        id
    }

    pub(super) fn release(&mut self, id: AnchorId) -> Option<AnchorEntry> {
        self.entries.remove(&id)
    }

    pub(super) fn entry(&self, id: AnchorId) -> Option<&AnchorEntry> {
        self.entries.get(&id)
    }

    pub(super) fn set_leaf(&mut self, id: AnchorId, leaf_id: LeafId) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.leaf_id = leaf_id;
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    pub(super) fn live_count(&self) -> usize {
        self.entries.len()
    }
}
