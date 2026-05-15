use super::{EditorViewState, SearchHighlightState};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

impl SearchHighlightState {
    #[must_use]
    pub fn layout_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl EditorViewState {
    pub(super) fn resolve_search_highlight_anchors(
        &mut self,
        buffer: &crate::app::domain::BufferState,
    ) {
        if self.anchors.search_highlight_anchors.is_empty() {
            return;
        }
        let piece_tree = buffer.document().piece_tree();
        let mut ranges = Vec::with_capacity(self.anchors.search_highlight_anchors.len());
        let mut active_range_index = None;
        for (index, anchored) in self.anchors.search_highlight_anchors.iter().enumerate() {
            let Some(start) = piece_tree.anchor_position(anchored.start) else {
                continue;
            };
            let Some(end) = piece_tree.anchor_position(anchored.end) else {
                continue;
            };
            if start >= end {
                continue;
            }
            if self.search_highlights.active_range_index == Some(index) {
                active_range_index = Some(ranges.len());
            }
            ranges.push(start..end);
        }
        self.search_highlights.ranges = ranges;
        self.search_highlights.active_range_index = active_range_index;
    }
}
