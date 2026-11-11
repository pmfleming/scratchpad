use super::collect_matches;
use super::word_boundary::ascii_whole_word_allows;
use memchr::{memchr2_iter, memmem};
use std::ops::Range;

pub(super) fn find_ascii_case_sensitive_matches(
    text: &[u8],
    query: &[u8],
    whole_word: bool,
    limit: usize,
    should_continue: &mut impl FnMut() -> bool,
) -> Option<Vec<Range<usize>>> {
    let finder = memmem::Finder::new(query);
    collect_matches(
        finder.find_iter(text),
        |start| {
            let end = start + query.len();
            ascii_whole_word_allows(text, whole_word, start, end).then_some(start..end)
        },
        limit,
        should_continue,
    )
}

pub(super) fn find_ascii_case_insensitive_single_byte_matches(
    text: &[u8],
    query: u8,
    whole_word: bool,
    limit: usize,
    should_continue: &mut impl FnMut() -> bool,
) -> Option<Vec<Range<usize>>> {
    collect_matches(
        memchr2_iter(query, query.to_ascii_uppercase(), text),
        |start| {
            let end = start + 1;
            ascii_whole_word_allows(text, whole_word, start, end).then_some(start..end)
        },
        limit,
        should_continue,
    )
}

pub(super) fn find_ascii_case_insensitive_multi_byte_matches(
    text: &[u8],
    query: &[u8],
    whole_word: bool,
    limit: usize,
    should_continue: &mut impl FnMut() -> bool,
) -> Option<Vec<Range<usize>>> {
    let last_query_byte = query[query.len() - 1];
    let middle_query = &query[1..query.len() - 1];
    collect_matches(
        memchr2_iter(query[0], query[0].to_ascii_uppercase(), text),
        |start| {
            let end = start + query.len();
            let candidate = text.get(start..end)?;
            (candidate[query.len() - 1].to_ascii_lowercase() == last_query_byte
                && candidate[1..query.len() - 1]
                    .iter()
                    .zip(middle_query)
                    .all(|(text, query)| text.to_ascii_lowercase() == *query)
                && ascii_whole_word_allows(text, whole_word, start, end))
            .then_some(start..end)
        },
        limit,
        should_continue,
    )
}
