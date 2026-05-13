use super::super::{LeafAddress, PieceTreeLeaf, PieceTreeLite, support::pack_leaves_into_nodes};
use std::ops::Range;

impl PieceTreeLite {
    pub(super) fn replace_leaf_span(
        &mut self,
        start: LeafAddress,
        end: LeafAddress,
        replacement_leaves: Vec<PieceTreeLeaf>,
    ) {
        let combined_leaves = self.take_leaf_replacement_window(start, end, replacement_leaves);

        let replacement_nodes = pack_leaves_into_nodes(combined_leaves);
        let inserted_nodes = replacement_nodes.len();
        self.root
            .replace_recalculated_nodes(start.node_index..end.node_index + 1, replacement_nodes);
        self.rebalance_node_window(start.node_index, inserted_nodes);
        self.refresh_leaf_index_after_structure_change();
    }

    fn take_leaf_replacement_window(
        &mut self,
        start: LeafAddress,
        end: LeafAddress,
        replacement_leaves: Vec<PieceTreeLeaf>,
    ) -> Vec<PieceTreeLeaf> {
        let kept_prefix = start.leaf_index;
        let kept_suffix = self.root.nodes[end.node_index]
            .leaves
            .len()
            .saturating_sub(end.leaf_index + 1);
        let mut combined_leaves =
            Vec::with_capacity(kept_prefix + replacement_leaves.len() + kept_suffix);

        if start.node_index == end.node_index {
            let mut leaves = std::mem::take(&mut self.root.nodes[start.node_index].leaves);
            let suffix = leaves.split_off(end.leaf_index + 1);
            leaves.truncate(start.leaf_index);
            combined_leaves.extend(leaves);
            combined_leaves.extend(replacement_leaves);
            combined_leaves.extend(suffix);
            return combined_leaves;
        }

        let mut prefix = std::mem::take(&mut self.root.nodes[start.node_index].leaves);
        prefix.truncate(start.leaf_index);
        combined_leaves.extend(prefix);
        combined_leaves.extend(replacement_leaves);

        let mut suffix = std::mem::take(&mut self.root.nodes[end.node_index].leaves);
        combined_leaves.extend(suffix.split_off(end.leaf_index + 1));
        combined_leaves
    }

    fn rebalance_node_window(&mut self, inserted_at: usize, inserted_nodes: usize) {
        if self.root.nodes.is_empty() {
            self.root.recalculate();
            return;
        }

        let touched_start = inserted_at.saturating_sub(1);
        let touched_end = (inserted_at + inserted_nodes + 1).min(self.root.nodes.len());
        if self.node_window_is_balanced(touched_start..touched_end) {
            return;
        }

        let mut window_start = touched_start;
        let mut window_end = touched_end;

        if window_start > 0
            && self.root.nodes[window_start].leaves.len() < super::super::MIN_LEAVES_PER_INTERNAL
        {
            window_start -= 1;
        }
        if window_end < self.root.nodes.len()
            && self.root.nodes[window_end - 1].leaves.len() < super::super::MIN_LEAVES_PER_INTERNAL
        {
            window_end = (window_end + 1).min(self.root.nodes.len());
        }

        let mut window_leaves = Vec::new();
        for node in &self.root.nodes[window_start..window_end] {
            window_leaves.extend(node.leaves.iter().cloned());
        }

        let rebalanced_nodes = pack_leaves_into_nodes(window_leaves);
        self.root
            .replace_recalculated_nodes(window_start..window_end, rebalanced_nodes);
    }

    fn node_window_is_balanced(&self, range: Range<usize>) -> bool {
        if self.root.nodes.len() <= 1 {
            return true;
        }

        self.root.nodes[range].iter().all(|node| {
            (super::super::MIN_LEAVES_PER_INTERNAL..=super::super::MAX_LEAVES_PER_INTERNAL)
                .contains(&node.leaves.len())
        })
    }
}
