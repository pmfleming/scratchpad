use super::{SearchOptions, finalize_matches};
use memchr::{memchr_iter, memchr2_iter, memmem};
use std::ops::Range;

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
    let mut matches = Vec::new();

    for search_match in regex.find_iter(text) {
        if interrupt_check.should_abort(should_continue) {
            return None;
        }
        let (start, end) = regex_match_range(ascii, text, &search_match);
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

fn regex_match_range(ascii: bool, text: &str, search_match: &regex::Match<'_>) -> (usize, usize) {
    if ascii {
        return (search_match.start(), search_match.end());
    }

    (
        byte_to_char_index(text, search_match.start()),
        byte_to_char_index(text, search_match.end()),
    )
}

fn whole_word_allows(
    ascii: bool,
    text_bytes: &[u8],
    whole_word_matcher: &WholeWordMatcher,
    whole_word: bool,
    start: usize,
    end: usize,
) -> bool {
    if ascii {
        ascii_whole_word_allows(text_bytes, whole_word, start, end)
    } else {
        whole_word_matcher.allows(start, end)
    }
}

fn find_ascii_case_sensitive_matches<F>(
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

fn find_ascii_case_insensitive_single_byte_matches<F>(
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

fn find_ascii_case_insensitive_multi_byte_matches<F>(
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

fn byte_to_char_index(text: &str, byte_index: usize) -> usize {
    text[..byte_index].chars().count()
}

fn char_to_byte_map(text: &str) -> Vec<usize> {
    let mut offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

fn whole_word_boundary_allows<T, F>(
    items: &[T],
    start: usize,
    end: usize,
    mut is_word_item: F,
) -> bool
where
    F: FnMut(&T) -> bool,
{
    let before_is_word = start > 0 && is_word_item(&items[start - 1]);
    let after_is_word = end < items.len() && is_word_item(&items[end]);
    !before_is_word && !after_is_word
}

fn is_whole_word_match(text: &str, start: usize, end: usize) -> bool {
    let before_is_word = start > 0 && text.chars().nth(start - 1).is_some_and(is_word_char);
    let after_is_word = text.chars().nth(end).is_some_and(is_word_char);
    !before_is_word && !after_is_word
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn matches_unicode_case_insensitive(candidate: &str, query: &str) -> bool {
    candidate
        .chars()
        .flat_map(char::to_lowercase)
        .eq(query.chars().flat_map(char::to_lowercase))
}

fn is_ascii_whole_word_match(text_bytes: &[u8], start: usize, end: usize) -> bool {
    whole_word_boundary_allows(text_bytes, start, end, |byte| is_ascii_word_char(*byte))
}

fn ascii_whole_word_allows(text_bytes: &[u8], whole_word: bool, start: usize, end: usize) -> bool {
    !whole_word || is_ascii_whole_word_match(text_bytes, start, end)
}

fn is_ascii_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
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

struct WholeWordMatcher<'a> {
    text: &'a str,
    enabled: bool,
}

impl<'a> WholeWordMatcher<'a> {
    fn new(text: &'a str, enabled: bool) -> Self {
        Self { text, enabled }
    }

    fn allows(&self, start: usize, end: usize) -> bool {
        !self.enabled || is_whole_word_match(self.text, start, end)
    }
}
