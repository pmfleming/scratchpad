use super::{
    AnchorBias, AnchorOwner, AnchorOwnerKind, MAX_LEAF_BYTES, MAX_LEAF_PIECES,
    MAX_LEAVES_PER_INTERNAL, MIN_LEAVES_PER_INTERNAL, PieceTreeLite,
};
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;

#[test]
fn anchor_follows_content_across_inserts_and_removals() {
    let mut tree = PieceTreeLite::from_string("hello world".to_owned());
    let anchor = tree.create_anchor(6, AnchorBias::Right);

    tree.insert(0, "[prefix] ");
    assert_eq!(
        tree.anchor_position(anchor),
        Some(6 + "[prefix] ".chars().count())
    );

    tree.remove_char_range(0..3);
    assert_eq!(
        tree.anchor_position(anchor),
        Some(6 + "[prefix] ".chars().count() - 3)
    );
}

#[test]
fn anchor_left_bias_stays_at_split_point_under_insertion() {
    let mut tree = PieceTreeLite::from_string("ab".to_owned());
    let left = tree.create_anchor(1, AnchorBias::Left);
    let right = tree.create_anchor(1, AnchorBias::Right);

    tree.insert(1, "XYZ");

    assert_eq!(tree.anchor_position(left), Some(1));
    assert_eq!(tree.anchor_position(right), Some(4));
}

#[test]
fn anchor_inside_removed_range_collapses_to_start() {
    let mut tree = PieceTreeLite::from_string("abcdefghij".to_owned());
    let anchor = tree.create_anchor(5, AnchorBias::Left);

    tree.remove_char_range(3..8);

    assert_eq!(tree.anchor_position(anchor), Some(3));
}

#[test]
fn anchor_release_drops_anchor_from_tree() {
    let mut tree = PieceTreeLite::from_string("abc".to_owned());
    let anchor = tree.create_anchor(2, AnchorBias::Left);
    tree.release_anchor(anchor);
    assert_eq!(tree.anchor_position(anchor), None);
}

#[test]
fn anchor_clamps_creation_offset_to_document_length() {
    let mut tree = PieceTreeLite::from_string("abc".to_owned());
    let anchor = tree.create_anchor(99, AnchorBias::Left);
    assert_eq!(tree.anchor_position(anchor), Some(3));
}

#[test]
fn anchor_owner_metadata_survives_edits_until_release() {
    let mut tree = PieceTreeLite::from_string("abcdef".to_owned());
    let owner = AnchorOwner::view_scroll(42);
    let anchor = tree.create_anchor_with_owner(3, AnchorBias::Left, owner);

    assert_eq!(tree.anchor_owner(anchor), Some(owner));
    assert_eq!(
        tree.anchor_owner(anchor).map(AnchorOwner::kind),
        Some(AnchorOwnerKind::ViewScroll)
    );

    tree.insert(0, "zz");
    assert_eq!(tree.anchor_position(anchor), Some(5));
    assert_eq!(tree.anchor_owner(anchor), Some(owner));

    tree.release_anchor(anchor);
    assert_eq!(tree.anchor_owner(anchor), None);
}

#[test]
fn anchor_on_empty_document_survives_first_insert() {
    let mut tree = PieceTreeLite::from_string(String::new());
    let left = tree.create_anchor(0, AnchorBias::Left);
    let right = tree.create_anchor(0, AnchorBias::Right);

    tree.insert(0, "abc");

    assert_eq!(tree.anchor_position(left), Some(0));
    assert_eq!(tree.anchor_position(right), Some(3));
    assert_eq!(tree.live_anchor_count(), 2);
    assert_balanced(&tree);
}

#[test]
fn anchors_in_untouched_leaves_shift_through_prefix_metrics() {
    let chunk = "a".repeat(MAX_LEAF_BYTES / 2);
    let mut text = String::new();
    text.push_str(&chunk);
    text.push_str(&chunk);
    text.push_str(&chunk);
    let mut tree = PieceTreeLite::from_string(text);
    assert!(
        tree.root
            .nodes
            .iter()
            .map(|node| node.leaves.len())
            .sum::<usize>()
            > 1
    );

    let distant_offset = tree.len_chars() - 10;
    let distant_anchor = tree.create_anchor(distant_offset, AnchorBias::Left);
    tree.insert(1, "XYZ");

    assert_eq!(
        tree.anchor_position(distant_anchor),
        Some(distant_offset + 3)
    );
    assert_eq!(tree.live_anchor_count(), 1);
    assert_balanced(&tree);
}

#[test]
fn many_point_anchors_match_string_model_across_edits() {
    let mut tree = PieceTreeLite::from_string("0123456789".repeat(2_000));
    let mut anchors = Vec::new();

    for index in 0..1_000 {
        let offset = index * 17;
        let bias = if index % 3 == 0 {
            AnchorBias::Right
        } else {
            AnchorBias::Left
        };
        let anchor = tree.create_anchor(offset, bias);
        anchors.push((anchor, offset, bias));
    }

    tree.insert(123, "abcdef");
    for (_, expected, bias) in &mut anchors {
        if *expected > 123 || (*expected == 123 && matches!(bias, AnchorBias::Right)) {
            *expected += 6;
        }
    }

    tree.remove_char_range(9_000..9_250);
    for (_, expected, _) in &mut anchors {
        if *expected > 9_000 {
            if *expected >= 9_250 {
                *expected -= 250;
            } else {
                *expected = 9_000;
            }
        }
    }

    for (anchor, expected, _) in anchors {
        assert_eq!(tree.anchor_position(anchor), Some(expected));
    }
    assert_eq!(tree.live_anchor_count(), 1_000);
    assert_balanced(&tree);
}

#[test]
fn anchor_survives_clone_of_tree_independently() {
    let mut tree = PieceTreeLite::from_string("abcdef".to_owned());
    let anchor = tree.create_anchor(3, AnchorBias::Left);
    let mut other = tree.clone();

    tree.insert(0, "ZZ");
    other.remove_char_range(0..2);

    assert_eq!(tree.anchor_position(anchor), Some(5));
    assert_eq!(other.anchor_position(anchor), Some(1));
}

#[test]
fn repeated_inserts_split_into_multiple_balanced_nodes() {
    let mut tree = PieceTreeLite::from_string("abc".repeat(128));
    let mut expected = "abc".repeat(128);

    for _ in 0..320 {
        tree.insert(1, "x");
        insert_string_at_char(&mut expected, 1, "x");
    }

    assert_eq!(tree.extract_range(0..tree.len_chars()), expected);
    assert!(tree.root.nodes.len() > 1);
    assert_balanced(&tree);
}

#[test]
fn repeated_removals_merge_nodes_back_down() {
    let mut tree = PieceTreeLite::from_string("abc".repeat(128));
    let mut expected = "abc".repeat(128);

    for _ in 0..320 {
        tree.insert(1, "x");
        insert_string_at_char(&mut expected, 1, "x");
    }
    let expanded_node_count = tree.root.nodes.len();

    tree.remove_char_range(1..321);
    remove_string_char_range(&mut expected, 1..321);

    assert_eq!(tree.extract_range(0..tree.len_chars()), expected);
    assert!(tree.root.nodes.len() < expanded_node_count);
    assert_balanced(&tree);
}

#[test]
fn pack_avoids_runt_nodes() {
    let mut tree = PieceTreeLite::from_string(String::new());
    let chunk = "x".repeat(1024);
    for i in 0..300 {
        tree.insert(i * 1024, &chunk);
    }
    for node in &tree.root.nodes {
        assert!(
            node.leaves.len() >= MIN_LEAVES_PER_INTERNAL || tree.root.nodes.len() == 1,
            "runt node with {} leaves (min {})",
            node.leaves.len(),
            MIN_LEAVES_PER_INTERNAL,
        );
    }
    assert_balanced(&tree);
}

#[test]
fn randomized_edit_sequences_match_string_model() {
    for seed in [
        0xC0DE_0001_u64,
        0xC0DE_0002_u64,
        0xC0DE_0003_u64,
        0xC0DE_0004_u64,
    ] {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut expected = random_text(&mut rng, 96);
        let mut tree = PieceTreeLite::from_string(expected.clone());
        assert_tree_matches_string_model(&tree, &expected);

        for _step in 0..300 {
            match rng.random_range(0..4) {
                0 => {
                    let at = rng.random_range(0..=expected.chars().count());
                    let inserted_len = rng.random_range(0..=12);
                    let inserted = random_text(&mut rng, inserted_len);
                    tree.insert(at, &inserted);
                    insert_string_at_char(&mut expected, at, &inserted);
                }
                1 => {
                    if expected.is_empty() {
                        continue;
                    }
                    let len = expected.chars().count();
                    let start = rng.random_range(0..len);
                    let end = rng.random_range(start + 1..=len);
                    tree.remove_char_range(start..end);
                    remove_string_char_range(&mut expected, start..end);
                }
                2 => {
                    let len = expected.chars().count();
                    let start = rng.random_range(0..=len);
                    let end = rng.random_range(start..=len);
                    let replacement_len = rng.random_range(0..=10);
                    let replacement = random_text(&mut rng, replacement_len);
                    tree.remove_char_range(start..end);
                    if !replacement.is_empty() {
                        tree.insert(start, &replacement);
                    }
                    replace_string_char_range(&mut expected, start..end, &replacement);
                }
                _ => {
                    let full = tree.extract_range(0..tree.len_chars());
                    let rebuilt = PieceTreeLite::from_string(full.clone());
                    assert_eq!(rebuilt.extract_range(0..rebuilt.len_chars()), full);
                    assert_eq!(rebuilt.metrics().bytes, tree.metrics().bytes);
                    assert_eq!(rebuilt.metrics().chars, tree.metrics().chars);
                    assert_eq!(rebuilt.metrics().newlines, tree.metrics().newlines);
                }
            }

            assert_tree_matches_string_model(&tree, &expected);
        }
    }
}

fn assert_balanced(tree: &PieceTreeLite) {
    let mut computed_bytes = 0usize;
    let mut computed_chars = 0usize;
    let mut computed_newlines = 0usize;
    let mut computed_pieces = 0usize;
    let mut computed_anchors = 0usize;

    if tree.root.nodes.len() > 1 {
        assert!(!tree.root.nodes.is_empty());
    }

    for node in &tree.root.nodes {
        assert!(!node.leaves.is_empty());
        assert!(node.leaves.len() <= MAX_LEAVES_PER_INTERNAL);

        for leaf in &node.leaves {
            if !leaf.pieces.is_empty() {
                assert!(leaf.pieces.len() <= MAX_LEAF_PIECES);
                assert!(leaf.metrics.bytes <= MAX_LEAF_BYTES);
            }

            assert_eq!(leaf.piece_start_chars.len(), leaf.pieces.len());
            assert_eq!(leaf.piece_start_newlines.len(), leaf.pieces.len());
            let mut prefix_chars = 0usize;
            let mut prefix_newlines = 0usize;
            for (index, piece) in leaf.pieces.iter().enumerate() {
                assert_eq!(leaf.piece_start_chars[index], prefix_chars);
                assert_eq!(leaf.piece_start_newlines[index], prefix_newlines);
                prefix_chars += piece.char_len;
                prefix_newlines += piece.newline_count;
            }

            computed_bytes += leaf.metrics.bytes;
            computed_chars += leaf.metrics.chars;
            computed_newlines += leaf.metrics.newlines;
            computed_pieces += leaf.metrics.pieces;
            computed_anchors += leaf.anchors.len();
        }
    }

    assert_eq!(tree.metrics().bytes, computed_bytes);
    assert_eq!(tree.metrics().chars, computed_chars);
    assert_eq!(tree.metrics().newlines, computed_newlines);
    assert_eq!(tree.metrics().pieces, computed_pieces);
    assert_eq!(tree.root.anchor_count, computed_anchors);
    assert_eq!(tree.live_anchor_count(), computed_anchors);
}

fn assert_tree_matches_string_model(tree: &PieceTreeLite, expected: &str) {
    assert_eq!(tree.extract_range(0..tree.len_chars()), expected);
    assert_eq!(tree.len_chars(), expected.chars().count());
    assert_eq!(tree.len_bytes(), expected.len());
    assert_eq!(tree.metrics().chars, expected.chars().count());
    assert_eq!(tree.metrics().bytes, expected.len());
    assert_eq!(tree.metrics().newlines, expected.matches('\n').count());

    for (offset, ch) in expected.chars().enumerate() {
        assert_eq!(tree.char_at(offset), Some(ch), "char mismatch at {offset}");
    }
    assert_eq!(tree.char_at(expected.chars().count()), None);

    let lines = split_lines_without_newlines(expected);
    for (line_index, expected_line) in lines.iter().enumerate() {
        let info = tree.line_info(line_index);
        assert_eq!(info.line_index, line_index);
        assert_eq!(
            tree.extract_range(info.start_char..info.start_char + info.char_len),
            *expected_line,
            "line {line_index} mismatch"
        );
    }

    assert_balanced(tree);
}

#[test]
fn batched_previews_match_individual_preview_generation() {
    let tree = PieceTreeLite::from_string("one\ntwo alpha\nthree alpha\nfour".to_owned());
    let ranges = vec![8..13, 20..25];

    let previews = tree.previews_for_matches(&ranges, ranges.len());
    let expected = ranges
        .iter()
        .map(|range| tree.preview_for_match(range))
        .collect::<Vec<_>>();

    assert_eq!(previews, expected);
}

#[test]
fn batched_previews_match_individual_preview_generation_after_fragmentation() {
    let mut tree = PieceTreeLite::from_string("one\ntwo alpha\nthree alpha\nfour".to_owned());
    tree.insert(0, "zero\n");
    let ranges = vec![13..18, 25..30];

    let previews = tree.previews_for_matches(&ranges, ranges.len());
    let expected = ranges
        .iter()
        .map(|range| tree.preview_for_match(range))
        .collect::<Vec<_>>();

    assert_eq!(previews, expected);
}

fn insert_string_at_char(text: &mut String, char_offset: usize, inserted: &str) {
    let byte_offset = char_to_byte_offset(text, char_offset);
    text.insert_str(byte_offset, inserted);
}

fn remove_string_char_range(text: &mut String, range: std::ops::Range<usize>) {
    let start = char_to_byte_offset(text, range.start);
    let end = char_to_byte_offset(text, range.end);
    text.replace_range(start..end, "");
}

fn replace_string_char_range(text: &mut String, range: std::ops::Range<usize>, replacement: &str) {
    let start = char_to_byte_offset(text, range.start);
    let end = char_to_byte_offset(text, range.end);
    text.replace_range(start..end, replacement);
}

fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
    if char_offset == 0 {
        return 0;
    }

    text.char_indices()
        .map(|(index, _)| index)
        .nth(char_offset)
        .unwrap_or(text.len())
}

fn random_text(rng: &mut StdRng, max_len: usize) -> String {
    const ALPHABET: &[char] = &[
        'a', 'b', 'c', 'x', 'y', 'z', '0', '1', '2', ' ', '\n', 'é', 'λ', 'β', '🙂', '界',
    ];
    let len = rng.random_range(0..=max_len);
    let mut text = String::new();
    for _ in 0..len {
        text.push(ALPHABET[rng.random_range(0..ALPHABET.len())]);
    }
    text
}

fn split_lines_without_newlines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }
    text.split('\n').collect()
}
