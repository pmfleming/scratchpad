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
use std::ops::Range;

use super::{LeafAddress, PieceTreeInternalNode, PieceTreeLeaf, PieceTreeLite};

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
}

impl PieceTreeLite {
    pub fn create_anchor(&mut self, char_offset: usize, bias: AnchorBias) -> AnchorId {
        self.create_anchor_with_owner(char_offset, bias, AnchorOwner::unspecified())
    }

    pub fn create_anchor_with_owner(
        &mut self,
        char_offset: usize,
        bias: AnchorBias,
        owner: AnchorOwner,
    ) -> AnchorId {
        let safe = char_offset.min(self.len_chars());
        self.ensure_anchorable_leaf();
        let address = self.find_leaf_for_char_offset(safe);
        let leaf_id = self.root.nodes[address.node_index].leaves[address.leaf_index].leaf_id;
        let id = self.anchors.create(leaf_id, bias, owner);
        self.root.nodes[address.node_index].leaves[address.leaf_index]
            .anchors
            .push(LeafAnchor {
                id,
                local_offset: safe.saturating_sub(address.leaf_start_char),
            });
        self.root.recalculate();
        id
    }

    pub fn release_anchor(&mut self, id: AnchorId) {
        let Some(entry) = self.anchors.release(id) else {
            return;
        };
        if let Some((node_index, leaf_index)) = self.find_leaf_indices_by_id(entry.leaf_id) {
            self.root.nodes[node_index].leaves[leaf_index]
                .anchors
                .retain(|anchor| anchor.id != id);
            self.root.recalculate();
        }
    }

    pub fn anchor_position(&self, id: AnchorId) -> Option<usize> {
        let entry = self.anchors.entry(id)?;
        let (address, leaf) = self.find_leaf_by_id(entry.leaf_id)?;
        let leaf_anchor = leaf.anchors.iter().find(|anchor| anchor.id == id)?;
        Some(address.leaf_start_char + leaf_anchor.local_offset.min(leaf.metrics.chars))
    }

    pub fn anchor_bias(&self, id: AnchorId) -> Option<AnchorBias> {
        self.anchors.entry(id).map(|entry| entry.bias)
    }

    pub fn anchor_owner(&self, id: AnchorId) -> Option<AnchorOwner> {
        self.anchors.entry(id).map(|entry| entry.owner)
    }

    pub(crate) fn clone_without_anchors(&self) -> Self {
        let mut clone = self.clone();
        clone.clear_anchors();
        clone
    }

    pub(crate) fn has_live_anchors(&self) -> bool {
        !self.anchors.is_empty()
    }

    pub(crate) fn clear_anchors(&mut self) {
        self.anchors.clear();
        for node in &mut self.root.nodes {
            for leaf in &mut node.leaves {
                leaf.anchors.clear();
            }
        }
        self.root.recalculate();
    }

    fn ensure_anchorable_leaf(&mut self) {
        if self.root.nodes.is_empty() {
            self.root.nodes.push(PieceTreeInternalNode::default());
        }
        self.assign_missing_leaf_ids();
    }

    pub(super) fn assign_missing_leaf_ids(&mut self) {
        for node in &mut self.root.nodes {
            for leaf in &mut node.leaves {
                if leaf.leaf_id.is_unassigned() {
                    leaf.leaf_id = LeafId::next(&mut self.next_leaf_id);
                }
            }
        }
        self.root.recalculate();
    }

    fn find_leaf_by_id(&self, leaf_id: LeafId) -> Option<(LeafAddress, &PieceTreeLeaf)> {
        let (node_index, leaf_index) = self.find_leaf_indices_by_id(leaf_id)?;
        let address = self.address_for_leaf_indices(node_index, leaf_index);
        let leaf = &self.root.nodes[node_index].leaves[leaf_index];
        Some((address, leaf))
    }

    fn find_leaf_indices_by_id(&self, leaf_id: LeafId) -> Option<(usize, usize)> {
        for (node_index, node) in self.root.nodes.iter().enumerate() {
            for (leaf_index, leaf) in node.leaves.iter().enumerate() {
                if leaf.leaf_id == leaf_id {
                    return Some((node_index, leaf_index));
                }
            }
        }
        None
    }

    pub(super) fn address_for_leaf_indices(
        &self,
        node_index: usize,
        leaf_index: usize,
    ) -> LeafAddress {
        LeafAddress {
            node_index,
            leaf_index,
            leaf_start_char: self.root.node_start_chars[node_index]
                + self.root.nodes[node_index].leaf_start_chars[leaf_index],
            leaf_start_newline: self.root.node_start_newlines[node_index]
                + self.root.nodes[node_index].leaf_start_newlines[leaf_index],
        }
    }

    pub(super) fn reposition_inserted_leaf_anchors(
        &self,
        address: LeafAddress,
        offset_chars: usize,
        inserted_chars: usize,
    ) -> Vec<LeafAnchor> {
        let leaf = &self.root.nodes[address.node_index].leaves[address.leaf_index];
        let insert_local_offset = offset_chars
            .saturating_sub(address.leaf_start_char)
            .min(leaf.metrics.chars);

        leaf.anchors
            .iter()
            .filter_map(|anchor| {
                let entry = self.anchors.entry(anchor.id)?;
                let mut local_offset = anchor.local_offset;
                if local_offset > insert_local_offset
                    || (local_offset == insert_local_offset
                        && matches!(entry.bias, AnchorBias::Right))
                {
                    local_offset += inserted_chars;
                }
                Some(LeafAnchor {
                    id: anchor.id,
                    local_offset,
                })
            })
            .collect()
    }

    pub(super) fn reposition_removed_span_anchors(
        &self,
        start_address: LeafAddress,
        end_address: LeafAddress,
        range_chars: &Range<usize>,
    ) -> Vec<LeafAnchor> {
        let removed_chars = range_chars.end - range_chars.start;
        let mut anchors = Vec::new();

        for node_index in start_address.node_index..=end_address.node_index {
            let node = &self.root.nodes[node_index];
            let leaf_start = if node_index == start_address.node_index {
                start_address.leaf_index
            } else {
                0
            };
            let leaf_end = if node_index == end_address.node_index {
                end_address.leaf_index
            } else {
                node.leaves.len() - 1
            };

            for leaf_index in leaf_start..=leaf_end {
                let address = self.address_for_leaf_indices(node_index, leaf_index);
                let leaf = &self.root.nodes[node_index].leaves[leaf_index];
                for anchor in &leaf.anchors {
                    let global_offset = address.leaf_start_char + anchor.local_offset;
                    let new_global_offset = if global_offset <= range_chars.start {
                        global_offset
                    } else if global_offset >= range_chars.end {
                        global_offset - removed_chars
                    } else {
                        range_chars.start
                    };
                    anchors.push(LeafAnchor {
                        id: anchor.id,
                        local_offset: new_global_offset
                            .saturating_sub(start_address.leaf_start_char),
                    });
                }
            }
        }

        anchors
    }

    pub(super) fn redistribute_anchors_into_leaves(
        &mut self,
        leaves: &mut [PieceTreeLeaf],
        anchors: Vec<LeafAnchor>,
    ) {
        if leaves.is_empty() {
            return;
        }

        let mut leaf_starts = Vec::with_capacity(leaves.len());
        let mut current_offset = 0usize;
        for leaf in leaves.iter_mut() {
            if leaf.leaf_id.is_unassigned() {
                leaf.leaf_id = LeafId::next(&mut self.next_leaf_id);
            }
            leaf.anchors.clear();
            leaf_starts.push(current_offset);
            current_offset += leaf.metrics.chars;
        }

        for anchor in anchors {
            let bounded_offset = anchor.local_offset.min(current_offset);
            let leaf_index = leaf_starts
                .partition_point(|start| *start <= bounded_offset)
                .saturating_sub(1)
                .min(leaves.len() - 1);
            let leaf_local_offset = bounded_offset.saturating_sub(leaf_starts[leaf_index]);
            let leaf_id = leaves[leaf_index].leaf_id;
            self.anchors.set_leaf(anchor.id, leaf_id);
            leaves[leaf_index].anchors.push(LeafAnchor {
                id: anchor.id,
                local_offset: leaf_local_offset.min(leaves[leaf_index].metrics.chars),
            });
        }
    }
}
