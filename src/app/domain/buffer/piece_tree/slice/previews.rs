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
    if !match_starts.is_sorted() {
        return ranges
            .iter()
            .map(|range| preview::preview_for_match(tree, range))
            .collect();
    }

    let mut scan = PiecePreviewScan::new(match_starts);
    for span in tree.spans_for_range(0..tree.len_chars()) {
        if scan.push_span(span.char_start, &span.text) {
            break;
        }
    }
    scan.finish_document();
    collect_piece_previews(tree, ranges, scan.previews)
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

struct PiecePreviewScan {
    match_starts: Vec<usize>,
    previews: Vec<Option<(usize, usize, String)>>,
    pending: Vec<PendingPiecePreview>,
    line: PiecePreviewLine,
    cursor: PiecePreviewCursor,
    next_match: usize,
}

impl PiecePreviewScan {
    fn new(match_starts: Vec<usize>) -> Self {
        Self {
            previews: vec![None; match_starts.len()],
            match_starts,
            pending: Vec::new(),
            line: PiecePreviewLine::default(),
            cursor: PiecePreviewCursor::default(),
            next_match: 0,
        }
    }

    fn push_span(&mut self, char_start: usize, text: &str) -> bool {
        self.cursor.current_char = self.cursor.current_char.max(char_start);
        text.chars().any(|ch| self.push_char(ch))
    }

    fn push_char(&mut self, ch: char) -> bool {
        self.queue_matches();
        if self.try_finish_truncated_line() {
            return true;
        }
        if ch == '\n' {
            self.finish_line();
            if self.all_matches_queued() {
                return true;
            }
            self.cursor.advance_line();
            self.line.clear();
        } else {
            self.line.push(ch);
            self.cursor.current_char += 1;
            return self.try_finish_truncated_line();
        }
        false
    }

    fn queue_matches(&mut self) {
        while self
            .match_starts
            .get(self.next_match)
            .is_some_and(|start| *start <= self.cursor.current_char)
        {
            let start = self.match_starts[self.next_match];
            self.pending.push(PendingPiecePreview {
                index: self.next_match,
                line_number: self.cursor.line_number(),
                column_number: start.saturating_sub(self.cursor.line_start_char) + 1,
            });
            self.next_match += 1;
        }
    }

    fn try_finish_truncated_line(&mut self) -> bool {
        let complete = self.all_matches_queued() && !self.pending.is_empty() && self.line.truncated;
        if complete {
            self.finish_line();
        }
        complete
    }

    fn all_matches_queued(&self) -> bool {
        self.next_match == self.match_starts.len()
    }

    fn finish_document(&mut self) {
        self.queue_matches();
        self.finish_line();
    }

    fn finish_line(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let preview = self.line.preview();
        for pending in self.pending.drain(..) {
            self.previews[pending.index] =
                Some((pending.line_number, pending.column_number, preview.clone()));
        }
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
