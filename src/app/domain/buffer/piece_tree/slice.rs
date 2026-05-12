use super::{
    ByteSpan, PREVIEW_MAX_CHARS, PieceTreeLite, PieceTreeSlice, PieceTreeSpan,
    byte_range_for_char_range, compact_preview,
};
use std::ops::Range;

impl PieceTreeLite {
    pub fn spans_for_line(&self, target_line: usize) -> PieceTreeSlice<'_> {
        let line_info = self.line_info(target_line);
        self.spans_for_range(line_info.start_char..line_info.start_char + line_info.char_len)
    }

    pub fn spans_for_range(&self, range_chars: Range<usize>) -> PieceTreeSlice<'_> {
        let normalized = self.normalize_char_range(range_chars);
        if normalized.is_empty() || self.len_chars() == 0 {
            return PieceTreeSlice::empty(self, normalized);
        }

        let address = self.find_leaf_for_char_offset(normalized.start);
        PieceTreeSlice {
            tree: self,
            range_chars: normalized,
            node_index: address.node_index,
            leaf_index: address.leaf_index,
            piece_index: 0,
            current_char: address.leaf_start_char,
            finished: false,
        }
    }
}

impl<'a> PieceTreeSlice<'a> {
    fn empty(tree: &'a PieceTreeLite, range_chars: Range<usize>) -> Self {
        let current_char = range_chars.start;
        Self {
            tree,
            range_chars,
            node_index: 0,
            leaf_index: 0,
            piece_index: 0,
            current_char,
            finished: true,
        }
    }

    fn advance_piece_cursor(&mut self) {
        if self.finished || self.node_index >= self.tree.root.nodes.len() {
            self.finished = true;
            return;
        }

        let node = &self.tree.root.nodes[self.node_index];
        if self.leaf_index >= node.leaves.len() {
            self.node_index += 1;
            self.leaf_index = 0;
            self.piece_index = 0;
            if self.node_index >= self.tree.root.nodes.len() {
                self.finished = true;
            }
            return;
        }

        let leaf = &node.leaves[self.leaf_index];
        self.piece_index += 1;
        if self.piece_index >= leaf.pieces.len() {
            self.leaf_index += 1;
            self.piece_index = 0;
            if self.leaf_index >= node.leaves.len() {
                self.node_index += 1;
                self.leaf_index = 0;
                if self.node_index >= self.tree.root.nodes.len() {
                    self.finished = true;
                }
            }
        }
    }
}

impl<'a> Iterator for PieceTreeSlice<'a> {
    type Item = PieceTreeSpan<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.finished {
            let node = self.tree.root.nodes.get(self.node_index)?;
            let leaf = node.leaves.get(self.leaf_index)?;
            let piece = match leaf.pieces.get(self.piece_index) {
                Some(piece) => piece,
                None => {
                    self.advance_piece_cursor();
                    continue;
                }
            };

            let piece_start_char = self.current_char;
            let piece_end_char = piece_start_char + piece.char_len;
            self.current_char = piece_end_char;
            self.advance_piece_cursor();

            if piece_end_char <= self.range_chars.start {
                continue;
            }
            if piece_start_char >= self.range_chars.end {
                self.finished = true;
                return None;
            }

            let local_start = self.range_chars.start.saturating_sub(piece_start_char);
            let local_end = (self.range_chars.end.min(piece_end_char)) - piece_start_char;
            let text = self.tree.piece_text(piece);

            if local_start == 0 && local_end == piece.char_len {
                return Some(PieceTreeSpan {
                    text,
                    char_start: piece_start_char,
                    char_len: piece.char_len,
                    byte_span: ByteSpan {
                        buffer: piece.buffer,
                        start_byte: piece.start_byte.min(u32::MAX as usize) as u32,
                        byte_len: piece.byte_len.min(u32::MAX as usize) as u32,
                    },
                });
            }

            let byte_range = if piece.is_ascii {
                local_start..local_end
            } else {
                byte_range_for_char_range(text, local_start, local_end)
            };
            let start_byte = piece.start_byte + byte_range.start;
            let byte_len = byte_range.len();
            return Some(PieceTreeSpan {
                text: &text[byte_range],
                char_start: piece_start_char + local_start,
                char_len: local_end - local_start,
                byte_span: ByteSpan {
                    buffer: piece.buffer,
                    start_byte: start_byte.min(u32::MAX as usize) as u32,
                    byte_len: byte_len.min(u32::MAX as usize) as u32,
                },
            });
        }

        None
    }
}

pub(super) fn previews_for_matches_in_contiguous_text(
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

pub(super) fn previews_for_matches_in_piece_spans(
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
            .map(|range| tree.preview_for_match(range))
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
        .map(|(index, preview)| preview.unwrap_or_else(|| tree.preview_for_match(&ranges[index])))
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
