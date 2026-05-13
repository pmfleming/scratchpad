use super::{PieceTreeInternalNode, PieceTreeMetrics};

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
