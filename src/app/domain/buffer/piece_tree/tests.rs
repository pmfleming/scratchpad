use super::{
    AnchorBias, AnchorOwner, AnchorOwnerKind, PIECE_PROVENANCE_ENTRY_LIMIT, PieceSource,
    PieceTreeLite,
};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use std::io::Write;

#[test]
fn anchor_left_bias_stays_before_insertion_at_same_offset() {
    let mut tree = PieceTreeLite::from_string("ab".to_owned());
    let anchor = tree.create_anchor(1, AnchorBias::Left);

    tree.insert_with_source(1, "X", PieceSource::Edit);

    assert_eq!(tree.extract_text(), "aXb");
    assert_eq!(tree.anchor_position(anchor), Some(1));
}

#[test]
fn anchor_right_bias_moves_after_insertion_at_same_offset() {
    let mut tree = PieceTreeLite::from_string("ab".to_owned());
    let anchor = tree.create_anchor(1, AnchorBias::Right);

    tree.insert_with_source(1, "X", PieceSource::Edit);

    assert_eq!(tree.anchor_position(anchor), Some(2));
}

#[test]
fn anchor_inside_removed_range_collapses_to_start() {
    let mut tree = PieceTreeLite::from_string("abcdef".to_owned());
    let anchor = tree.create_anchor(3, AnchorBias::Right);

    tree.remove_char_range(1..5);

    assert_eq!(tree.extract_text(), "af");
    assert_eq!(tree.anchor_position(anchor), Some(1));
}

#[test]
fn release_anchor_removes_registry_entry() {
    let mut tree = PieceTreeLite::from_string("abc".to_owned());
    let anchor = tree.create_anchor(1, AnchorBias::Left);

    tree.release_anchor(anchor);

    assert_eq!(tree.anchor_position(anchor), None);
    assert_eq!(tree.anchor_bias(anchor), None);
}

#[test]
fn owner_metadata_survives_edits() {
    let mut tree = PieceTreeLite::from_string("abcdef".to_owned());
    let owner = AnchorOwner::new(AnchorOwnerKind::Cursor, Some(42));
    let anchor = tree.create_anchor_with_owner(3, AnchorBias::Right, owner);

    tree.insert_with_source(0, ">>", PieceSource::Edit);
    tree.remove_char_range(5..6);

    assert_eq!(tree.anchor_owner(anchor), Some(owner));
}

#[test]
fn anchor_stripped_clone_shares_original_text_storage() {
    let mut tree = PieceTreeLite::from_string("needle\n".repeat(1024));
    let anchor = tree.create_anchor(3, AnchorBias::Right);

    let clone = tree.clone_without_anchors();

    assert!(tree.storage.shares_original_storage_with(&clone.storage));
    assert_eq!(tree.anchor_position(anchor), Some(3));
    assert_eq!(clone.anchor_position(anchor), None);
    assert_eq!(clone.extract_text(), tree.extract_text());
}

#[test]
fn file_backed_chunk_cache_is_bounded_without_invalidating_active_text() {
    const CHUNK_BYTES: usize = 256 * 1024;
    let mut file = tempfile::NamedTempFile::new().expect("create file-backed tree fixture");
    let chunk = vec![b'a'; CHUNK_BYTES];
    for _ in 0..40 {
        file.write_all(&chunk).expect("write fixture chunk");
    }
    file.flush().expect("flush fixture");

    let (tree, _, _) =
        PieceTreeLite::from_utf8_file(file.path(), 0, 0).expect("construct file-backed tree");
    let pinned = tree.borrow_range(0..1).expect("borrow first chunk");
    for chunk_index in 1..40 {
        let offset = chunk_index * CHUNK_BYTES;
        assert_eq!(
            tree.borrow_range(offset..offset + 1)
                .expect("borrow cache probe")
                .as_ref(),
            "a"
        );
    }

    assert_eq!(pinned.as_ref(), "a");
    assert_eq!(
        tree.loaded_file_chunk_count(),
        tree.file_chunk_cache_limit()
    );
    assert_eq!(tree.file_chunk_cache_limit() * CHUNK_BYTES, 8 * 1024 * 1024);
}

#[test]
fn byte_spans_preserve_offsets_beyond_four_gibibytes() {
    let start = u32::MAX as usize + 17;
    let span = super::storage::add_byte_span(start, 42);

    assert_eq!(span.start_byte, start as u64);
    assert_eq!(span.byte_len, 42);
}

#[test]
fn unicode_insert_delete_and_extract_use_char_offsets() {
    let mut tree = PieceTreeLite::from_string("a😀c".to_owned());

    tree.insert_with_source(2, "β", PieceSource::Edit);
    tree.remove_char_range(1..2);

    assert_eq!(tree.extract_text(), "aβc");
    assert_eq!(super::query::char_position(&tree, 2).column_index, 2);
    assert_eq!(tree.extract_range(1..3), "βc");
}

#[test]
fn bounded_extraction_reports_truncation_without_splitting_characters() {
    let tree = PieceTreeLite::from_string("a😀βc".to_owned());

    let (text, truncated) = tree.extract_range_bounded(0..4, 2);

    assert_eq!(text, "a😀");
    assert!(truncated);
}

#[test]
fn line_lookup_tracks_mixed_edits() {
    let mut tree = PieceTreeLite::from_string("one\ntwo\nfour".to_owned());

    tree.insert_with_source(8, "three\n", PieceSource::Edit);
    tree.remove_char_range(0..4);

    assert_eq!(tree.extract_text(), "two\nthree\nfour");
    assert_eq!(tree.line_info(1).start_char, 4);
    assert_eq!(tree.line_info(1).char_len, 5);
    assert_eq!(tree.line_index_at_offset(8), 1);
}

#[test]
fn line_lookup_handles_lines_spanning_many_leaves() {
    let first = "a".repeat(300_000);
    let second = "β".repeat(150_000);
    let text = format!("{first}\n{second}\ntail");
    let tree = PieceTreeLite::from_string(text);

    let first_line = tree.line_info(0);
    assert_eq!(first_line.start_char, 0);
    assert_eq!(first_line.char_len, 300_000);

    let second_line = tree.line_info(1);
    assert_eq!(second_line.start_char, 300_001);
    assert_eq!(second_line.char_len, 150_000);

    let tail_line = tree.line_info(2);
    assert_eq!(tail_line.start_char, 450_002);
    assert_eq!(tail_line.char_len, 4);
    assert_eq!(tree.line_index_at_offset(450_003), 2);
}

#[test]
fn batched_previews_match_single_preview_on_edited_tree() {
    let mut tree =
        PieceTreeLite::from_string("alpha target\nbeta target beta\nfinal target".to_owned());
    tree.insert_with_source(6, "inserted ", PieceSource::Edit);
    tree.insert_with_source(31, "wide ", PieceSource::Edit);
    tree.remove_char_range(0..1);

    assert!(tree.borrow_range(0..tree.len_chars()).is_none());
    let ranges = match_char_ranges(&tree.extract_text(), "target");
    let expected = ranges
        .iter()
        .map(|range| super::preview::preview_for_match(&tree, range))
        .collect::<Vec<_>>();

    assert_eq!(
        super::preview::previews_for_matches(&tree, &ranges, ranges.len()),
        expected
    );
}

#[test]
fn provenance_tracks_insert_source() {
    let mut tree = PieceTreeLite::from_string("ab".to_owned());

    tree.insert_with_source(1, "paste", PieceSource::Paste);
    let span = tree
        .spans_for_range(1..6)
        .next()
        .expect("inserted span")
        .byte_span;

    assert_eq!(tree.provenance_for_span(span).source, PieceSource::Paste);
}

#[test]
fn compact_add_buffer_preserves_visible_text_and_history_spans() {
    let mut tree = PieceTreeLite::from_string("ab".to_owned());
    tree.insert_with_source(1, "XX", PieceSource::Edit);
    let mut spans = vec![tree.append_history_text("history", PieceSource::SearchReplace)];

    tree.compact_add_buffer(&mut spans);

    assert_eq!(tree.extract_text(), "aXXb");
    assert_eq!(tree.text_for_span(spans[0]).as_ref(), "history");
    assert_eq!(
        tree.provenance_for_span(spans[0]).source,
        PieceSource::SearchReplace
    );
}

#[test]
fn compact_add_buffer_rewrites_provenance_for_relocated_history_spans() {
    let mut tree = PieceTreeLite::from_string("ab".to_owned());
    tree.insert_with_source(1, "XX", PieceSource::Paste);
    let mut spans = vec![tree.append_history_text("history", PieceSource::SearchReplace)];

    tree.remove_char_range(1..3);
    tree.compact_add_buffer(&mut spans);

    assert_eq!(tree.extract_text(), "ab");
    assert_eq!(tree.text_for_span(spans[0]).as_ref(), "history");
    assert_eq!(
        tree.provenance_for_span(spans[0]).source,
        PieceSource::SearchReplace
    );
    assert_eq!(tree.provenance_entry_count(), 1);
}

#[test]
fn provenance_store_caps_cold_entries() {
    let mut tree = PieceTreeLite::from_string(String::new());
    let mut spans = Vec::new();

    for _ in 0..PIECE_PROVENANCE_ENTRY_LIMIT + 4 {
        spans.push(tree.append_history_text("x", PieceSource::Edit));
    }

    assert_eq!(tree.provenance_entry_count(), PIECE_PROVENANCE_ENTRY_LIMIT);
    assert_eq!(tree.provenance_for_span(spans[0]).source, PieceSource::Load);
    assert_eq!(
        tree.provenance_for_span(*spans.last().expect("latest span"))
            .source,
        PieceSource::Edit
    );
}

#[test]
fn randomized_edits_match_string_model() {
    let mut rng = StdRng::seed_from_u64(0x5eed_2026);
    let mut tree = PieceTreeLite::from_string(String::new());
    let mut model = String::new();

    for _ in 0..200 {
        let model_len = model.chars().count();
        if model_len == 0 || rng.random_bool(0.65) {
            let index = rng.random_range(0..=model_len);
            let text = random_text(&mut rng);
            tree.insert_with_source(index, &text, PieceSource::Edit);
            insert_string_at_char(&mut model, index, &text);
        } else {
            let start = rng.random_range(0..model_len);
            let end = rng.random_range(start + 1..=model_len);
            tree.remove_char_range(start..end);
            remove_string_char_range(&mut model, start..end);
        }

        assert_eq!(tree.extract_text(), model);
        assert_eq!(tree.len_chars(), model.chars().count());
        assert_eq!(tree.len_bytes(), model.len());
    }
}

fn random_text(rng: &mut StdRng) -> String {
    const CHOICES: &[&str] = &["a", "β", "😀", "\n", "xy", " "];
    let count = rng.random_range(1..=4);
    (0..count)
        .map(|_| CHOICES[rng.random_range(0..CHOICES.len())])
        .collect()
}

fn insert_string_at_char(text: &mut String, char_offset: usize, inserted: &str) {
    let byte = byte_index_for_char(text, char_offset);
    text.insert_str(byte, inserted);
}

fn remove_string_char_range(text: &mut String, range: std::ops::Range<usize>) {
    let start = byte_index_for_char(text, range.start);
    let end = byte_index_for_char(text, range.end);
    text.replace_range(start..end, "");
}

fn byte_index_for_char(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .nth(char_offset)
        .map_or(text.len(), |(index, _)| index)
}

fn match_char_ranges(text: &str, needle: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut search_start = 0usize;
    while let Some(relative_start) = text[search_start..].find(needle) {
        let start_byte = search_start + relative_start;
        let start_char = text[..start_byte].chars().count();
        let char_len = needle.chars().count();
        ranges.push(start_char..start_char + char_len);
        search_start = start_byte + needle.len();
    }
    ranges
}
