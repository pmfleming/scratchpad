use super::EditorViewState;

impl EditorViewState {
    pub(super) fn resolve_search_highlight_anchors(
        &mut self,
        buffer: &crate::app::domain::BufferState,
    ) {
        if self.anchors.search_highlight_anchors.is_empty() {
            return;
        }
        let token = buffer.document().history_token();
        if self.anchors.resolved_search_token == Some(token) {
            return;
        }
        self.anchors.resolved_search_token = Some(token);
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

#[cfg(test)]
mod tests {
    use crate::app::domain::{BufferState, EditorViewState, SearchHighlightState};

    #[test]
    fn cached_highlights_follow_edits_and_replacement_documents() {
        let mut buffer = BufferState::new("sample".into(), "alpha beta".into(), None);
        let mut view = EditorViewState::new(buffer.id);
        view.set_search_highlights_anchored(
            &mut buffer,
            SearchHighlightState {
                ranges: vec![6..10],
                active_range_index: Some(0),
            },
        );
        view.resolve_anchored_ranges(&buffer);
        let allocation = view.search_highlights.ranges.as_ptr();
        view.resolve_anchored_ranges(&buffer);
        assert_eq!(allocation, view.search_highlights.ranges.as_ptr());
        buffer.document_mut().insert_direct(0, "x");
        view.resolve_anchored_ranges(&buffer);
        assert_eq!(view.search_highlights.ranges, vec![7..11]);
        let replacement = buffer.document().clone();
        *buffer.document_mut() = replacement;
        view.resolve_anchored_ranges(&buffer);
        assert_eq!(view.search_highlights.ranges, vec![7..11]);
        view.set_search_highlights_anchored(
            &mut buffer,
            SearchHighlightState {
                ranges: vec![1..6],
                active_range_index: None,
            },
        );
        view.resolve_anchored_ranges(&buffer);
        assert_eq!(view.search_highlights.ranges, vec![1..6]);
    }
}
