mod ascii;
mod word_boundary;

use super::{SearchOptions, finalize_matches};
use ascii::{
    find_ascii_case_insensitive_multi_byte_matches,
    find_ascii_case_insensitive_single_byte_matches, find_ascii_case_sensitive_matches,
};
use std::ops::Range;
use word_boundary::{WholeWordMatcher, whole_word_allows};

const INTERRUPT_CHECK_INTERVAL: u16 = 1024;

pub(super) fn plain_text_matches<F>(
    text: &str,
    query: &str,
    options: SearchOptions,
    interruptible: bool,
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
            interruptible,
            should_continue,
        );
    }
    if text.is_ascii() && query.is_ascii() {
        return find_matches_ascii_case_insensitive_impl(
            text.as_bytes(),
            query.as_bytes(),
            options.whole_word,
            interruptible,
            should_continue,
        );
    }
    find_matches_unicode_case_insensitive_impl(
        text,
        query,
        options.whole_word,
        interruptible,
        should_continue,
    )
}

pub(super) fn collect_regex_matches<F>(
    text: &str,
    regex: &regex::Regex,
    whole_word: bool,
    interruptible: bool,
    should_continue: &mut F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    let mut interrupt_check = InterruptCheck::new(interruptible);
    let ascii = text.is_ascii();
    let whole_word_matcher = WholeWordMatcher::new(text, whole_word);
    let mut char_cursor = RegexMatchCharCursor::default();
    let mut matches = Vec::new();

    for search_match in regex.find_iter(text) {
        if interrupt_check.should_abort(should_continue) {
            return None;
        }
        let (start, end) = if ascii {
            (search_match.start(), search_match.end())
        } else {
            char_cursor.match_range(text, &search_match)
        };
        if whole_word_allows(
            ascii,
            text.as_bytes(),
            &whole_word_matcher,
            whole_word,
            start,
            end,
        ) {
            matches.push(start..end);
        }
    }

    finalize_matches(matches, interruptible, should_continue)
}

#[derive(Default)]
struct RegexMatchCharCursor {
    byte_pos: usize,
    char_pos: usize,
}

impl RegexMatchCharCursor {
    fn match_range(&mut self, text: &str, search_match: &regex::Match<'_>) -> (usize, usize) {
        let start = self.advance_to(text, search_match.start());
        let end = self.advance_to(text, search_match.end());
        (start, end)
    }

    fn advance_to(&mut self, text: &str, byte_index: usize) -> usize {
        debug_assert!(self.byte_pos <= byte_index);
        self.char_pos += text[self.byte_pos..byte_index].chars().count();
        self.byte_pos = byte_index;
        self.char_pos
    }
}

struct InterruptCheck {
    enabled: bool,
    steps_until_check: u16,
}

impl InterruptCheck {
    fn new(interruptible: bool) -> Self {
        Self {
            enabled: interruptible,
            steps_until_check: 0,
        }
    }

    #[inline(always)]
    fn should_abort<F>(&mut self, should_continue: &mut F) -> bool
    where
        F: FnMut() -> bool,
    {
        if !self.enabled {
            return false;
        }
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
    interruptible: bool,
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
            interruptible,
            &mut should_continue,
        );
    }

    let mut interrupt_check = InterruptCheck::new(interruptible);
    let whole_word_matcher = WholeWordMatcher::new(text, whole_word);
    let mut matches = Vec::new();

    let query_char_len = query.chars().count();
    for (start, start_byte) in text
        .char_indices()
        .map(|(byte_index, _)| byte_index)
        .enumerate()
    {
        if interrupt_check.should_abort(&mut should_continue) {
            return None;
        }

        let end_byte = start_byte + query.len();
        if end_byte > text.len() {
            break;
        }
        if !text.is_char_boundary(end_byte) || &text[start_byte..end_byte] != query {
            continue;
        }

        let end = start + query_char_len;
        if !whole_word_matcher.allows(start, end) {
            continue;
        }
        matches.push(start..end);
    }

    finalize_matches(matches, interruptible, &mut should_continue)
}

fn find_matches_ascii_case_insensitive_impl<F>(
    text_bytes: &[u8],
    query_bytes: &[u8],
    whole_word: bool,
    interruptible: bool,
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
            interruptible,
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
        interruptible,
        &mut should_continue,
    )
}

fn find_matches_unicode_case_insensitive_impl<F>(
    text: &str,
    query: &str,
    whole_word: bool,
    interruptible: bool,
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
    let mut interrupt_check = InterruptCheck::new(interruptible);
    let mut matches = Vec::new();

    for start in 0..=char_count - query_char_len {
        if interrupt_check.should_abort(&mut should_continue) {
            return None;
        }

        let end = start + query_char_len;
        let candidate = &text[char_to_byte[start]..char_to_byte[end]];
        if !matches_unicode_case_insensitive(candidate, query)
            || !whole_word_matcher.allows(start, end)
        {
            continue;
        }
        matches.push(start..end);
    }

    finalize_matches(matches, interruptible, &mut should_continue)
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
