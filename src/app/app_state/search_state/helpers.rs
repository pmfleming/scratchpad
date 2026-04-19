use super::SearchMatch;
use crate::app::domain::{BufferId, SearchHighlightState};
use eframe::egui;
use std::ops::Range;

pub(super) fn cursor_range_from_char_range(range: Range<usize>) -> egui::text::CCursorRange {
    egui::text::CCursorRange::two(
        egui::text::CCursor::new(range.start),
        egui::text::CCursor::new(range.end),
    )
}

pub(super) fn selection_char_range(
    cursor_range: egui::text::CCursorRange,
) -> Option<std::ops::Range<usize>> {
    let [left, right] = [cursor_range.primary.index, cursor_range.secondary.index];
    let (start, end) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    (start < end).then_some(start..end)
}

pub(super) fn preview_for_match(text: &str, range: &Range<usize>) -> (usize, usize, String) {
    let chars = text.chars().collect::<Vec<_>>();
    let safe_start = range.start.min(chars.len());
    let safe_end = range.end.min(chars.len());

    let mut line_start = safe_start;
    while line_start > 0 && chars[line_start - 1] != '\n' {
        line_start -= 1;
    }

    let mut line_end = safe_end;
    while line_end < chars.len() && chars[line_end] != '\n' {
        line_end += 1;
    }

    let line_number = chars[..safe_start].iter().filter(|ch| **ch == '\n').count() + 1;
    let column_number = safe_start.saturating_sub(line_start) + 1;
    let line_text = chars[line_start..line_end].iter().collect::<String>();
    let preview = compact_preview(&line_text);

    (line_number, column_number, preview)
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

fn compact_preview(line_text: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 96;
    let trimmed = line_text.trim();
    let trimmed_chars = trimmed.chars().collect::<Vec<_>>();
    if trimmed_chars.len() <= MAX_PREVIEW_CHARS {
        return trimmed.to_owned();
    }

    let mut preview = trimmed_chars[..MAX_PREVIEW_CHARS]
        .iter()
        .collect::<String>();
    preview.push_str("...");
    preview
}

#[cfg(test)]
mod tests {
    use super::{compact_preview, preview_for_match};

    #[test]
    fn preview_for_match_reports_line_and_column() {
        let (line, column, preview) = preview_for_match("one\ntwo alpha\nthree", &(8..13));
        assert_eq!(line, 2);
        assert_eq!(column, 5);
        assert_eq!(preview, "two alpha");
    }

    #[test]
    fn compact_preview_truncates_long_lines() {
        let preview = compact_preview(&"x".repeat(120));
        assert!(preview.ends_with("..."));
    }
}
