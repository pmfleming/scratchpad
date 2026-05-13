use super::super::support::compact_preview;
use super::super::{PREVIEW_MAX_CHARS, PieceTreeLite, preview};
use std::ops::Range;

pub(in crate::app::domain::buffer::piece_tree) fn previews_for_matches_in_contiguous_text(
    text: &str,
    ranges: &[Range<usize>],
) -> Vec<(usize, usize, String)> {
    let mut previews = Vec::with_capacity(ranges.len());
    let mut cursor = PreviewCursor::default();
    let mut cached_line_start_byte = None;
    let mut cached_preview = String::new();

    for range in ranges {
        cursor.advance_to(text, range.start);
        update_cached_line_preview(
            text,
            cursor.line_start_byte,
            &mut cached_line_start_byte,
            &mut cached_preview,
        );

        previews.push((
            cursor.line_number,
            range.start.saturating_sub(cursor.line_start_char) + 1,
            cached_preview.clone(),
        ));
    }

    previews
}

pub(in crate::app::domain::buffer::piece_tree) fn previews_for_matches_in_piece_spans(
    tree: &PieceTreeLite,
    ranges: &[Range<usize>],
) -> Vec<(usize, usize, String)> {
    let match_starts = ranges
        .iter()
        .map(|range| tree.normalize_char_range(range.clone()).start)
        .collect::<Vec<_>>();
    if !match_starts.windows(2).all(|pair| pair[0] <= pair[1]) {
        return ranges
            .iter()
            .map(|range| preview::preview_for_match(tree, range))
            .collect();
    }

    let mut previews = vec![None; ranges.len()];
    let mut pending = Vec::new();
    let mut line = PiecePreviewLine::default();
    let mut cursor = PiecePreviewCursor::default();
    let mut next_match = 0usize;

    for span in tree.spans_for_range(0..tree.len_chars()) {
        if cursor.current_char < span.char_start {
            cursor.current_char = span.char_start;
        }
        for ch in span.text.chars() {
            queue_piece_preview_matches(&match_starts, &mut next_match, &cursor, &mut pending);
            if next_match == match_starts.len() && !pending.is_empty() && line.truncated {
                finish_piece_preview_line(&line, &mut pending, &mut previews);
                return collect_piece_previews(tree, ranges, previews);
            }

            if ch == '\n' {
                finish_piece_preview_line(&line, &mut pending, &mut previews);
                if next_match == match_starts.len() {
                    return collect_piece_previews(tree, ranges, previews);
                }
                cursor.advance_line();
                line.clear();
            } else {
                line.push(ch);
                cursor.current_char += 1;
                if next_match == match_starts.len() && !pending.is_empty() && line.truncated {
                    finish_piece_preview_line(&line, &mut pending, &mut previews);
                    return collect_piece_previews(tree, ranges, previews);
                }
            }
        }
    }

    queue_piece_preview_matches(&match_starts, &mut next_match, &cursor, &mut pending);
    finish_piece_preview_line(&line, &mut pending, &mut previews);

    collect_piece_previews(tree, ranges, previews)
}

#[derive(Default)]
struct PreviewCursor {
    current_char: usize,
    current_byte: usize,
    line_number: usize,
    line_start_char: usize,
    line_start_byte: usize,
}

impl PreviewCursor {
    fn advance_to(&mut self, text: &str, target_char: usize) {
        if self.line_number == 0 {
            self.line_number = 1;
        }
        while self.current_char < target_char && self.current_byte < text.len() {
            let Some(ch) = text[self.current_byte..].chars().next() else {
                break;
            };
            self.advance_char(ch);
        }
    }

    fn advance_char(&mut self, ch: char) {
        let next_byte = self.current_byte + ch.len_utf8();
        if ch == '\n' {
            self.line_number += 1;
            self.line_start_char = self.current_char + 1;
            self.line_start_byte = next_byte;
        }
        self.current_char += 1;
        self.current_byte = next_byte;
    }
}

#[derive(Default)]
struct PiecePreviewCursor {
    current_char: usize,
    line_number: usize,
    line_start_char: usize,
}

impl PiecePreviewCursor {
    fn line_number(&self) -> usize {
        self.line_number.max(1)
    }

    fn advance_line(&mut self) {
        self.current_char += 1;
        self.line_number = self.line_number().saturating_add(1);
        self.line_start_char = self.current_char;
    }
}

#[derive(Default)]
struct PiecePreviewLine {
    text: String,
    char_len: usize,
    truncated: bool,
}

impl PiecePreviewLine {
    fn push(&mut self, ch: char) {
        if self.char_len < PREVIEW_MAX_CHARS {
            self.text.push(ch);
        } else {
            self.truncated = true;
        }
        self.char_len += 1;
    }

    fn clear(&mut self) {
        self.text.clear();
        self.char_len = 0;
        self.truncated = false;
    }

    fn preview(&self) -> String {
        let mut preview = compact_preview(&self.text);
        if self.truncated && !preview.ends_with("...") {
            preview.push_str("...");
        }
        preview
    }
}

struct PendingPiecePreview {
    index: usize,
    line_number: usize,
    column_number: usize,
}

fn queue_piece_preview_matches(
    match_starts: &[usize],
    next_match: &mut usize,
    cursor: &PiecePreviewCursor,
    pending: &mut Vec<PendingPiecePreview>,
) {
    while match_starts
        .get(*next_match)
        .is_some_and(|start| *start <= cursor.current_char)
    {
        let start = match_starts[*next_match];
        pending.push(PendingPiecePreview {
            index: *next_match,
            line_number: cursor.line_number(),
            column_number: start.saturating_sub(cursor.line_start_char) + 1,
        });
        *next_match += 1;
    }
}

fn finish_piece_preview_line(
    line: &PiecePreviewLine,
    pending: &mut Vec<PendingPiecePreview>,
    previews: &mut [Option<(usize, usize, String)>],
) {
    if pending.is_empty() {
        return;
    }
    let preview = line.preview();
    for pending_preview in pending.drain(..) {
        previews[pending_preview.index] = Some((
            pending_preview.line_number,
            pending_preview.column_number,
            preview.clone(),
        ));
    }
}

fn collect_piece_previews(
    tree: &PieceTreeLite,
    ranges: &[Range<usize>],
    previews: Vec<Option<(usize, usize, String)>>,
) -> Vec<(usize, usize, String)> {
    previews
        .into_iter()
        .enumerate()
        .map(|(index, preview)| {
            preview.unwrap_or_else(|| preview::preview_for_match(tree, &ranges[index]))
        })
        .collect()
}

fn update_cached_line_preview(
    text: &str,
    line_start_byte: usize,
    cached_line_start_byte: &mut Option<usize>,
    cached_preview: &mut String,
) {
    if *cached_line_start_byte == Some(line_start_byte) {
        return;
    }

    let line_slice = match text[line_start_byte..].find('\n') {
        Some(relative_end) => &text[line_start_byte..line_start_byte + relative_end],
        None => &text[line_start_byte..],
    };
    let mut bounded = String::new();
    let mut chars = line_slice.chars();
    for _ in 0..PREVIEW_MAX_CHARS {
        let Some(ch) = chars.next() else {
            break;
        };
        bounded.push(ch);
    }
    *cached_preview = compact_preview(&bounded);
    if chars.next().is_some() && !cached_preview.ends_with("...") {
        cached_preview.push_str("...");
    }
    *cached_line_start_byte = Some(line_start_byte);
}
