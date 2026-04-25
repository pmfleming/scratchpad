use super::{SearchOptions, finalize_matches};
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
    let byte_to_char = (!ascii).then(|| byte_to_char_map(text));
    let whole_word_matcher = ascii
        .then(WholeWordMatcher::disabled)
        .unwrap_or_else(|| WholeWordMatcher::new(text, whole_word));
    let mut matches = Vec::new();

    for (step, search_match) in regex.find_iter(text).enumerate() {
        let _ = step;
        if interrupt_check.should_abort(should_continue) {
            return None;
        }
        let (start, end) = regex_match_range(ascii, byte_to_char.as_deref(), &search_match);
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
    let byte_to_char = byte_to_char_map(text);
    let whole_word_matcher = WholeWordMatcher::new(text, whole_word);
    let mut matches = Vec::new();

    for (step, start_byte) in text
        .char_indices()
        .map(|(byte_index, _)| byte_index)
        .enumerate()
    {
        let _ = step;
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

        let start = byte_to_char[start_byte];
        let end = byte_to_char[end_byte];
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

fn regex_match_range(
    ascii: bool,
    byte_to_char: Option<&[usize]>,
    search_match: &regex::Match<'_>,
) -> (usize, usize) {
    if ascii {
        return (search_match.start(), search_match.end());
    }

    let byte_to_char = byte_to_char.expect("unicode matches require byte-to-char map");
    (
        byte_to_char[search_match.start()],
        byte_to_char[search_match.end()],
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
    for start in 0..=text_bytes.len() - query_bytes.len() {
        if interrupt_check.should_abort(should_continue) {
            return None;
        }
        let end = start + query_bytes.len();
        if &text_bytes[start..end] == query_bytes
            && ascii_whole_word_allows(text_bytes, whole_word, start, end)
        {
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
    for (start, byte) in text_bytes.iter().copied().enumerate() {
        if interrupt_check.should_abort(should_continue) {
            return None;
        }
        let end = start + 1;
        if byte.to_ascii_lowercase() == query_byte
            && ascii_whole_word_allows(text_bytes, whole_word, start, end)
        {
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

    for start in 0..=text_bytes.len() - query_lower.len() {
        if interrupt_check.should_abort(should_continue) {
            return None;
        }

        let end = start + query_lower.len();
        if text_bytes[start].to_ascii_lowercase() != first_query_byte
            || text_bytes[end - 1].to_ascii_lowercase() != last_query_byte
            || !ascii_case_insensitive_bytes_match(
                &text_bytes[start + 1..end.saturating_sub(1)],
                middle_query_bytes,
            )
            || !ascii_whole_word_allows(text_bytes, whole_word, start, end)
        {
            continue;
        }

        matches.push(start..end);
    }

    finalize_matches(matches, interruptible, should_continue)
}

fn byte_to_char_map(text: &str) -> Vec<usize> {
    let mut map = vec![0; text.len() + 1];
    let mut char_index = 0;
    for (byte_index, ch) in text.char_indices() {
        map[byte_index] = char_index;
        char_index += 1;
        map[byte_index + ch.len_utf8()] = char_index;
    }
    map
}

fn char_to_byte_map(text: &str) -> Vec<usize> {
    let mut offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

fn is_whole_word_match(text_chars: &[char], start: usize, end: usize) -> bool {
    let before_is_word = start > 0 && is_word_char(text_chars[start - 1]);
    let after_is_word = end < text_chars.len() && is_word_char(text_chars[end]);
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
    let before_is_word = start > 0 && is_ascii_word_char(text_bytes[start - 1]);
    let after_is_word = end < text_bytes.len() && is_ascii_word_char(text_bytes[end]);
    !before_is_word && !after_is_word
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

struct WholeWordMatcher {
    chars: Option<Vec<char>>,
}

impl WholeWordMatcher {
    fn new(text: &str, enabled: bool) -> Self {
        Self {
            chars: enabled.then(|| text.chars().collect()),
        }
    }

    fn disabled() -> Self {
        Self { chars: None }
    }

    fn allows(&self, start: usize, end: usize) -> bool {
        self.chars
            .as_deref()
            .is_none_or(|chars| is_whole_word_match(chars, start, end))
    }
}
