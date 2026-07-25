use super::slice::{previews_for_matches_in_contiguous_text, previews_for_matches_in_piece_spans};
use super::support::compact_preview;
use super::{PREVIEW_MAX_CHARS, PieceTreeLite};
use std::ops::Range;

pub(crate) fn preview_for_match(
    tree: &PieceTreeLite,
    range_chars: &Range<usize>,
) -> (usize, usize, String) {
    let normalized = tree.normalize_char_range(range_chars.clone());
    let line_index = tree.line_index_at_offset(normalized.start);
    let info = tree.line_info(line_index);
    let column = normalized.start.saturating_sub(info.start_char);
    let (line_text, truncated) = tree.extract_range_bounded(
        info.start_char..info.start_char + info.char_len,
        PREVIEW_MAX_CHARS,
    );
    let mut preview = compact_preview(&line_text);
    if truncated && !preview.ends_with("...") {
        preview.push_str("...");
    }
    (line_index + 1, column + 1, preview)
}

pub(crate) fn previews_for_matches(
    tree: &PieceTreeLite,
    ranges: &[Range<usize>],
    limit: usize,
) -> Vec<(usize, usize, String)> {
    let limited_ranges = &ranges[..ranges.len().min(limit)];
    if limited_ranges.is_empty() {
        return Vec::new();
    }

    if let Some(text) = tree.borrow_range(0..tree.len_chars()) {
        return previews_for_matches_in_contiguous_text(&text, limited_ranges);
    }

    previews_for_matches_in_piece_spans(tree, limited_ranges)
}
