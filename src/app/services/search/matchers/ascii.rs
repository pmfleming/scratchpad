use super::InterruptCheck;
use super::word_boundary::ascii_whole_word_allows;
use crate::app::services::search::finalize_matches;
use memchr::{memchr_iter, memchr2_iter, memmem};
use std::ops::Range;

pub(super) fn find_ascii_case_sensitive_matches<F>(
    text_bytes: &[u8],
    query_bytes: &[u8],
    whole_word: bool,
    interruptible: bool,
    should_continue: &mut F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    let mut interrupt_check = InterruptCheck::new(interruptible);
    let mut matches = Vec::new();
    let finder = memmem::Finder::new(query_bytes);
    for start in finder.find_iter(text_bytes) {
        if interrupt_check.should_abort(should_continue) {
            return None;
        }
        let end = start + query_bytes.len();
        if ascii_whole_word_allows(text_bytes, whole_word, start, end) {
            matches.push(start..end);
        }
    }
    finalize_matches(matches, interruptible, should_continue)
}

pub(super) fn find_ascii_case_insensitive_single_byte_matches<F>(
    text_bytes: &[u8],
    query_byte: u8,
    whole_word: bool,
    interruptible: bool,
    should_continue: &mut F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    let mut interrupt_check = InterruptCheck::new(interruptible);
    let mut matches = Vec::new();
    let upper_query_byte = query_byte.to_ascii_uppercase();
    let iter = ascii_byte_candidates(text_bytes, query_byte, upper_query_byte);
    for start in iter {
        if interrupt_check.should_abort(should_continue) {
            return None;
        }
        let end = start + 1;
        if ascii_whole_word_allows(text_bytes, whole_word, start, end) {
            matches.push(start..end);
        }
    }
    finalize_matches(matches, interruptible, should_continue)
}

pub(super) fn find_ascii_case_insensitive_multi_byte_matches<F>(
    text_bytes: &[u8],
    query_lower: &[u8],
    whole_word: bool,
    interruptible: bool,
    should_continue: &mut F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    let first_query_byte = query_lower[0];
    let last_query_byte = query_lower[query_lower.len() - 1];
    let middle_query_bytes = &query_lower[1..query_lower.len().saturating_sub(1)];
    let mut interrupt_check = InterruptCheck::new(interruptible);
    let mut matches = Vec::new();
    let upper_first_query_byte = first_query_byte.to_ascii_uppercase();
    for start in ascii_byte_candidates(text_bytes, first_query_byte, upper_first_query_byte) {
        if interrupt_check.should_abort(should_continue) {
            return None;
        }
        let end = start + query_lower.len();
        if end > text_bytes.len() {
            continue;
        }
        if text_bytes[end - 1].to_ascii_lowercase() == last_query_byte
            && ascii_case_insensitive_bytes_match(
                &text_bytes[start + 1..end.saturating_sub(1)],
                middle_query_bytes,
            )
            && ascii_whole_word_allows(text_bytes, whole_word, start, end)
        {
            matches.push(start..end);
        }
    }
    finalize_matches(matches, interruptible, should_continue)
}

fn ascii_byte_candidates<'a>(
    text_bytes: &'a [u8],
    lower_byte: u8,
    upper_byte: u8,
) -> Box<dyn Iterator<Item = usize> + 'a> {
    if lower_byte == upper_byte {
        Box::new(memchr_iter(lower_byte, text_bytes))
    } else {
        Box::new(memchr2_iter(lower_byte, upper_byte, text_bytes))
    }
}

#[inline(always)]
fn ascii_case_insensitive_bytes_match(text_bytes: &[u8], query_lower_bytes: &[u8]) -> bool {
    debug_assert_eq!(text_bytes.len(), query_lower_bytes.len());

    let mut index = 0;
    while index < query_lower_bytes.len() {
        if text_bytes[index].to_ascii_lowercase() != query_lower_bytes[index] {
            return false;
        }
        index += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_sensitive_matches_report_byte_ranges() {
        let mut should_continue = || true;

        let matches = find_ascii_case_sensitive_matches(
            b"abc ABC abc",
            b"abc",
            false,
            false,
            &mut should_continue,
        )
        .unwrap();

        assert_eq!(matches, vec![0..3, 8..11]);
    }

    #[test]
    fn case_insensitive_single_byte_matches_upper_and_lower_forms() {
        let mut should_continue = || true;

        let matches = find_ascii_case_insensitive_single_byte_matches(
            b"aA_aa",
            b'a',
            false,
            false,
            &mut should_continue,
        )
        .unwrap();

        assert_eq!(matches, vec![0..1, 1..2, 3..4, 4..5]);
    }

    #[test]
    fn case_insensitive_multi_byte_whole_word_filters_embedded_hits() {
        let mut should_continue = || true;

        let matches = find_ascii_case_insensitive_multi_byte_matches(
            b"foo FOO food _foo foo_",
            b"foo",
            true,
            false,
            &mut should_continue,
        )
        .unwrap();

        assert_eq!(matches, vec![0..3, 4..7]);
    }

    #[test]
    fn interruptible_ascii_scan_can_abort() {
        let mut calls = 0usize;
        let mut should_continue = || {
            calls += 1;
            false
        };

        let matches = find_ascii_case_insensitive_single_byte_matches(
            b"aaaa",
            b'a',
            false,
            true,
            &mut should_continue,
        );

        assert!(matches.is_none());
        assert_eq!(calls, 1);
    }
}
