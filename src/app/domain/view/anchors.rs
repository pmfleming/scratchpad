use super::ViewId;
use crate::app::domain::buffer::{AnchorBias, AnchorId, AnchorOwner};
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct AnchoredEndpoint {
    pub(super) anchor: AnchorId,
    pub(super) prefer_next_row: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct AnchoredCursorRange {
    pub(super) primary: AnchoredEndpoint,
    pub(super) secondary: AnchoredEndpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct AnchoredSearchRange {
    pub(super) start: AnchorId,
    pub(super) end: AnchorId,
}

pub(super) fn sync_optional_cursor_anchor_range(
    view_id: ViewId,
    buffer: &mut crate::app::domain::BufferState,
    cursor_range: Option<CursorRange>,
    anchored: &mut Option<AnchoredCursorRange>,
) {
    if resolve_cursor_anchor_range(*anchored, buffer) == cursor_range {
        return;
    }
    release_anchors(buffer, take_cursor_anchors(anchored));
    let Some(cursor_range) = cursor_range else {
        return;
    };
    *anchored = Some(create_cursor_anchor_range(view_id, buffer, cursor_range));
}

fn create_cursor_anchor_range(
    view_id: ViewId,
    buffer: &mut crate::app::domain::BufferState,
    cursor_range: CursorRange,
) -> AnchoredCursorRange {
    let (start, end) = cursor_range.sorted_indices();
    AnchoredCursorRange {
        primary: create_cursor_endpoint_anchor(
            buffer,
            cursor_range.primary.index,
            cursor_endpoint_bias(cursor_range.primary.index, start, end),
            AnchorOwner::cursor(view_id),
            cursor_range.primary.prefer_next_row,
        ),
        secondary: create_cursor_endpoint_anchor(
            buffer,
            cursor_range.secondary.index,
            cursor_endpoint_bias(cursor_range.secondary.index, start, end),
            AnchorOwner::selection_endpoint(view_id),
            cursor_range.secondary.prefer_next_row,
        ),
    }
}

fn cursor_endpoint_bias(index: usize, start: usize, end: usize) -> AnchorBias {
    if start == end || index >= end {
        AnchorBias::Right
    } else {
        AnchorBias::Left
    }
}

fn create_cursor_endpoint_anchor(
    buffer: &mut crate::app::domain::BufferState,
    index: usize,
    bias: AnchorBias,
    owner: AnchorOwner,
    prefer_next_row: bool,
) -> AnchoredEndpoint {
    let anchor = buffer
        .document_mut()
        .piece_tree_mut()
        .create_anchor_with_owner(index, bias, owner);
    AnchoredEndpoint {
        anchor,
        prefer_next_row,
    }
}

pub(super) fn resolve_cursor_anchor_range(
    anchored: Option<AnchoredCursorRange>,
    buffer: &crate::app::domain::BufferState,
) -> Option<CursorRange> {
    let anchored = anchored?;
    let piece_tree = buffer.document().piece_tree();
    Some(CursorRange {
        primary: CharCursor {
            index: piece_tree.anchor_position(anchored.primary.anchor)?,
            prefer_next_row: anchored.primary.prefer_next_row,
        },
        secondary: CharCursor {
            index: piece_tree.anchor_position(anchored.secondary.anchor)?,
            prefer_next_row: anchored.secondary.prefer_next_row,
        },
    })
}

pub(super) fn take_cursor_anchors(anchored: &mut Option<AnchoredCursorRange>) -> Vec<AnchorId> {
    anchored
        .take()
        .map(|range| vec![range.primary.anchor, range.secondary.anchor])
        .unwrap_or_default()
}

pub(super) fn take_search_anchors(anchors: &mut Vec<AnchoredSearchRange>) -> Vec<AnchorId> {
    anchors
        .drain(..)
        .flat_map(|range| [range.start, range.end])
        .collect()
}

pub(super) fn release_anchors(
    buffer: &mut crate::app::domain::BufferState,
    anchors: Vec<AnchorId>,
) {
    for anchor in anchors {
        buffer
            .document_mut()
            .piece_tree_mut()
            .release_anchor(anchor);
    }
}
