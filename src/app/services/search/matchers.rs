mod ascii;
mod word_boundary;

use super::SearchOptions;
use ascii::{
    find_ascii_case_insensitive_multi_byte_matches,
    find_ascii_case_insensitive_single_byte_matches, find_ascii_case_sensitive_matches,
};
use memchr::memmem;
use std::ops::Range;
use word_boundary::{WholeWordMatcher, whole_word_allows};

const INTERRUPT_CHECK_INTERVAL: u16 = 1024;

pub(super) fn plain_text_matches<F>(
    text: &str,
    query: &str,
    options: SearchOptions,
    limit: usize,
    should_continue: &mut F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    if options.match_case {
        return find_matches_case_sensitive_impl(
            text,
            query,
            options.whole_word,
            limit,
            should_continue,
        );
    }
    if text.is_ascii() && query.is_ascii() {
        return find_matches_ascii_case_insensitive_impl(
            text.as_bytes(),
            query.as_bytes(),
            options.whole_word,
            limit,
            should_continue,
        );
    }
    find_matches_unicode_case_insensitive_impl(
        text,
        query,
        options.whole_word,
        limit,
        should_continue,
    )
}

pub(super) fn collect_regex_matches<F>(
    text: &str,
    regex: &regex::Regex,
    whole_word: bool,
    limit: usize,
    should_continue: &mut F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    let ascii = text.is_ascii();
    let whole_word_matcher = WholeWordMatcher::new(text, whole_word);
    let mut char_cursor = ByteToCharCursor::default();
    collect_matches(
        regex.find_iter(text),
        |matched| {
            let (start, end) = if ascii {
                (matched.start(), matched.end())
            } else {
                char_cursor.byte_range(text, matched.start(), matched.end())
            };
            let allowed = if ascii {
                whole_word_allows(
                    true,
                    text.as_bytes(),
                    &whole_word_matcher,
                    whole_word,
                    start,
                    end,
                )
            } else {
                whole_word_matcher.allows_byte_range(matched.start(), matched.end())
            };
            allowed.then_some(start..end)
        },
        limit,
        should_continue,
    )
}

// All matcher paths share cancellation cadence and the limit+1 overflow sentinel.
fn collect_matches<T>(
    candidates: impl IntoIterator<Item = T>,
    mut match_range: impl FnMut(T) -> Option<Range<usize>>,
    limit: usize,
    should_continue: &mut impl FnMut() -> bool,
) -> Option<Vec<Range<usize>>> {
    let mut interrupt = InterruptCheck::default();
    let mut matches = Vec::new();
    for candidate in candidates {
        if interrupt.should_abort(should_continue) {
            return None;
        }
        if let Some(range) = match_range(candidate) {
            matches.push(range);
            if matches.len() > limit {
                return Some(matches);
            }
        }
    }
    should_continue().then_some(matches)
}

#[derive(Default)]
struct ByteToCharCursor {
    byte_pos: usize,
    char_pos: usize,
}

impl ByteToCharCursor {
    fn byte_range(&mut self, text: &str, start_byte: usize, end_byte: usize) -> (usize, usize) {
        let start = self.advance_to(text, start_byte);
        let end = self.advance_to(text, end_byte);
        (start, end)
    }

    fn advance_to(&mut self, text: &str, byte_index: usize) -> usize {
        debug_assert!(self.byte_pos <= byte_index);
        self.char_pos += text[self.byte_pos..byte_index].chars().count();
        self.byte_pos = byte_index;
        self.char_pos
    }
}

#[derive(Default)]
struct InterruptCheck {
    steps_until_check: u16,
}

impl InterruptCheck {
    #[inline(always)]
    fn should_abort<F>(&mut self, should_continue: &mut F) -> bool
    where
        F: FnMut() -> bool,
    {
        if self.steps_until_check == 0 {
            self.steps_until_check = INTERRUPT_CHECK_INTERVAL - 1;
            return !should_continue();
        }

        self.steps_until_check -= 1;
        false
    }
}

fn find_matches_case_sensitive_impl<F>(
    text: &str,
    query: &str,
    whole_word: bool,
    limit: usize,
    mut should_continue: F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    if query.len() > text.len() {
        return Some(Vec::new());
    }

    if text.is_ascii() {
        return find_ascii_case_sensitive_matches(
            text.as_bytes(),
            query.as_bytes(),
            whole_word,
            limit,
            &mut should_continue,
        );
    }

    let whole_word_matcher = WholeWordMatcher::new(text, whole_word);
    let finder = memmem::Finder::new(query.as_bytes());
    let mut char_cursor = ByteToCharCursor::default();
    collect_matches(
        finder.find_iter(text.as_bytes()),
        |start_byte| {
            let end_byte = start_byte + query.len();
            if !text.is_char_boundary(start_byte)
                || !text.is_char_boundary(end_byte)
                || !whole_word_matcher.allows_byte_range(start_byte, end_byte)
            {
                return None;
            }
            let (start, end) = char_cursor.byte_range(text, start_byte, end_byte);
            Some(start..end)
        },
        limit,
        &mut should_continue,
    )
}

fn find_matches_ascii_case_insensitive_impl<F>(
    text_bytes: &[u8],
    query_bytes: &[u8],
    whole_word: bool,
    limit: usize,
    mut should_continue: F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    if query_bytes.len() > text_bytes.len() {
        return Some(Vec::new());
    }

    if query_bytes.len() == 1 {
        return find_ascii_case_insensitive_single_byte_matches(
            text_bytes,
            query_bytes[0].to_ascii_lowercase(),
            whole_word,
            limit,
            &mut should_continue,
        );
    }

    let query_lower = query_bytes
        .iter()
        .map(u8::to_ascii_lowercase)
        .collect::<Vec<_>>();
    find_ascii_case_insensitive_multi_byte_matches(
        text_bytes,
        &query_lower,
        whole_word,
        limit,
        &mut should_continue,
    )
}

fn find_matches_unicode_case_insensitive_impl<F>(
    text: &str,
    query: &str,
    whole_word: bool,
    limit: usize,
    mut should_continue: F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    let query_char_len = query.chars().count();
    let char_to_byte = char_to_byte_map(text);
    let char_count = char_to_byte.len().saturating_sub(1);
    if query_char_len > char_count {
        return Some(Vec::new());
    }

    let whole_word_matcher = WholeWordMatcher::new(text, whole_word);
    collect_matches(
        0..=char_count - query_char_len,
        |start| {
            let end = start + query_char_len;
            let start_byte = char_to_byte[start];
            let end_byte = char_to_byte[end];
            (matches_unicode_case_insensitive(&text[start_byte..end_byte], query)
                && whole_word_matcher.allows_byte_range(start_byte, end_byte))
            .then_some(start..end)
        },
        limit,
        &mut should_continue,
    )
}

fn char_to_byte_map(text: &str) -> Vec<usize> {
    let mut offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

fn matches_unicode_case_insensitive(candidate: &str, query: &str) -> bool {
    candidate
        .chars()
        .flat_map(char::to_lowercase)
        .eq(query.chars().flat_map(char::to_lowercase))
}
