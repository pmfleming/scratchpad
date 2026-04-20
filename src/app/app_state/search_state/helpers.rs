use super::SearchMatch;
use crate::app::domain::{BufferId, SearchHighlightState};
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::ops::Range;

pub(super) fn cursor_range_from_char_range(range: Range<usize>) -> CursorRange {
    CursorRange::two(range.start, range.end)
}

pub(super) fn selection_char_range(cursor_range: CursorRange) -> Option<std::ops::Range<usize>> {
    let (start, end) = cursor_range.sorted_indices();
    (start < end).then_some(start..end)
}

pub(super) fn search_highlight_state_for_view(
    tab_index: usize,
    buffer_id: BufferId,
    matches: &[SearchMatch],
    active_match_index: Option<usize>,
) -> SearchHighlightState {
    let mut ranges = Vec::new();
    let mut active_range_index = None;

    for (match_index, search_match) in matches.iter().enumerate() {
        if search_match.tab_index != tab_index || search_match.buffer_id != buffer_id {
            continue;
        }
        if Some(match_index) == active_match_index {
            active_range_index = Some(ranges.len());
        }
        ranges.push(search_match.range.clone());
    }

    SearchHighlightState {
        ranges,
        active_range_index,
    }
}
