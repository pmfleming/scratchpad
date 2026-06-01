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

use super::{LeafAddress, PieceTreeInternalNode, PieceTreeLeaf, PieceTreeLite, PieceTreeRoot};

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
    #[must_use]
    pub const fn new(kind: AnchorOwnerKind, id: Option<u64>) -> Self {
        Self { kind, id }
    }

    #[must_use]
    pub const fn unspecified() -> Self {
        Self::new(AnchorOwnerKind::Unspecified, None)
    }

    #[must_use]
    pub const fn view_scroll(view_id: u64) -> Self {
        Self::new(AnchorOwnerKind::ViewScroll, Some(view_id))
    }

    #[must_use]
    pub const fn cursor(id: u64) -> Self {
        Self::new(AnchorOwnerKind::Cursor, Some(id))
    }

    #[must_use]
    pub const fn selection_endpoint(id: u64) -> Self {
        Self::new(AnchorOwnerKind::SelectionEndpoint, Some(id))
    }

    #[must_use]
    pub const fn search_endpoint(id: u64) -> Self {
        Self::new(AnchorOwnerKind::SearchEndpoint, Some(id))
    }

    #[must_use]
    pub const fn kind(self) -> AnchorOwnerKind {
        self.kind
    }

    #[must_use]
    pub const fn id(self) -> Option<u64> {
        self.id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnchorId(u64);

impl AnchorId {
    #[must_use]
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

#[derive(Clone, Debug)]
pub(super) struct PieceTreeAnchorState {
    registry: AnchorRegistry,
    leaf_indices_by_id: HashMap<LeafId, (usize, usize)>,
    next_leaf_id: u64,
}

impl Default for PieceTreeAnchorState {
    fn default() -> Self {
        Self {
            registry: AnchorRegistry::default(),
            leaf_indices_by_id: HashMap::new(),
            next_leaf_id: 1,
        }
    }
}

impl PieceTreeAnchorState {
    pub(super) fn create(
        &mut self,
        leaf_id: LeafId,
        bias: AnchorBias,
        owner: AnchorOwner,
    ) -> AnchorId {
        self.registry.create(leaf_id, bias, owner)
    }

    pub(super) fn release(&mut self, id: AnchorId) -> Option<AnchorEntry> {
        self.registry.release(id)
    }

    pub(super) fn entry(&self, id: AnchorId) -> Option<&AnchorEntry> {
        self.registry.entry(id)
    }

    pub(super) fn set_leaf(&mut self, id: AnchorId, leaf_id: LeafId) {
        self.registry.set_leaf(id, leaf_id);
    }

    pub(super) fn clear(&mut self) {
        self.registry.clear();
    }

    pub(super) fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }

    pub(super) fn next_leaf_id(&mut self) -> LeafId {
        LeafId::next(&mut self.next_leaf_id)
    }

    pub(super) fn rebuild_leaf_index(&mut self, root: &PieceTreeRoot) {
        self.leaf_indices_by_id.clear();
        for (node_index, node) in root.nodes.iter().enumerate() {
            for (leaf_index, leaf) in node.leaves.iter().enumerate() {
                if !leaf.leaf_id.is_unassigned() {
                    self.leaf_indices_by_id
                        .insert(leaf.leaf_id, (node_index, leaf_index));
                }
            }
        }
    }

    pub(super) fn refresh_leaf_index_after_structure_change(&mut self, root: &PieceTreeRoot) {
        if self.is_empty() {
            self.leaf_indices_by_id.clear();
        } else {
            self.rebuild_leaf_index(root);
        }
    }

    pub(super) fn assign_missing_leaf_ids(&mut self, root: &mut PieceTreeRoot) {
        let mut assigned = false;
        for node in &mut root.nodes {
            for leaf in &mut node.leaves {
                if leaf.leaf_id.is_unassigned() {
                    leaf.leaf_id = self.next_leaf_id();
                    assigned = true;
                }
            }
        }
        if assigned || self.leaf_indices_by_id.is_empty() {
            self.rebuild_leaf_index(root);
        }
    }

    pub(super) fn find_leaf_indices_by_id(
        &self,
        root: &PieceTreeRoot,
        leaf_id: LeafId,
    ) -> Option<(usize, usize)> {
        let (node_index, leaf_index) = self.leaf_indices_by_id.get(&leaf_id).copied()?;
        let leaf = root.nodes.get(node_index)?.leaves.get(leaf_index)?;
        (leaf.leaf_id == leaf_id).then_some((node_index, leaf_index))
    }
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
        let id = self.anchor_state.create(leaf_id, bias, owner);
        self.root.nodes[address.node_index].leaves[address.leaf_index]
            .anchors
            .push(LeafAnchor {
                id,
                local_offset: safe.saturating_sub(address.leaf_start_char),
            });
        self.root.nodes[address.node_index].anchor_count += 1;
        self.root.anchor_count += 1;
        id
    }

    pub fn release_anchor(&mut self, id: AnchorId) {
        let Some(entry) = self.anchor_state.release(id) else {
            return;
        };
        if let Some((node_index, leaf_index)) = self.find_leaf_indices_by_id(entry.leaf_id) {
            let before = self.root.nodes[node_index].leaves[leaf_index].anchors.len();
            self.root.nodes[node_index].leaves[leaf_index]
                .anchors
                .retain(|anchor| anchor.id != id);
            let removed =
                before.saturating_sub(self.root.nodes[node_index].leaves[leaf_index].anchors.len());
            self.root.nodes[node_index].anchor_count = self.root.nodes[node_index]
                .anchor_count
                .saturating_sub(removed);
            self.root.anchor_count = self.root.anchor_count.saturating_sub(removed);
        }
    }

    #[must_use]
    pub fn anchor_position(&self, id: AnchorId) -> Option<usize> {
        let entry = self.anchor_state.entry(id)?;
        let (address, leaf) = self.find_leaf_by_id(entry.leaf_id)?;
        let leaf_anchor = leaf.anchors.iter().find(|anchor| anchor.id == id)?;
        Some(address.leaf_start_char + leaf_anchor.local_offset.min(leaf.metrics.chars))
    }

    #[must_use]
    pub fn anchor_bias(&self, id: AnchorId) -> Option<AnchorBias> {
        self.anchor_state.entry(id).map(|entry| entry.bias)
    }

    #[must_use]
    pub fn anchor_owner(&self, id: AnchorId) -> Option<AnchorOwner> {
        self.anchor_state.entry(id).map(|entry| entry.owner)
    }

    pub(crate) fn clone_without_anchors(&self) -> Self {
        let mut clone = self.clone();
        clone.clear_anchors();
        clone
    }

    pub(crate) fn has_live_anchors(&self) -> bool {
        !self.anchor_state.is_empty()
    }

    pub(crate) fn clear_anchors(&mut self) {
        self.anchor_state.clear();
        for node in &mut self.root.nodes {
            for leaf in &mut node.leaves {
                leaf.anchors.clear();
            }
            node.anchor_count = 0;
        }
        self.root.anchor_count = 0;
    }

    fn ensure_anchorable_leaf(&mut self) {
        if self.root.nodes.is_empty() {
            self.root.nodes.push(PieceTreeInternalNode::default());
        }
        self.assign_missing_leaf_ids();
    }

    pub(super) fn assign_missing_leaf_ids(&mut self) {
        self.anchor_state.assign_missing_leaf_ids(&mut self.root);
    }

    fn find_leaf_by_id(&self, leaf_id: LeafId) -> Option<(LeafAddress, &PieceTreeLeaf)> {
        let (node_index, leaf_index) = self.find_leaf_indices_by_id(leaf_id)?;
        let address = self.address_for_leaf_indices(node_index, leaf_index);
        let leaf = &self.root.nodes[node_index].leaves[leaf_index];
        Some((address, leaf))
    }

    fn find_leaf_indices_by_id(&self, leaf_id: LeafId) -> Option<(usize, usize)> {
        self.anchor_state
            .find_leaf_indices_by_id(&self.root, leaf_id)
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
                let entry = self.anchor_state.entry(anchor.id)?;
                Some(LeafAnchor {
                    id: anchor.id,
                    local_offset: inserted_anchor_local_offset(
                        anchor.local_offset,
                        insert_local_offset,
                        inserted_chars,
                        entry.bias,
                    ),
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
                    let new_global_offset =
                        removed_anchor_global_offset(global_offset, range_chars, removed_chars);
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
                leaf.leaf_id = self.anchor_state.next_leaf_id();
            }
            leaf.anchors.clear();
            leaf_starts.push(current_offset);
            current_offset += leaf.metrics.chars;
        }

        for anchor in anchors {
            let bounded_offset = anchor.local_offset.min(current_offset);
            let leaf_index =
                leaf_index_for_anchor_offset(&leaf_starts, bounded_offset, leaves.len());
            let leaf_local_offset = bounded_offset.saturating_sub(leaf_starts[leaf_index]);
            let leaf_id = leaves[leaf_index].leaf_id;
            self.anchor_state.set_leaf(anchor.id, leaf_id);
            leaves[leaf_index].anchors.push(LeafAnchor {
                id: anchor.id,
                local_offset: leaf_local_offset.min(leaves[leaf_index].metrics.chars),
            });
        }
    }
}

fn inserted_anchor_local_offset(
    local_offset: usize,
    insert_local_offset: usize,
    inserted_chars: usize,
    bias: AnchorBias,
) -> usize {
    if local_offset > insert_local_offset
        || (local_offset == insert_local_offset && matches!(bias, AnchorBias::Right))
    {
        local_offset + inserted_chars
    } else {
        local_offset
    }
}

fn removed_anchor_global_offset(
    global_offset: usize,
    range_chars: &Range<usize>,
    removed_chars: usize,
) -> usize {
    if global_offset <= range_chars.start {
        global_offset
    } else if global_offset >= range_chars.end {
        global_offset - removed_chars
    } else {
        range_chars.start
    }
}

fn leaf_index_for_anchor_offset(
    leaf_starts: &[usize],
    bounded_offset: usize,
    leaf_count: usize,
) -> usize {
    leaf_starts
        .partition_point(|start| *start <= bounded_offset)
        .saturating_sub(1)
        .min(leaf_count - 1)
}

#[cfg(test)]
mod decision_tests {
    use super::{
        AnchorBias, inserted_anchor_local_offset, leaf_index_for_anchor_offset,
        removed_anchor_global_offset,
    };

    #[test]
    fn inserted_anchor_offset_respects_bias_at_insert_boundary() {
        assert_eq!(inserted_anchor_local_offset(5, 5, 3, AnchorBias::Left), 5);
        assert_eq!(inserted_anchor_local_offset(5, 5, 3, AnchorBias::Right), 8);
        assert_eq!(inserted_anchor_local_offset(7, 5, 3, AnchorBias::Left), 10);
    }

    #[test]
    fn removed_anchor_offset_collapses_inside_range_and_shifts_after_range() {
        let range = 10..15;

        assert_eq!(removed_anchor_global_offset(9, &range, 5), 9);
        assert_eq!(removed_anchor_global_offset(12, &range, 5), 10);
        assert_eq!(removed_anchor_global_offset(17, &range, 5), 12);
    }

    #[test]
    fn leaf_index_for_anchor_offset_selects_containing_leaf() {
        let starts = [0, 10, 25];

        assert_eq!(leaf_index_for_anchor_offset(&starts, 0, 3), 0);
        assert_eq!(leaf_index_for_anchor_offset(&starts, 14, 3), 1);
        assert_eq!(leaf_index_for_anchor_offset(&starts, 99, 3), 2);
    }
}
