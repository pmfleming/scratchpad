use super::types::{CharCursor, CursorRange, EditOperation, OperationRecord};
use crate::app::domain::BufferState;
use crate::app::domain::buffer::PieceTreeLite;

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

    let delete_start = if modifiers.alt || modifiers.ctrl {
        find_word_boundary_left(buffer.document().piece_tree(), start)
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
    let total = buffer.document().piece_tree().len_chars();
    if start < end {
        return apply_delete_selection(buffer, cursor);
    }
    if start >= total {
        return *cursor;
    }

    let delete_end = if modifiers.alt || modifiers.ctrl {
        find_word_boundary_right(buffer.document().piece_tree(), start)
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
    let deleted_text = extract_and_delete_range(buffer, start, end);
    let new_cursor = CursorRange::one(CharCursor {
        index: start,
        prefer_next_row: true,
    });
    record_edit(
        buffer,
        cursor,
        new_cursor,
        start,
        deleted_text,
        String::new(),
    );
    new_cursor
}

pub(super) fn apply_cut(buffer: &mut BufferState, cursor: &CursorRange) -> (CursorRange, String) {
    let (start, end) = cursor.sorted_indices();
    let selected = buffer.document().piece_tree().extract_range(start..end);
    let deleted_text = extract_and_delete_range(buffer, start, end);
    let new_cursor = CursorRange::one(CharCursor {
        index: start,
        prefer_next_row: true,
    });
    record_edit(
        buffer,
        cursor,
        new_cursor,
        start,
        deleted_text,
        String::new(),
    );
    (new_cursor, selected)
}

fn apply_char_delete(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    delete_start: usize,
    delete_end: usize,
    prefer_next_row: bool,
) -> CursorRange {
    let deleted_text = buffer
        .document()
        .piece_tree()
        .extract_range(delete_start..delete_end);
    buffer
        .document_mut()
        .delete_char_range_direct(delete_start..delete_end);
    let new_cursor = CursorRange::one(CharCursor {
        index: delete_start,
        prefer_next_row,
    });
    record_edit(
        buffer,
        cursor,
        new_cursor,
        delete_start,
        deleted_text,
        String::new(),
    );
    new_cursor
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
    buffer.document_mut().push_edit_operation(OperationRecord {
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
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                result.push_str(preferred);
            }
            '\n' => result.push_str(preferred),
            _ => result.push(ch),
        }
    }
    result
}

fn find_word_boundary_left(piece_tree: &PieceTreeLite, index: usize) -> usize {
    if index == 0 {
        return 0;
    }
    let text = piece_tree.extract_range(0..index);
    let chars: Vec<char> = text.chars().collect();
    let mut pos = chars.len();
    while pos > 0 && chars[pos - 1].is_whitespace() {
        pos -= 1;
    }
    while pos > 0 && !chars[pos - 1].is_whitespace() {
        pos -= 1;
    }
    pos
}

fn find_word_boundary_right(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let total = piece_tree.len_chars();
    if index >= total {
        return total;
    }
    let text = piece_tree.extract_range(index..total);
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    while pos < chars.len() && !chars[pos].is_whitespace() {
        pos += 1;
    }
    while pos < chars.len() && chars[pos].is_whitespace() {
        pos += 1;
    }
    index + pos
}
