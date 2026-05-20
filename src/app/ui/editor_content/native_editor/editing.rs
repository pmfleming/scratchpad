use super::types::{CharCursor, CursorRange, EditOperation, OperationRecord};
use super::word_boundary;
use crate::app::domain::buffer::ByteSpan;
use crate::app::domain::{BufferState, PieceSource};
use std::borrow::Cow;

struct RecordedEdit {
    new_cursor: CursorRange,
    start_char: usize,
    deleted_text: String,
    inserted_text: String,
    deleted_spans: Vec<ByteSpan>,
}

fn is_wordwise_modifier(modifiers: &eframe::egui::Modifiers) -> bool {
    modifiers.alt || modifiers.ctrl
}

pub(super) fn apply_text_insert_with_source(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    text: &str,
    source: PieceSource,
) -> CursorRange {
    let (start, end) = cursor.sorted_indices();
    let (deleted_text, deleted_spans) = extract_spans_and_delete_range(buffer, start, end);

    let normalized = normalize_line_endings(text, buffer.document().preferred_line_ending_str());
    let inserted_chars = normalized.chars().count();
    buffer
        .document_mut()
        .insert_direct_with_source(start, &normalized, source);

    let new_cursor = CursorRange::one(CharCursor::new(start + inserted_chars));
    record_edit(
        buffer,
        cursor,
        RecordedEdit {
            new_cursor,
            start_char: start,
            deleted_text,
            inserted_text: normalized.into_owned(),
            deleted_spans,
        },
        source,
    );
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
    let (deleted_text, deleted_spans) = remove_line_prefix(buffer, line_start, chars_to_remove);

    let new_caret = caret.saturating_sub(chars_to_remove);
    let new_cursor = CursorRange::one(CharCursor::new(new_caret));
    record_edit(
        buffer,
        cursor,
        RecordedEdit {
            new_cursor,
            start_char: line_start,
            deleted_text,
            inserted_text: String::new(),
            deleted_spans,
        },
        PieceSource::Edit,
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
) -> (String, Vec<ByteSpan>) {
    let remove_range = line_start..line_start + chars_to_remove;
    let deleted_text = buffer
        .document()
        .piece_tree()
        .extract_range_with_capacity(remove_range.clone(), remove_range.len());
    let deleted_spans = buffer.document().byte_spans_for_range(remove_range.clone());
    buffer.document_mut().delete_char_range_direct(remove_range);
    (deleted_text, deleted_spans)
}

pub(super) fn apply_cut(buffer: &mut BufferState, cursor: &CursorRange) -> (CursorRange, String) {
    let (start, end) = cursor.sorted_indices();
    delete_range_with_source(buffer, cursor, start, end, true, PieceSource::Cut)
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
    delete_range_with_source(
        buffer,
        cursor,
        start,
        end,
        prefer_next_row,
        PieceSource::Edit,
    )
}

fn delete_range_with_source(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    start: usize,
    end: usize,
    prefer_next_row: bool,
    source: PieceSource,
) -> (CursorRange, String) {
    let (deleted_text, deleted_spans) = extract_spans_and_delete_range(buffer, start, end);
    let new_cursor = CursorRange::one(CharCursor {
        index: start,
        prefer_next_row,
    });
    record_edit(
        buffer,
        cursor,
        RecordedEdit {
            new_cursor,
            start_char: start,
            deleted_text: deleted_text.clone(),
            inserted_text: String::new(),
            deleted_spans,
        },
        source,
    );
    (new_cursor, deleted_text)
}

fn extract_spans_and_delete_range(
    buffer: &mut BufferState,
    start: usize,
    end: usize,
) -> (String, Vec<ByteSpan>) {
    if start >= end {
        return (String::new(), Vec::new());
    }
    let text = buffer
        .document()
        .piece_tree()
        .extract_range_with_capacity(start..end, end - start);
    let spans = buffer.document().byte_spans_for_range(start..end);
    buffer.document_mut().delete_char_range_direct(start..end);
    (text, spans)
}

fn record_edit(
    buffer: &mut BufferState,
    cursor: &CursorRange,
    edit: RecordedEdit,
    source: PieceSource,
) {
    buffer.push_text_edit_operation_with_source(
        OperationRecord {
            previous_cursor: *cursor,
            next_cursor: edit.new_cursor,
            edits: vec![EditOperation {
                start_char: edit.start_char,
                deleted_text: edit.deleted_text,
                inserted_text: edit.inserted_text,
                deleted_spans: edit.deleted_spans,
            }],
        },
        source,
    );
}

fn normalize_line_endings<'a>(text: &'a str, preferred: &str) -> Cow<'a, str> {
    if !text.contains('\n') && !text.contains('\r') {
        return Cow::Borrowed(text);
    }
    if preferred == "\n" && !text.contains('\r') {
        return Cow::Borrowed(text);
    }

    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        push_normalized_line_char(&mut result, ch, &mut chars, preferred);
    }
    Cow::Owned(result)
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

#[cfg(test)]
mod tests {
    use super::{
        BufferState, CharCursor, CursorRange, apply_backspace, apply_cut, apply_delete,
        apply_delete_selection, apply_outdent, apply_text_insert_with_source,
    };
    use crate::app::domain::PieceSource;
    use eframe::egui;

    fn buffer(text: &str) -> BufferState {
        BufferState::new("sample.txt".to_owned(), text.to_owned(), None)
    }

    fn cursor(index: usize) -> CursorRange {
        CursorRange::one(CharCursor::new(index))
    }

    fn selection(start: usize, end: usize) -> CursorRange {
        CursorRange::two(start, end)
    }

    fn modifiers(mut set: impl FnMut(&mut egui::Modifiers)) -> egui::Modifiers {
        let mut modifiers = egui::Modifiers::default();
        set(&mut modifiers);
        modifiers
    }

    #[test]
    fn insert_replaces_selection_and_records_deleted_text() {
        let mut buffer = buffer("hello world");
        let next = apply_text_insert_with_source(
            &mut buffer,
            &selection(6, 11),
            "there",
            PieceSource::Edit,
        );

        assert_eq!(buffer.text(), "hello there");
        assert_eq!(next, cursor(11));
        let record = buffer.document().latest_operation_record().unwrap();
        assert_eq!(record.previous_selection, selection(6, 11));
        assert_eq!(record.next_selection, cursor(11));
        assert_eq!(record.edits[0].start_char, 6);
        assert_eq!(record.edits[0].deleted_text, "world");
        assert_eq!(record.edits[0].inserted_text, "there");
    }

    #[test]
    fn pasted_newlines_follow_buffer_preferred_line_ending() {
        let mut buffer = buffer("alpha\r\nomega");

        apply_text_insert_with_source(&mut buffer, &cursor(7), "beta\ngamma", PieceSource::Paste);

        assert_eq!(buffer.text(), "alpha\r\nbeta\r\ngammaomega");
        assert_eq!(
            buffer.document().history_entries().last().unwrap().source,
            PieceSource::Paste
        );
    }

    #[test]
    fn backspace_deletes_previous_scalar_value() {
        let mut buffer = buffer("a😀b");

        let next = apply_backspace(&mut buffer, &cursor(2), &egui::Modifiers::default());

        assert_eq!(buffer.text(), "ab");
        assert_eq!(next, cursor(1));
        assert_eq!(
            buffer.document().latest_operation_record().unwrap().edits[0].deleted_text,
            "😀"
        );
    }

    #[test]
    fn delete_removes_next_scalar_value() {
        let mut buffer = buffer("a😀b");

        let next = apply_delete(&mut buffer, &cursor(1), &egui::Modifiers::default());

        assert_eq!(buffer.text(), "ab");
        assert_eq!(next.primary.index, 1);
        assert!(next.primary.prefer_next_row);
    }

    #[test]
    fn wordwise_backspace_removes_previous_word_after_whitespace() {
        let mut buffer = buffer("alpha  beta");
        let wordwise = modifiers(|modifiers| modifiers.ctrl = true);

        let next = apply_backspace(&mut buffer, &cursor(11), &wordwise);

        assert_eq!(buffer.text(), "alpha  ");
        assert_eq!(next, cursor(7));
    }

    #[test]
    fn wordwise_delete_removes_current_word_and_following_whitespace() {
        let mut buffer = buffer("alpha  beta");
        let wordwise = modifiers(|modifiers| modifiers.ctrl = true);

        let next = apply_delete(&mut buffer, &cursor(0), &wordwise);

        assert_eq!(buffer.text(), "beta");
        assert_eq!(next.primary.index, 0);
    }

    #[test]
    fn delete_selection_collapses_to_start_with_next_row_preference() {
        let mut buffer = buffer("alpha beta");

        let next = apply_delete_selection(&mut buffer, &selection(5, 10));

        assert_eq!(buffer.text(), "alpha");
        assert_eq!(next.primary.index, 5);
        assert!(next.primary.prefer_next_row);
    }

    #[test]
    fn cut_records_cut_source_and_returns_document_text() {
        let mut buffer = buffer("a\u{200e}\n\tb");

        let (next, cut) = apply_cut(&mut buffer, &selection(1, 4));

        assert_eq!(cut, "\u{200e}\n\t");
        assert_eq!(buffer.text(), "ab");
        assert_eq!(next.primary.index, 1);
        assert_eq!(
            buffer.document().history_entries().last().unwrap().source,
            PieceSource::Cut
        );
    }

    #[test]
    fn outdent_removes_leading_tab_on_current_line() {
        let mut buffer = buffer("one\n\tword");

        let next = apply_outdent(&mut buffer, &cursor(4)).unwrap();

        assert_eq!(buffer.text(), "one\nword");
        assert_eq!(next, cursor(3));
    }

    #[test]
    fn outdent_removes_up_to_four_leading_spaces() {
        let mut buffer = buffer("    indented");

        let next = apply_outdent(&mut buffer, &cursor(6)).unwrap();

        assert_eq!(buffer.text(), "indented");
        assert_eq!(next, cursor(2));
    }

    #[test]
    fn outdent_without_indent_is_noop() {
        let mut buffer = buffer("plain");

        assert_eq!(apply_outdent(&mut buffer, &cursor(2)), None);
        assert_eq!(buffer.text(), "plain");
        assert_eq!(buffer.document().operation_undo_depth(), 0);
    }
}
