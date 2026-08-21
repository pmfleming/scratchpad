use super::{PieceTreeInternalNode, PieceTreeMetrics};

#[derive(Clone, Debug, Default)]
pub(super) struct NodeMetricIndex {
    chars: Vec<usize>,
    newlines: Vec<usize>,
}

const FENWICK_NODE_THRESHOLD: usize = 64;

impl NodeMetricIndex {
    pub(super) fn rebuild(&mut self, nodes: &[PieceTreeInternalNode]) {
        self.chars.clear();
        self.newlines.clear();
        if nodes.len() >= FENWICK_NODE_THRESHOLD {
            self.chars.resize(nodes.len() + 1, 0);
            self.newlines.resize(nodes.len() + 1, 0);
            for (index, node) in nodes.iter().enumerate() {
                Self::add(&mut self.chars, index, node.metrics.chars);
                Self::add(&mut self.newlines, index, node.metrics.newlines);
            }
            return;
        }

        let mut chars = 0usize;
        let mut newlines = 0usize;
        for node in nodes {
            self.chars.push(chars);
            chars = chars.saturating_add(node.metrics.chars);
            newlines = newlines.saturating_add(node.metrics.newlines);
            self.newlines.push(newlines);
        }
    }

    pub(super) fn update(&mut self, index: usize, old: PieceTreeMetrics, new: PieceTreeMetrics) {
        if self.uses_fenwick() {
            Self::replace_value(&mut self.chars, index, old.chars, new.chars);
            Self::replace_value(&mut self.newlines, index, old.newlines, new.newlines);
            return;
        }

        for start in self.chars.iter_mut().skip(index + 1) {
            *start = Self::replace_total(*start, old.chars, new.chars);
        }
        for end in self.newlines.iter_mut().skip(index) {
            *end = Self::replace_total(*end, old.newlines, new.newlines);
        }
    }

    pub(super) fn chars_before(&self, index: usize) -> usize {
        if self.uses_fenwick() {
            Self::prefix(&self.chars, index)
        } else {
            self.chars.get(index).copied().unwrap_or_default()
        }
    }

    pub(super) fn newlines_before(&self, index: usize) -> usize {
        if self.uses_fenwick() {
            Self::prefix(&self.newlines, index)
        } else if index == 0 {
            0
        } else {
            self.newlines.get(index - 1).copied().unwrap_or_default()
        }
    }

    pub(super) fn node_for_char(&self, target: usize) -> usize {
        if self.uses_fenwick() {
            Self::largest_prefix(&self.chars, target, false).min(self.chars.len().saturating_sub(2))
        } else {
            self.chars
                .partition_point(|start| *start <= target)
                .saturating_sub(1)
                .min(self.chars.len().saturating_sub(1))
        }
    }

    pub(super) fn node_for_line(&self, target: usize) -> usize {
        if self.uses_fenwick() {
            Self::largest_prefix(&self.newlines, target, true)
                .min(self.newlines.len().saturating_sub(2))
        } else {
            self.newlines
                .partition_point(|end| *end < target)
                .min(self.newlines.len().saturating_sub(1))
        }
    }

    fn uses_fenwick(&self) -> bool {
        self.chars.len() > FENWICK_NODE_THRESHOLD
    }

    fn add(tree: &mut [usize], index: usize, value: usize) {
        let mut cursor = index + 1;
        while cursor < tree.len() {
            tree[cursor] = tree[cursor].saturating_add(value);
            cursor += cursor & cursor.wrapping_neg();
        }
    }

    fn replace_value(tree: &mut [usize], index: usize, old: usize, new: usize) {
        let mut cursor = index + 1;
        while cursor < tree.len() {
            tree[cursor] = Self::replace_total(tree[cursor], old, new);
            cursor += cursor & cursor.wrapping_neg();
        }
    }

    fn replace_total(total: usize, old: usize, new: usize) -> usize {
        if new >= old {
            total.saturating_add(new - old)
        } else {
            total.saturating_sub(old - new)
        }
    }

    fn prefix(tree: &[usize], count: usize) -> usize {
        let mut cursor = count.min(tree.len().saturating_sub(1));
        let mut total = 0usize;
        while cursor > 0 {
            total = total.saturating_add(tree[cursor]);
            cursor &= cursor - 1;
        }
        total
    }

    fn largest_prefix(tree: &[usize], target: usize, strict: bool) -> usize {
        let item_count = tree.len().saturating_sub(1);
        let mut index = 0usize;
        let mut sum = 0usize;
        let mut step = item_count.next_power_of_two();
        while step > 0 {
            let next = index + step;
            if next <= item_count {
                let candidate = sum.saturating_add(tree[next]);
                let can_advance = if strict {
                    candidate < target
                } else {
                    candidate <= target
                };
                if can_advance {
                    index = next;
                    sum = candidate;
                }
            }
            step >>= 1;
        }
        index
    }
}

impl PieceTreeMetrics {
    pub(in crate::app::domain::buffer::piece_tree) fn add_assign(&mut self, other: Self) {
        self.bytes += other.bytes;
        self.chars += other.chars;
        self.newlines += other.newlines;
        self.pieces += other.pieces;
    }

    pub(in crate::app::domain::buffer::piece_tree) fn saturating_sub_assign(
        &mut self,
        other: Self,
    ) {
        self.bytes = self.bytes.saturating_sub(other.bytes);
        self.chars = self.chars.saturating_sub(other.chars);
        self.newlines = self.newlines.saturating_sub(other.newlines);
        self.pieces = self.pieces.saturating_sub(other.pieces);
    }
}

pub(in crate::app::domain::buffer::piece_tree) fn sum_node_metrics(
    nodes: &[PieceTreeInternalNode],
) -> PieceTreeMetrics {
    let mut metrics = PieceTreeMetrics::default();
    for node in nodes {
        metrics.add_assign(node.metrics);
    }
    metrics
}

#[cfg(test)]
mod tests {
    use super::{FENWICK_NODE_THRESHOLD, NodeMetricIndex};
    use crate::app::domain::buffer::piece_tree::{PieceTreeInternalNode, PieceTreeMetrics};

    #[test]
    fn fenwick_index_matches_linear_prefixes_and_boundaries() {
        let mut nodes = (0..FENWICK_NODE_THRESHOLD + 3)
            .map(|index| PieceTreeInternalNode {
                metrics: PieceTreeMetrics {
                    chars: 10 + index,
                    newlines: index % 4,
                    ..PieceTreeMetrics::default()
                },
                ..PieceTreeInternalNode::default()
            })
            .collect::<Vec<_>>();
        let mut index = NodeMetricIndex::default();
        index.rebuild(&nodes);

        let mut chars = 0usize;
        let mut newlines = 0usize;
        for (node_index, node) in nodes.iter().enumerate() {
            assert_eq!(index.chars_before(node_index), chars);
            assert_eq!(index.newlines_before(node_index), newlines);
            assert_eq!(index.node_for_char(chars), node_index);
            let mut cumulative = 0usize;
            let expected_line_node = nodes
                .iter()
                .position(|candidate| {
                    cumulative += candidate.metrics.newlines;
                    cumulative >= newlines
                })
                .unwrap_or(nodes.len() - 1);
            assert_eq!(index.node_for_line(newlines), expected_line_node);
            chars += node.metrics.chars;
            newlines += node.metrics.newlines;
        }

        let changed = 30usize;
        let old = nodes[changed].metrics;
        nodes[changed].metrics.chars += 17;
        nodes[changed].metrics.newlines += 2;
        index.update(changed, old, nodes[changed].metrics);
        assert_eq!(
            index.chars_before(changed + 1),
            nodes[..=changed]
                .iter()
                .map(|node| node.metrics.chars)
                .sum::<usize>()
        );
        assert_eq!(
            index.newlines_before(changed + 1),
            nodes[..=changed]
                .iter()
                .map(|node| node.metrics.newlines)
                .sum::<usize>()
        );
    }
}
