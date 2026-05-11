use super::{
    cursor_line_for_viewport_slice, display_text_slice, editor_eof_tail_height,
    editor_interaction_id, preview_text_slice,
};
use crate::app::domain::{
    BufferState, CursorRevealMode, EditorViewState, SearchReplacementPreview,
    SearchReplacementPreviewEntry,
};
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};

#[test]
fn editor_interaction_id_is_stable_per_view() {
    assert_eq!(editor_interaction_id(7), editor_interaction_id(7));
    assert_ne!(editor_interaction_id(7), editor_interaction_id(8));
}

#[test]
fn eof_tail_does_not_create_blank_scroll_page() {
    assert_eq!(editor_eof_tail_height(600.0, 20.0), 0.0);
}

#[test]
fn viewport_slice_ignores_offscreen_cursor_without_pending_reveal() {
    let buffer = BufferState::new("sample.txt".to_owned(), numbered_lines(200), None);
    let mut view = view_with_cursor_at_line(&buffer, 120);

    assert_eq!(cursor_line_for_viewport_slice(&buffer, &view), None);
    view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    assert_eq!(cursor_line_for_viewport_slice(&buffer, &view), Some(120));
}

#[test]
fn visible_control_text_maps_newline_marker_to_single_document_char() {
    let display = display_text_slice("a\nb", true);
    let map = display.map.as_ref().unwrap();

    assert_eq!(display.text, "a\u{240A}\nb");
    assert_eq!(map.doc_to_display_cursor(1), 1);
    assert_eq!(map.doc_to_display_cursor(2), 3);
    assert_eq!(map.display_to_doc_cursor(1), 1);
    assert_eq!(map.display_to_doc_cursor(2), 1);
    assert_eq!(map.display_to_doc_cursor(3), 2);
}

#[test]
fn visible_unicode_controls_use_private_use_glyphs() {
    let display = display_text_slice("a\u{200E}b", true);
    let map = display.map.as_ref().unwrap();

    assert_eq!(display.text, "a\u{F003}b");
    assert_eq!(map.doc_range_to_display(1..2), Some(1..2));
    assert_eq!(map.display_to_doc_cursor(1), 1);
    assert_eq!(map.display_to_doc_cursor(2), 2);
}

#[test]
fn visible_c0_and_del_controls_use_control_pictures() {
    let display = display_text_slice("\u{0000}\u{001B}\u{007F}", true);

    assert_eq!(display.text, "\u{2400}\u{241B}\u{2421}");
}

#[test]
fn visible_bare_cr_creates_display_row_break() {
    let display = display_text_slice("a\rb", true);
    let map = display.map.as_ref().unwrap();

    assert_eq!(display.text, "a\u{240D}\nb");
    assert_eq!(map.doc_to_display_cursor(1), 1);
    assert_eq!(map.doc_to_display_cursor(2), 3);
    assert_eq!(map.display_to_doc_cursor(2), 1);
    assert_eq!(map.display_to_doc_cursor(3), 2);
}

#[test]
fn preview_text_slice_projects_replacements_without_changing_original_coordinates() {
    let preview = replacement_preview(4..7, "barley");
    let slice = preview_text_slice("foo foo baz", 0..11, Some(&preview));
    let map = slice.map.as_ref().expect("preview map");

    assert_eq!(slice.text, "foo barley baz");
    assert_eq!(map.doc_to_display_cursor(4), 4);
    assert_eq!(map.doc_to_display_cursor(7), 10);
    assert_eq!(map.display_to_doc_cursor(10), 7);
    assert_eq!(map.doc_range_to_display(4..7), Some(4..10));
}

#[test]
fn preview_text_slice_can_project_deletion() {
    let preview = replacement_preview(4..7, "");
    let slice = preview_text_slice("foo foo baz", 0..11, Some(&preview));
    let map = slice.map.as_ref().expect("preview map");

    assert_eq!(slice.text, "foo  baz");
    assert_eq!(map.doc_to_display_cursor(4), 4);
    assert_eq!(map.doc_to_display_cursor(7), 4);
    assert_eq!(map.doc_range_to_display(4..7), None);
}

#[test]
fn preview_text_slice_projects_adjacent_replacements() {
    let preview = SearchReplacementPreview {
        entries: vec![
            SearchReplacementPreviewEntry {
                range: 0..3,
                replacement: "bar".to_owned(),
            },
            SearchReplacementPreviewEntry {
                range: 3..6,
                replacement: "bazooka".to_owned(),
            },
        ],
    };

    let slice = preview_text_slice("fooqux!", 0..7, Some(&preview));
    let map = slice.map.as_ref().expect("preview map");

    assert_eq!(slice.text, "barbazooka!");
    assert_eq!(map.doc_to_display_cursor(0), 0);
    assert_eq!(map.doc_to_display_cursor(3), 3);
    assert_eq!(map.doc_to_display_cursor(6), 10);
    assert_eq!(map.display_to_doc_cursor(10), 6);
}

#[test]
fn preview_text_slice_projects_replacement_at_eof() {
    let preview = replacement_preview(4..7, "barley");
    let slice = preview_text_slice("foo foo", 0..7, Some(&preview));
    let map = slice.map.as_ref().expect("preview map");

    assert_eq!(slice.text, "foo barley");
    assert_eq!(map.doc_to_display_cursor(7), 10);
    assert_eq!(map.display_to_doc_cursor(10), 7);
}

#[test]
fn preview_text_slice_projects_deletion_at_eof() {
    let preview = replacement_preview(4..7, "");
    let slice = preview_text_slice("foo foo", 0..7, Some(&preview));
    let map = slice.map.as_ref().expect("preview map");

    assert_eq!(slice.text, "foo ");
    assert_eq!(map.doc_to_display_cursor(7), 4);
}

#[test]
fn preview_text_slice_ignores_replacement_outside_slice() {
    let preview = replacement_preview(10..13, "BAR");
    let slice = preview_text_slice("01234", 0..5, Some(&preview));

    assert_eq!(slice.text, "01234");
}

#[test]
fn visible_crlf_pins_each_line_ending_char() {
    let display = display_text_slice("a\r\nb", true);
    let map = display.map.as_ref().unwrap();

    assert_eq!(display.text, "a\u{240D}\u{240A}\nb");
    assert_eq!(map.doc_to_display_cursor(1), 1);
    assert_eq!(map.doc_to_display_cursor(2), 2);
    assert_eq!(map.doc_to_display_cursor(3), 4);
    assert_eq!(map.display_to_doc_cursor(2), 2);
    assert_eq!(map.display_to_doc_cursor(3), 2);
    assert_eq!(map.display_to_doc_cursor(4), 3);
}

fn view_with_cursor_at_line(buffer: &BufferState, line: usize) -> EditorViewState {
    let mut view = EditorViewState::new(buffer.id);
    view.cursor_range = Some(CursorRange::one(CharCursor::new(
        buffer.document().piece_tree().line_info(line).start_char,
    )));
    view
}

fn replacement_preview(
    range: std::ops::Range<usize>,
    replacement: &str,
) -> SearchReplacementPreview {
    SearchReplacementPreview {
        entries: vec![SearchReplacementPreviewEntry {
            range,
            replacement: replacement.to_owned(),
        }],
    }
}

fn numbered_lines(count: usize) -> String {
    (0..count)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n")
}
