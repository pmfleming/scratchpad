use super::{
    cursor_line_for_viewport_slice, display_text_slice, editor_interaction_id, preview_text_slice,
};
use crate::app::domain::{
    BufferState, CursorRevealMode, EditorViewState, SearchReplacementPreview,
    SearchReplacementPreviewEntry,
};
use crate::app::ui::editor_content::extent;
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};

#[test]
fn editor_interaction_id_is_stable_per_view() {
    assert_eq!(editor_interaction_id(7), editor_interaction_id(7));
    assert_ne!(editor_interaction_id(7), editor_interaction_id(8));
}

#[test]
fn eof_tail_does_not_create_blank_scroll_page() {
    assert_eq!(extent::eof_tail_height(600.0, 20.0), 0.0);
}

#[test]
fn viewport_slice_ignores_offscreen_cursor_without_pending_reveal() {
    let buffer = BufferState::new("sample.txt".to_owned(), numbered_lines(200), None);
    let mut view = view_with_cursor_at_line(&buffer, 120);

    assert_eq!(cursor_line_for_viewport_slice(&buffer, &view), None);
    view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    assert_eq!(cursor_line_for_viewport_slice(&buffer, &view), Some(120));
}

type CursorPairs = &'static [(usize, usize)];
type Replacements = Vec<(std::ops::Range<usize>, &'static str)>;

fn assert_cursor_maps(
    map: &super::DisplayTextMap,
    doc_to_display: CursorPairs,
    display_to_doc: CursorPairs,
    case: &str,
) {
    for &(doc, display) in doc_to_display {
        assert_eq!(map.doc_to_display_cursor(doc), display, "{case}: doc {doc}");
    }
    for &(display, doc) in display_to_doc {
        assert_eq!(
            map.display_to_doc_cursor(display),
            doc,
            "{case}: display {display}"
        );
    }
}

#[test]
fn visible_controls_use_glyphs_and_map_cursors_to_document_chars() {
    let cases: [(&str, &str, CursorPairs, CursorPairs); 5] = [
        // A newline marker plus row break is a single document char.
        (
            "a\nb",
            "a\u{240A}\nb",
            &[(1, 1), (2, 3)],
            &[(1, 1), (2, 1), (3, 2)],
        ),
        // A bare CR still breaks the display row.
        ("a\rb", "a\u{240D}\nb", &[(1, 1), (2, 3)], &[(2, 1), (3, 2)]),
        // CRLF pins each line-ending char.
        (
            "a\r\nb",
            "a\u{240D}\u{240A}\nb",
            &[(1, 1), (2, 2), (3, 4)],
            &[(2, 2), (3, 2), (4, 3)],
        ),
        // Unicode format controls use private-use glyphs.
        (
            "a\u{200E}b",
            "a\u{F003}b",
            &[(1, 1), (2, 2)],
            &[(1, 1), (2, 2)],
        ),
        // C0 and DEL controls use control pictures.
        (
            "\u{0000}\u{001B}\u{007F}",
            "\u{2400}\u{241B}\u{2421}",
            &[],
            &[],
        ),
    ];
    for (text, expected, doc_to_display, display_to_doc) in cases {
        let display = display_text_slice(text, true);
        assert_eq!(display.text, expected, "{text:?}");
        let map = display.map.as_ref().unwrap();
        assert_cursor_maps(map, doc_to_display, display_to_doc, text);
    }
    let map = display_text_slice("a\u{200E}b", true).map.unwrap();
    assert_eq!(map.doc_range_to_display(1..2), Some(1..2));
    assert!(display_text_slice("a\nb", false).map.is_none());
}

#[test]
fn preview_text_slice_projects_replacements_without_changing_original_coordinates() {
    // (slice text, slice start, replacements, projected text, doc->display, display->doc)
    let cases: [(&str, usize, Replacements, &str, CursorPairs, CursorPairs); 7] = [
        (
            "foo foo baz",
            0,
            vec![(4..7, "barley")],
            "foo barley baz",
            &[(4, 4), (7, 10)],
            &[(10, 7)],
        ),
        (
            "foo foo baz",
            0,
            vec![(4..7, "")],
            "foo  baz",
            &[(4, 4), (7, 4)],
            &[],
        ),
        (
            "fooqux!",
            0,
            vec![(0..3, "bar"), (3..6, "bazooka")],
            "barbazooka!",
            &[(0, 0), (3, 3), (6, 10)],
            &[(10, 6)],
        ),
        (
            "foo foo",
            0,
            vec![(4..7, "barley")],
            "foo barley",
            &[(7, 10)],
            &[(10, 7)],
        ),
        ("foo foo", 0, vec![(4..7, "")], "foo ", &[(7, 4)], &[]),
        // Overlapping replacements keep the first.
        (
            "abcdef",
            0,
            vec![(0..3, "X"), (1..4, "Y")],
            "Xdef",
            &[(3, 1)],
            &[],
        ),
        // Document ranges are relative to the slice start.
        (
            "foo baz",
            10,
            vec![(14..17, "quxx")],
            "foo quxx",
            &[(7, 8)],
            &[(8, 7)],
        ),
    ];
    for (text, slice_start, entries, expected, doc_to_display, display_to_doc) in cases {
        let preview = replacement_preview(entries);
        let slice_range = slice_start..slice_start + text.chars().count();
        let slice = preview_text_slice(text, slice_range, Some(&preview));
        assert_eq!(slice.text, expected);
        let map = slice.map.as_ref().expect("preview map");
        assert_cursor_maps(map, doc_to_display, display_to_doc, expected);
    }

    let replaced = preview_text_slice(
        "foo foo baz",
        0..11,
        Some(&replacement_preview(vec![(4..7, "barley")])),
    );
    assert_eq!(
        replaced.map.unwrap().doc_range_to_display(4..7),
        Some(4..10)
    );
    let deleted = preview_text_slice(
        "foo foo baz",
        0..11,
        Some(&replacement_preview(vec![(4..7, "")])),
    );
    assert_eq!(deleted.map.unwrap().doc_range_to_display(4..7), None);
}

#[test]
fn preview_text_slice_ignores_replacement_outside_slice() {
    let preview = replacement_preview(vec![(10..13, "BAR")]);
    let slice = preview_text_slice("01234", 0..5, Some(&preview));

    assert_eq!(slice.text, "01234");
    assert!(slice.map.is_none());
    assert!(preview_text_slice("01234", 0..5, None).map.is_none());
}

fn view_with_cursor_at_line(buffer: &BufferState, line: usize) -> EditorViewState {
    let mut view = EditorViewState::new(buffer.id);
    view.cursor_range = Some(CursorRange::one(CharCursor::new(
        buffer.document().piece_tree().line_info(line).start_char,
    )));
    view
}

fn replacement_preview(entries: Replacements) -> SearchReplacementPreview {
    SearchReplacementPreview::new(
        entries
            .into_iter()
            .map(|(range, replacement)| SearchReplacementPreviewEntry {
                range,
                replacement: replacement.to_owned(),
            })
            .collect(),
    )
}

fn numbered_lines(count: usize) -> String {
    (0..count)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n")
}
