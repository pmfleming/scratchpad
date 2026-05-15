use super::{
    BufferLength, PieceTreeLite, TextFormatMetadata, analyze_piece_tree_text,
    display_line_count_from_piece_tree,
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

    let analysis = analyze_piece_tree_text(&tree, &mut format);

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
