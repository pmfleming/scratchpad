use super::types::{CharCursor, CursorRange, EditOperation, OperationRecord};
use super::word_boundary;
use crate::app::domain::BufferState;

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
    let total = buffer.document().piece_tree().len_chars();
    if start < end {
        return apply_delete_selection(buffer, cursor);
    }
    if start >= total {
        return *cursor;
    }

    let delete_end = if modifiers.alt || modifiers.ctrl {
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

pub(super) fn apply_outdent(buffer: &mut BufferState, cursor: &CursorRange) -> Option<CursorRange> {
    let piece_tree = buffer.document().piece_tree();
    let caret = cursor.primary.index;
    let line_index = piece_tree.line_index_at_offset(caret);
    let line_info = piece_tree.line_info(line_index);
    let line_start = line_info.start_char;
    let line_end = line_start + line_info.char_len;
    let (line_prefix, _) = piece_tree.extract_range_bounded(line_start..line_end, 4);
    let mut prefix_chars = line_prefix.chars();
    let first_char = prefix_chars.next()?;

    let chars_to_remove = if first_char == '\t' {
        1
    } else if first_char == ' ' {
        line_prefix.chars().take_while(|&c| c == ' ').count()
    } else {
        return None;
    };

    if chars_to_remove == 0 {
        return None;
    }

    let deleted_text = piece_tree.extract_range(line_start..line_start + chars_to_remove);
    buffer
        .document_mut()
        .delete_char_range_direct(line_start..line_start + chars_to_remove);

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
