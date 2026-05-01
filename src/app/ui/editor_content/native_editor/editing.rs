use super::types::{CharCursor, CursorRange, EditOperation, OperationRecord};
use super::word_boundary;
use crate::app::domain::BufferState;

fn is_wordwise_modifier(modifiers: &eframe::egui::Modifiers) -> bool {
    modifiers.alt || modifiers.ctrl
}

pub(super) fn apply_text_insert(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    text: &str,
) -> CursorRange {
    let (start, end) = cursor.sorted_indices();
    let deleted_text = extract_and_delete_range(buffer, start, end);

    let line_ending = buffer.document().preferred_line_ending_str().to_owned();
    let normalized = normalize_line_endings(text, &line_ending);
    let inserted_chars = normalized.chars().count();
    buffer.document_mut().insert_direct(start, &normalized);

    let new_cursor = CursorRange::one(CharCursor::new(start + inserted_chars));
    record_edit(buffer, cursor, new_cursor, start, deleted_text, normalized);
    new_cursor
}

pub(super) fn apply_backspace(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    modifiers: &eframe::egui::Modifiers,
) -> CursorRange {
    let (start, end) = cursor.sorted_indices();
    if start < end {
        return apply_delete_selection(buffer, cursor);
    }
    if start == 0 {
        return *cursor;
    }

    let delete_start = if is_wordwise_modifier(modifiers) {
        word_boundary::find_word_boundary_left(buffer.document().piece_tree(), start)
    } else {
        start - 1
    };

    apply_char_delete(buffer, cursor, delete_start, start, false)
}

pub(super) fn apply_delete(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    modifiers: &eframe::egui::Modifiers,
) -> CursorRange {
    let (start, end) = cursor.sorted_indices();
    let total = buffer.current_file_length().chars;
    if start < end {
        return apply_delete_selection(buffer, cursor);
    }
    if start >= total {
        return *cursor;
    }

    let delete_end = if is_wordwise_modifier(modifiers) {
        word_boundary::find_word_boundary_right(buffer.document().piece_tree(), start)
    } else {
        start + 1
    }
    .min(total);

    apply_char_delete(buffer, cursor, start, delete_end, true)
}

pub(super) fn apply_delete_selection(
    buffer: &mut BufferState,
    cursor: &CursorRange,
) -> CursorRange {
    let (start, end) = cursor.sorted_indices();
    delete_range(buffer, cursor, start, end, true).0
}

pub(super) fn apply_outdent(buffer: &mut BufferState, cursor: &CursorRange) -> Option<CursorRange> {
    let caret = cursor.primary.index;
    let (line_start, line_end) = cursor_line_span(buffer, caret);
    let line_prefix = line_prefix(buffer, line_start, line_end);
    let chars_to_remove = leading_outdent_width(&line_prefix)?;
    let deleted_text = remove_line_prefix(buffer, line_start, chars_to_remove);

    let new_caret = caret.saturating_sub(chars_to_remove);
    let new_cursor = CursorRange::one(CharCursor::new(new_caret));
    record_edit(
        buffer,
        cursor,
        new_cursor,
        line_start,
        deleted_text,
        String::new(),
    );
    Some(new_cursor)
}

fn cursor_line_span(buffer: &BufferState, caret: usize) -> (usize, usize) {
    let piece_tree = buffer.document().piece_tree();
    let line_index = piece_tree.line_index_at_offset(caret);
    let line_info = piece_tree.line_info(line_index);
    let line_start = line_info.start_char;
    (line_start, line_start + line_info.char_len)
}

fn line_prefix(buffer: &BufferState, line_start: usize, line_end: usize) -> String {
    buffer
        .document()
        .piece_tree()
        .extract_range_bounded(line_start..line_end, 4)
        .0
}

fn leading_outdent_width(line_prefix: &str) -> Option<usize> {
    match line_prefix.chars().next()? {
        '\t' => Some(1),
        ' ' => {
            Some(line_prefix.chars().take_while(|&ch| ch == ' ').count()).filter(|&width| width > 0)
        }
        _ => None,
    }
}

fn remove_line_prefix(
    buffer: &mut BufferState,
    line_start: usize,
    chars_to_remove: usize,
) -> String {
    let remove_range = line_start..line_start + chars_to_remove;
    let deleted_text = buffer
        .document()
        .piece_tree()
        .extract_range(remove_range.clone());
    buffer.document_mut().delete_char_range_direct(remove_range);
    deleted_text
}

pub(super) fn apply_cut(buffer: &mut BufferState, cursor: &CursorRange) -> (CursorRange, String) {
    let (start, end) = cursor.sorted_indices();
    delete_range(buffer, cursor, start, end, true)
}

fn apply_char_delete(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    delete_start: usize,
    delete_end: usize,
    prefer_next_row: bool,
) -> CursorRange {
    delete_range(buffer, cursor, delete_start, delete_end, prefer_next_row).0
}

fn delete_range(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    start: usize,
    end: usize,
    prefer_next_row: bool,
) -> (CursorRange, String) {
    let deleted_text = extract_and_delete_range(buffer, start, end);
    let new_cursor = CursorRange::one(CharCursor {
        index: start,
        prefer_next_row,
    });
    record_edit(
        buffer,
        cursor,
        new_cursor,
        start,
        deleted_text.clone(),
        String::new(),
    );
    (new_cursor, deleted_text)
}

fn extract_and_delete_range(buffer: &mut BufferState, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    let text = buffer.document().piece_tree().extract_range(start..end);
    buffer.document_mut().delete_char_range_direct(start..end);
    text
}

fn record_edit(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    new_cursor: CursorRange,
    start_char: usize,
    deleted_text: String,
    inserted_text: String,
) {
    buffer.push_text_edit_operation(OperationRecord {
        previous_cursor: *cursor,
        next_cursor: new_cursor,
        edits: vec![EditOperation {
            start_char,
            deleted_text,
            inserted_text,
        }],
    });
}

fn normalize_line_endings(text: &str, preferred: &str) -> String {
    if !text.contains('\n') && !text.contains('\r') {
        return text.to_owned();
    }

    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        push_normalized_line_char(&mut result, ch, &mut chars, preferred);
    }
    result
}

fn push_normalized_line_char(
    result: &mut String,
    ch: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    preferred: &str,
) {
    match ch {
        '\r' => {
            consume_lf_after_cr(chars);
            result.push_str(preferred);
        }
        '\n' => result.push_str(preferred),
        _ => result.push(ch),
    }
}

fn consume_lf_after_cr(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    if chars.peek() == Some(&'\n') {
        chars.next();
    }
}
