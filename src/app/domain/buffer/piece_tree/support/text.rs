use super::super::{PREVIEW_MAX_CHARS, PieceTreeMetrics};
use memchr::memchr_iter;

pub(in crate::app::domain::buffer::piece_tree) struct PieceTextMetrics {
    pub byte_len: usize,
    pub char_len: usize,
    pub newline_count: usize,
    pub is_ascii: bool,
}

pub(in crate::app::domain::buffer::piece_tree) fn measure_text(text: &str) -> PieceTextMetrics {
    let bytes = text.as_bytes();
    let byte_len = bytes.len();
    if bytes.is_ascii() {
        return PieceTextMetrics {
            byte_len,
            char_len: byte_len,
            newline_count: memchr_iter(b'\n', bytes).count(),
            is_ascii: true,
        };
    }

    PieceTextMetrics {
        byte_len,
        char_len: text.chars().count(),
        newline_count: memchr_iter(b'\n', bytes).count(),
        is_ascii: false,
    }
}

pub(in crate::app::domain::buffer::piece_tree) fn recalculate_prefix_metrics<T>(
    items: &[T],
    start_chars: &mut Vec<usize>,
    start_newlines: &mut Vec<usize>,
    metrics_of: impl Fn(&T) -> PieceTreeMetrics,
) -> PieceTreeMetrics {
    let mut metrics = PieceTreeMetrics::default();
    start_chars.clear();
    start_newlines.clear();

    let mut current_chars = 0usize;
    let mut current_newlines = 0usize;
    for item in items {
        start_chars.push(current_chars);
        start_newlines.push(current_newlines);

        let item_metrics = metrics_of(item);
        metrics.add_assign(item_metrics);
        current_chars += item_metrics.chars;
        current_newlines += item_metrics.newlines;
    }

    metrics
}

pub(in crate::app::domain::buffer::piece_tree) fn byte_range_for_char_range(
    text: &str,
    start_char: usize,
    end_char: usize,
) -> std::ops::Range<usize> {
    let start = byte_index_for_char_offset(text, start_char);
    let end = byte_index_for_char_offset(text, end_char);
    start..end
}

pub(in crate::app::domain::buffer::piece_tree) fn byte_index_for_char_offset(
    text: &str,
    char_offset: usize,
) -> usize {
    if char_offset == 0 {
        return 0;
    }
    if text.is_ascii() {
        return char_offset.min(text.len());
    }

    text.char_indices()
        .map(|(index, _)| index)
        .nth(char_offset)
        .unwrap_or(text.len())
}

pub(in crate::app::domain::buffer::piece_tree) fn compact_preview(line_text: &str) -> String {
    let trimmed = line_text.trim();
    let Some((end_byte, _)) = trimmed.char_indices().nth(PREVIEW_MAX_CHARS) else {
        return trimmed.to_owned();
    };
    format!("{}...", &trimmed[..end_byte])
}
