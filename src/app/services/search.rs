use std::ops::Range;

const INTERRUPT_CHECK_INTERVAL: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub match_case: bool,
    pub whole_word: bool,
}

pub fn find_matches(text: &str, query: &str, options: SearchOptions) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }

    if options.match_case {
        return find_matches_case_sensitive(text, query, options.whole_word);
    }
    if text.is_ascii() && query.is_ascii() {
        let mut should_continue = || true;
        return find_matches_ascii_case_insensitive_impl(
            text.as_bytes(),
            query.as_bytes(),
            options.whole_word,
            false,
            &mut should_continue,
        )
        .unwrap_or_default();
    }

    let mut should_continue = || true;
    find_matches_unicode_case_insensitive_impl(
        text,
        query,
        options.whole_word,
        false,
        &mut should_continue,
    )
    .unwrap_or_default()
}

pub fn find_matches_interruptible<F>(
    text: &str,
    query: &str,
    options: SearchOptions,
    mut should_continue: F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    if query.is_empty() {
        return Some(Vec::new());
    }

    if options.match_case {
        return find_matches_case_sensitive_interruptible(
            text,
            query,
            options.whole_word,
            should_continue,
        );
    }
    if text.is_ascii() && query.is_ascii() {
        return find_matches_ascii_case_insensitive_impl(
            text.as_bytes(),
            query.as_bytes(),
            options.whole_word,
            true,
            &mut should_continue,
        );
    }

    find_matches_unicode_case_insensitive_impl(
        text,
        query,
        options.whole_word,
        true,
        &mut should_continue,
    )
}

fn find_matches_case_sensitive_interruptible<F>(
    text: &str,
    query: &str,
    whole_word: bool,
    mut should_continue: F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    if query.len() > text.len() {
        return Some(Vec::new());
    }

    let byte_to_char = byte_to_char_map(text);
    let whole_word_matcher = WholeWordMatcher::new(text, whole_word);
    let mut matches = Vec::new();

    for (step, start_byte) in text
        .char_indices()
        .map(|(byte_index, _)| byte_index)
        .enumerate()
    {
        if should_abort(step, true, &mut should_continue) {
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

    finalize_matches(matches, true, &mut should_continue)
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

    let mut matches = Vec::new();
    for start in 0..=text_bytes.len() - query_bytes.len() {
        if should_abort(start, interruptible, &mut should_continue) {
            return None;
        }

        let end = start + query_bytes.len();
        if !text_bytes[start..end].eq_ignore_ascii_case(query_bytes) {
            continue;
        }
        if whole_word && !is_ascii_whole_word_match(text_bytes, start, end) {
            continue;
        }
        matches.push(start..end);
    }

    finalize_matches(matches, interruptible, &mut should_continue)
}

pub fn next_match_index(total_matches: usize, current: Option<usize>) -> Option<usize> {
    (total_matches > 0).then(|| current.map_or(0, |index| (index + 1) % total_matches))
}

pub fn previous_match_index(total_matches: usize, current: Option<usize>) -> Option<usize> {
    (total_matches > 0).then(|| {
        current.map_or(total_matches - 1, |index| {
            index.checked_sub(1).unwrap_or(total_matches - 1)
        })
    })
}

fn find_matches_case_sensitive(text: &str, query: &str, whole_word: bool) -> Vec<Range<usize>> {
    let byte_to_char = byte_to_char_map(text);
    let whole_word_matcher = WholeWordMatcher::new(text, whole_word);

    text.match_indices(query)
        .filter_map(|(start_byte, candidate)| {
            let end_byte = start_byte + candidate.len();
            let start = byte_to_char[start_byte];
            let end = byte_to_char[end_byte];
            whole_word_matcher.allows(start, end).then_some(start..end)
        })
        .collect()
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
    let mut matches = Vec::new();

    for start in 0..=char_count - query_char_len {
        if should_abort(start, interruptible, &mut should_continue) {
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

fn is_ascii_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn should_abort<F>(step: usize, interruptible: bool, should_continue: &mut F) -> bool
where
    F: FnMut() -> bool,
{
    interruptible && step.is_multiple_of(INTERRUPT_CHECK_INTERVAL) && !should_continue()
}

fn finalize_matches<F>(
    matches: Vec<Range<usize>>,
    interruptible: bool,
    should_continue: &mut F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    if interruptible && !should_continue() {
        None
    } else {
        Some(matches)
    }
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

    fn allows(&self, start: usize, end: usize) -> bool {
        self.chars
            .as_deref()
            .is_none_or(|chars| is_whole_word_match(chars, start, end))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SearchOptions, find_matches, find_matches_interruptible, next_match_index,
        previous_match_index,
    };

    #[test]
    fn find_matches_returns_character_ranges() {
        let matches = find_matches("naive cafe", "cafe", SearchOptions::default());
        assert_eq!(matches, vec![6..10]);
    }

    #[test]
    fn find_matches_supports_case_insensitive_search() {
        let matches = find_matches(
            "Alpha alpha ALPHA",
            "alpha",
            SearchOptions {
                match_case: false,
                whole_word: false,
            },
        );
        assert_eq!(matches, vec![0..5, 6..11, 12..17]);
    }

    #[test]
    fn whole_word_matching_rejects_embedded_hits() {
        let matches = find_matches(
            "cat concatenate cat",
            "cat",
            SearchOptions {
                match_case: true,
                whole_word: true,
            },
        );
        assert_eq!(matches, vec![0..3, 16..19]);
    }

    #[test]
    fn unicode_search_uses_character_offsets() {
        let matches = find_matches(
            "cafe cafe caf\u{00e9}",
            "caf\u{00e9}",
            SearchOptions::default(),
        );
        assert_eq!(matches, vec![10..14]);
    }

    #[test]
    fn next_and_previous_match_indices_wrap() {
        assert_eq!(next_match_index(3, None), Some(0));
        assert_eq!(next_match_index(3, Some(2)), Some(0));
        assert_eq!(previous_match_index(3, None), Some(2));
        assert_eq!(previous_match_index(3, Some(0)), Some(2));
    }

    #[test]
    fn interruptible_search_supports_ascii_case_insensitive_matches() {
        let matches = find_matches_interruptible(
            "Alpha alpha ALPHA",
            "alpha",
            SearchOptions::default(),
            || true,
        )
        .expect("search should complete");

        assert_eq!(matches, vec![0..5, 6..11, 12..17]);
    }

    #[test]
    fn interruptible_search_supports_case_sensitive_unicode_offsets() {
        let matches = find_matches_interruptible(
            "naive cafe caf\u{00e9}",
            "caf\u{00e9}",
            SearchOptions {
                match_case: true,
                whole_word: false,
            },
            || true,
        )
        .expect("search should complete");

        assert_eq!(matches, vec![11..15]);
    }

    #[test]
    fn interruptible_ascii_search_can_cancel_mid_scan() {
        let text = "a".repeat(1024);
        let mut checks = 0;
        let result = find_matches_interruptible(&text, "b", SearchOptions::default(), || {
            checks += 1;
            checks < 2
        });

        assert_eq!(result, None);
    }
}
