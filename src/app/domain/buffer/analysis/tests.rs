use super::{
    BufferLength, PieceTreeLite, TextFormatMetadata, display_line_count_from_piece_tree,
    try_analyze_piece_tree_text,
};
use crate::app::domain::PieceSource;

#[test]
fn display_line_count_from_piece_tree_uses_metrics_and_last_char() {
    let empty = PieceTreeLite::from_string(String::new());
    assert_eq!(display_line_count_from_piece_tree(&empty), 0);

    let no_trailing_newline = PieceTreeLite::from_string("one\ntwo".to_owned());
    assert_eq!(display_line_count_from_piece_tree(&no_trailing_newline), 2);

    let trailing_newline = PieceTreeLite::from_string("one\ntwo\n".to_owned());
    assert_eq!(display_line_count_from_piece_tree(&trailing_newline), 2);
}

#[test]
fn display_line_count_from_piece_tree_tracks_edited_buffers() {
    let mut tree = PieceTreeLite::from_string("one\nthree".to_owned());

    tree.insert_with_source(4, "two\n", PieceSource::Edit);
    tree.remove_char_range(0..4);

    assert_eq!(tree.extract_text(), "two\nthree");
    assert_eq!(display_line_count_from_piece_tree(&tree), 2);
}

#[test]
fn analyze_piece_tree_text_returns_metadata_and_cached_length() {
    let tree = PieceTreeLite::from_string("one\ntwo\n".to_owned());
    let mut format = TextFormatMetadata::utf8_for_new_file("");

    let analysis = try_analyze_piece_tree_text(&tree, &mut format).unwrap();

    assert_eq!(analysis.metadata.line_count, 3);
    assert_eq!(
        analysis.length,
        BufferLength {
            bytes: 8,
            chars: 8,
            lines: 2,
        }
    );
    assert_eq!(format.line_ending_counts.lf, 2);
}

#[test]
fn replacement_metadata_matches_materialized_unicode_and_boundary_edits() {
    use super::{buffer_text_metadata, metadata_after_replacements};
    use crate::app::domain::BufferState;
    for original in ["ab\r\n東京z", "α\rβ\nc", ""] {
        let buffer = BufferState::new("metadata".into(), original.into(), None);
        let len = original.chars().count();
        let mut cases = vec![
            vec![],
            vec![(0..len, "\nnew\r\n\u{200b}é".to_owned())],
            vec![(0..0, "\r".to_owned()), (0..0, "\n".to_owned())],
        ];
        if len >= 4 {
            cases.push(vec![
                (len - 1..len, "\x1b\u{0085}".into()),
                (1..3, "\n".into()),
            ]);
        }
        for edits in cases {
            let mut chars: Vec<_> = original.chars().collect();
            for (range, text) in &edits {
                chars.splice(range.clone(), text.chars());
            }
            let text: String = chars.into_iter().collect();
            let actual = metadata_after_replacements(
                buffer.document().piece_tree(),
                &edits,
                &mut buffer.format.clone(),
            )
            .unwrap();
            let expected = buffer_text_metadata(&text, &mut buffer.format.clone());
            assert_eq!(actual, expected, "{original:?} {edits:?}");
        }
        assert!(
            metadata_after_replacements(
                buffer.document().piece_tree(),
                &[(len + 1..len + 1, String::new())],
                &mut buffer.format.clone()
            )
            .is_err()
        );
    }
}
