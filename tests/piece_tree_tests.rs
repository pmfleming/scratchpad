use scratchpad::app::domain::buffer::{PieceTreeCharPosition, PieceTreeLineInfo, PieceTreeLite};

#[test]
fn normalized_ranges_are_half_open_clamped_and_ascending() {
    let tree = PieceTreeLite::from_string("abcdef".to_owned());

    #[allow(clippy::reversed_empty_ranges)]
    {
        assert_eq!(tree.normalize_char_range(5..2), 2..5);
    }
    assert_eq!(tree.normalize_char_range(3..99), 3..6);
    #[allow(clippy::reversed_empty_ranges)]
    {
        assert_eq!(tree.normalize_char_range(99..3), 3..6);
    }
}

#[test]
fn char_positions_report_zero_based_scalar_columns() {
    let tree = PieceTreeLite::from_string("a\nbéta".to_owned());

    assert_eq!(
        tree.char_position(4),
        PieceTreeCharPosition {
            offset_chars: 4,
            line_index: 1,
            column_index: 2,
        }
    );
    assert_eq!(
        tree.line_info(1),
        PieceTreeLineInfo {
            line_index: 1,
            start_char: 2,
            char_len: 4,
        }
    );
}

#[test]
fn unicode_insert_and_extract_keep_char_coordinates() {
    let mut tree = PieceTreeLite::from_string("aé\n🙂z".to_owned());
    tree.insert(2, "λ");

    assert_eq!(tree.len_chars(), 6);
    assert_eq!(tree.extract_range(0..6), "aéλ\n🙂z");
    assert_eq!(tree.line_index_at_offset(4), 1);
}

#[test]
fn unicode_remove_range_spanning_pieces_is_char_safe() {
    let mut tree = PieceTreeLite::from_string("alpha🙂beta\ngamma".to_owned());
    tree.remove_char_range(5..10);

    assert_eq!(tree.extract_range(0..tree.len_chars()), "alpha\ngamma");
    assert_eq!(tree.metrics().newlines, 1);
}

#[test]
fn preview_and_line_lookup_work_on_unicode_content() {
    let text = "zero\nhéllo needle κόσμε\nlast".to_owned();
    let match_byte = text.find("needle").expect("needle present");
    let match_char = text[..match_byte].chars().count();
    let tree = PieceTreeLite::from_string(text);

    let (line, column, preview) = tree.preview_for_match(&(match_char..match_char + 6));
    assert_eq!(line, 2);
    assert_eq!(column, 7);
    assert!(preview.contains("needle"));

    let (line_start, line_len) = tree.line_lookup(1);
    assert_eq!(
        tree.extract_range(line_start..line_start + line_len),
        "héllo needle κόσμε"
    );
}

#[test]
fn combining_marks_follow_scalar_value_coordinates() {
    let mut tree = PieceTreeLite::from_string("Cafe\u{301}\n".to_owned());
    tree.insert(4, "!");

    assert_eq!(tree.extract_range(0..tree.len_chars()), "Cafe!\u{301}\n");
    assert_eq!(
        tree.char_position(5),
        PieceTreeCharPosition {
            offset_chars: 5,
            line_index: 0,
            column_index: 5,
        }
    );
}

#[test]
fn range_spans_preserve_piece_boundaries_without_allocating() {
    let mut tree = PieceTreeLite::from_string("alpha beta".to_owned());
    tree.insert(5, "🙂");
    tree.insert(6, "!");

    let spans = tree
        .spans_for_range(3..9)
        .map(|span| span.text.to_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        spans,
        vec!["ha".to_owned(), "🙂!".to_owned(), " b".to_owned()]
    );
}

#[test]
fn bounded_extraction_reports_truncation() {
    let tree = PieceTreeLite::from_string("0123456789".repeat(20));

    let (text, truncated) = tree.extract_range_bounded(0..tree.len_chars(), 16);

    assert_eq!(text, "0123456789012345");
    assert!(truncated);
}

#[test]
fn preview_marks_truncated_lines() {
    let long_line = format!("needle {}", "x".repeat(140));
    let tree = PieceTreeLite::from_string(long_line.clone());
    let match_char = long_line.find("needle").expect("needle present");
    let (line, column, preview) = tree.preview_for_match(&(match_char..match_char + 6));

    assert_eq!(line, 1);
    assert_eq!(column, 1);
    assert!(preview.starts_with("needle "));
    assert!(preview.ends_with("..."));
}

#[test]
fn line_spans_match_line_lookup_ranges() {
    let tree = PieceTreeLite::from_string("one\ntwo🙂\nthree".to_owned());
    let line_info = tree.line_info(1);
    let from_spans = tree
        .spans_for_line(1)
        .map(|span| span.text)
        .collect::<String>();

    assert_eq!(from_spans, "two🙂");
    assert_eq!(
        tree.extract_range(line_info.start_char..line_info.start_char + line_info.char_len),
        from_spans
    );
}

// ── Unicode edge-case coverage ──

#[test]
fn zwj_emoji_sequence_uses_scalar_value_counting() {
    // 👨‍👩‍👧‍👦 = U+1F468 ZWJ U+1F469 ZWJ U+1F467 ZWJ U+1F466 = 7 scalar values
    let zwj_family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    assert_eq!(zwj_family.chars().count(), 7);

    let text = format!("a{zwj_family}b\nline2");
    let tree = PieceTreeLite::from_string(text);
    // 'a'(1) + 7 scalars + 'b'(1) + '\n'(1) + "line2"(5) = 15 chars total
    assert_eq!(tree.len_chars(), 15);
    assert_eq!(
        tree.char_position(8),
        PieceTreeCharPosition {
            offset_chars: 8,
            line_index: 0,
            column_index: 8,
        }
    );
    // 'b' is at index 8, '\n' at 9, "line2" starts at 10
    assert_eq!(tree.line_info(1).start_char, 10);
    assert_eq!(tree.extract_range(1..8), zwj_family);
}

#[test]
fn all_multibyte_content_stress() {
    // Document with zero ASCII: Greek + emoji + CJK
    let line1 = "αβγδεζηθ";
    let line2 = "🙂🎉🚀💡";
    let line3 = "你好世界";
    let text = format!("{line1}\n{line2}\n{line3}");
    let mut tree = PieceTreeLite::from_string(text.clone());

    let expected_chars = text.chars().count();
    assert_eq!(tree.len_chars(), expected_chars);
    assert_eq!(tree.metrics().newlines, 2);

    // Verify all three lines via line_lookup
    assert_eq!(
        tree.extract_range(
            tree.line_info(0).start_char..tree.line_info(0).start_char + tree.line_info(0).char_len
        ),
        line1,
    );
    assert_eq!(
        tree.extract_range(
            tree.line_info(1).start_char..tree.line_info(1).start_char + tree.line_info(1).char_len
        ),
        line2,
    );
    assert_eq!(
        tree.extract_range(
            tree.line_info(2).start_char..tree.line_info(2).start_char + tree.line_info(2).char_len
        ),
        line3,
    );

    // Insert at start, middle of line2, and end
    tree.insert(0, "λ");
    let line2_start = tree.line_info(1).start_char;
    tree.insert(line2_start + 2, "★");
    tree.insert(tree.len_chars(), "ω");

    // Round-trip: extract full text, rebuild, compare
    let full = tree.extract_range(0..tree.len_chars());
    let rebuilt = PieceTreeLite::from_string(full.clone());
    assert_eq!(rebuilt.extract_range(0..rebuilt.len_chars()), full);
    assert_eq!(rebuilt.metrics().newlines, 2);
}

#[test]
fn insert_delete_at_piece_boundary_near_multibyte() {
    // Create a tree with a known piece boundary by inserting in the middle
    let mut tree = PieceTreeLite::from_string("αβ🙂γδ".to_owned());
    // Insert creates piece split: ["αβ🙂"] + ["ε"] + ["γδ"]
    tree.insert(3, "ε");
    assert_eq!(tree.extract_range(0..tree.len_chars()), "αβ🙂εγδ");

    // Delete across the piece boundary: remove 🙂ε (chars 2..4)
    tree.remove_char_range(2..4);
    assert_eq!(tree.extract_range(0..tree.len_chars()), "αβγδ");
    assert_eq!(tree.len_chars(), 4);

    // Insert at the exact boundary again
    tree.insert(2, "🎉");
    assert_eq!(tree.extract_range(0..tree.len_chars()), "αβ🎉γδ");

    // Verify char positions across the boundary
    assert_eq!(tree.char_position(2).column_index, 2);
    assert_eq!(tree.char_position(3).column_index, 3);
}

#[test]
fn high_plane_characters_beyond_basic_emoji() {
    // U+10000 LINEAR B SYLLABLE B008 A (4 bytes)
    // U+1D11E MUSICAL SYMBOL G CLEF (4 bytes)
    // U+1F600 GRINNING FACE (4 bytes)
    // U+2070E CJK UNIFIED IDEOGRAPH (4 bytes)
    let high_plane = "\u{10000}\u{1D11E}\u{1F600}\u{2070E}";
    assert_eq!(high_plane.len(), 16); // 4 chars × 4 bytes each
    assert_eq!(high_plane.chars().count(), 4);

    let text = format!("ab\n{high_plane}\ncd");
    let mut tree = PieceTreeLite::from_string(text);
    assert_eq!(tree.len_chars(), 10); // a b \n 4×high \n c d
    assert_eq!(tree.metrics().newlines, 2);

    // Line 1 should be the high-plane chars
    let info = tree.line_info(1);
    assert_eq!(info.char_len, 4);
    assert_eq!(
        tree.extract_range(info.start_char..info.start_char + info.char_len),
        high_plane
    );

    // Insert between two 4-byte chars
    tree.insert(info.start_char + 2, "x");
    let updated_info = tree.line_info(1);
    assert_eq!(updated_info.char_len, 5);
    assert_eq!(
        tree.extract_range(
            updated_info.start_char..updated_info.start_char + updated_info.char_len
        ),
        format!("\u{10000}\u{1D11E}x\u{1F600}\u{2070E}"),
    );

    // Delete the inserted char
    tree.remove_char_range(info.start_char + 2..info.start_char + 3);
    let restored_info = tree.line_info(1);
    assert_eq!(restored_info.char_len, 4);
    assert_eq!(
        tree.extract_range(
            restored_info.start_char..restored_info.start_char + restored_info.char_len
        ),
        high_plane,
    );
}

#[test]
fn mixed_width_chars_line_lookup_consistency() {
    // Mix 1-byte, 2-byte, 3-byte, and 4-byte characters across lines
    let lines = [
        "hello",                // 1-byte only
        "café",                 // 1+2 byte mix
        "日本語",               // 3-byte CJK
        "🙂🎉🚀",               // 4-byte emoji
        "a\u{0301}β\u{1F600}γ", // combining + mixed
    ];
    let text = lines.join("\n");
    let tree = PieceTreeLite::from_string(text.clone());

    assert_eq!(tree.metrics().newlines, 4);

    // Verify every line round-trips correctly
    for (i, expected_line) in lines.iter().enumerate() {
        let info = tree.line_info(i);
        let extracted = tree.extract_range(info.start_char..info.start_char + info.char_len);
        assert_eq!(
            &extracted, expected_line,
            "line {i} mismatch: expected {expected_line:?}, got {extracted:?}"
        );
    }

    // Verify char_position at the start of each line
    for i in 0..lines.len() {
        let info = tree.line_info(i);
        let pos = tree.char_position(info.start_char);
        assert_eq!(pos.line_index, i, "char_position line mismatch at line {i}");
        assert_eq!(
            pos.column_index, 0,
            "char_position column mismatch at line {i}"
        );
    }
}
