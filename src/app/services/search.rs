use regex::RegexBuilder;
use std::ops::Range;

const INTERRUPT_CHECK_INTERVAL: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    PlainText,
    Regex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchError {
    InvalidRegex(String),
}

impl SearchError {
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidRegex(message) => message,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchOutcome {
    pub matches: Vec<Range<usize>>,
    pub error: Option<SearchError>,
}

impl SearchOutcome {
    fn with_matches(matches: Vec<Range<usize>>) -> Self {
        Self {
            matches,
            error: None,
        }
    }

    fn with_error(error: SearchError) -> Self {
        Self {
            matches: Vec::new(),
            error: Some(error),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub mode: SearchMode,
    pub match_case: bool,
    pub whole_word: bool,
}

pub fn find_matches(text: &str, query: &str, options: SearchOptions) -> Vec<Range<usize>> {
    search_text(text, query, options).matches
}

pub fn search_text(text: &str, query: &str, options: SearchOptions) -> SearchOutcome {
    if query.is_empty() {
        return SearchOutcome::default();
    }

    match options.mode {
        SearchMode::PlainText => plain_text_search(text, query, options),
        SearchMode::Regex => regex_search(text, query, options),
    }
}

pub fn find_matches_interruptible<F>(
    text: &str,
    query: &str,
    options: SearchOptions,
    should_continue: F,
) -> Option<Vec<Range<usize>>>
where
    F: FnMut() -> bool,
{
    search_text_interruptible(text, query, options, should_continue).map(|result| result.matches)
}

pub fn search_text_interruptible<F>(
    text: &str,
    query: &str,
    options: SearchOptions,
    should_continue: F,
) -> Option<SearchOutcome>
where
    F: FnMut() -> bool,
{
    if query.is_empty() {
        return Some(SearchOutcome::default());
    }

    match options.mode {
        SearchMode::PlainText => {
            plain_text_search_interruptible(text, query, options, should_continue)
        }
        SearchMode::Regex => regex_search_interruptible(text, query, options, should_continue),
    }
}

fn plain_text_search(text: &str, query: &str, options: SearchOptions) -> SearchOutcome {
    let matches = if options.match_case {
        find_matches_case_sensitive(text, query, options.whole_word)
    } else if text.is_ascii() && query.is_ascii() {
        let mut should_continue = || true;
        find_matches_ascii_case_insensitive_impl(
            text.as_bytes(),
            query.as_bytes(),
            options.whole_word,
            false,
            &mut should_continue,
        )
        .unwrap_or_default()
    } else {
        let mut should_continue = || true;
        find_matches_unicode_case_insensitive_impl(
            text,
            query,
            options.whole_word,
            false,
            &mut should_continue,
        )
        .unwrap_or_default()
    };

    SearchOutcome::with_matches(matches)
}

fn plain_text_search_interruptible<F>(
    text: &str,
    query: &str,
    options: SearchOptions,
    mut should_continue: F,
) -> Option<SearchOutcome>
where
    F: FnMut() -> bool,
{
    let matches = if options.match_case {
        find_matches_case_sensitive_interruptible(text, query, options.whole_word, should_continue)?
    } else if text.is_ascii() && query.is_ascii() {
        find_matches_ascii_case_insensitive_impl(
            text.as_bytes(),
            query.as_bytes(),
            options.whole_word,
            true,
            &mut should_continue,
        )?
    } else {
        find_matches_unicode_case_insensitive_impl(
            text,
            query,
            options.whole_word,
            true,
            &mut should_continue,
        )?
    };

    Some(SearchOutcome::with_matches(matches))
}

fn regex_search(text: &str, query: &str, options: SearchOptions) -> SearchOutcome {
    match compile_regex(query, options) {
        Ok(regex) => {
            SearchOutcome::with_matches(find_regex_matches(text, &regex, options.whole_word))
        }
        Err(error) => SearchOutcome::with_error(error),
    }
}

fn regex_search_interruptible<F>(
    text: &str,
    query: &str,
    options: SearchOptions,
    mut should_continue: F,
) -> Option<SearchOutcome>
where
    F: FnMut() -> bool,
{
    let regex = match compile_regex(query, options) {
        Ok(regex) => regex,
        Err(error) => return Some(SearchOutcome::with_error(error)),
    };

    let ascii = text.is_ascii();
    let byte_to_char = if ascii {
        Vec::new()
    } else {
        byte_to_char_map(text)
    };
    let whole_word_matcher = if ascii {
        WholeWordMatcher::disabled()
    } else {
        WholeWordMatcher::new(text, options.whole_word)
    };
    let mut matches = Vec::new();

    for (step, search_match) in regex.find_iter(text).enumerate() {
        if should_abort(step, true, &mut should_continue) {
            return None;
        }
        let (start, end) = if ascii {
            (search_match.start(), search_match.end())
        } else {
            (
                byte_to_char[search_match.start()],
                byte_to_char[search_match.end()],
            )
        };
        if ascii {
            if !options.whole_word || is_ascii_whole_word_match(text.as_bytes(), start, end) {
                matches.push(start..end);
            }
        } else if whole_word_matcher.allows(start, end) {
            matches.push(start..end);
        }
    }

    finalize_matches(matches, true, &mut should_continue).map(SearchOutcome::with_matches)
}

fn compile_regex(query: &str, options: SearchOptions) -> Result<regex::Regex, SearchError> {
    RegexBuilder::new(query)
        .case_insensitive(!options.match_case)
        .build()
        .map_err(|error| SearchError::InvalidRegex(error.to_string()))
}

fn find_regex_matches(text: &str, regex: &regex::Regex, whole_word: bool) -> Vec<Range<usize>> {
    if text.is_ascii() {
        return regex
            .find_iter(text)
            .filter_map(|search_match| {
                let start = search_match.start();
                let end = search_match.end();
                if whole_word && !is_ascii_whole_word_match(text.as_bytes(), start, end) {
                    return None;
                }
                Some(start..end)
            })
            .collect();
    }

    let byte_to_char = byte_to_char_map(text);
    let whole_word_matcher = WholeWordMatcher::new(text, whole_word);
    regex
        .find_iter(text)
        .filter_map(|search_match| {
            let start = byte_to_char[search_match.start()];
            let end = byte_to_char[search_match.end()];
            whole_word_matcher.allows(start, end).then_some(start..end)
        })
        .collect()
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

    if text.is_ascii() {
        let text_bytes = text.as_bytes();
        let query_bytes = query.as_bytes();
        let mut matches = Vec::new();
        for start in 0..=text_bytes.len() - query_bytes.len() {
            if should_abort(start, true, &mut should_continue) {
                return None;
            }
            let end = start + query_bytes.len();
            if &text_bytes[start..end] != query_bytes {
                continue;
            }
            if whole_word && !is_ascii_whole_word_match(text_bytes, start, end) {
                continue;
            }
            matches.push(start..end);
        }
        return finalize_matches(matches, true, &mut should_continue);
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
    if text.is_ascii() {
        return text
            .match_indices(query)
            .filter_map(|(start, candidate)| {
                let end = start + candidate.len();
                if whole_word && !is_ascii_whole_word_match(text.as_bytes(), start, end) {
                    return None;
                }
                Some(start..end)
            })
            .collect();
    }

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

    fn disabled() -> Self {
        Self { chars: None }
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
        SearchError, SearchMode, SearchOptions, find_matches, next_match_index,
        previous_match_index, search_text, search_text_interruptible,
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
                mode: SearchMode::PlainText,
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
                mode: SearchMode::PlainText,
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
    fn regex_search_supports_case_insensitive_matches() {
        let outcome = search_text(
            "Alpha beta alpha",
            "alpha|beta",
            SearchOptions {
                mode: SearchMode::Regex,
                match_case: false,
                whole_word: false,
            },
        );
        assert_eq!(outcome.matches, vec![0..5, 6..10, 11..16]);
        assert_eq!(outcome.error, None);
    }

    #[test]
    fn regex_search_reports_invalid_queries() {
        let outcome = search_text(
            "Alpha",
            "(",
            SearchOptions {
                mode: SearchMode::Regex,
                match_case: true,
                whole_word: false,
            },
        );
        assert!(outcome.matches.is_empty());
        assert!(matches!(outcome.error, Some(SearchError::InvalidRegex(_))));
    }

    #[test]
    fn regex_whole_word_uses_character_offsets() {
        let outcome = search_text(
            "cat concatenate cat",
            "cat",
            SearchOptions {
                mode: SearchMode::Regex,
                match_case: true,
                whole_word: true,
            },
        );
        assert_eq!(outcome.matches, vec![0..3, 16..19]);
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
        let matches = search_text_interruptible(
            "Alpha alpha ALPHA",
            "alpha",
            SearchOptions::default(),
            || true,
        )
        .expect("search should complete");

        assert_eq!(matches.matches, vec![0..5, 6..11, 12..17]);
    }

    #[test]
    fn interruptible_search_supports_case_sensitive_unicode_offsets() {
        let matches = search_text_interruptible(
            "naive cafe caf\u{00e9}",
            "caf\u{00e9}",
            SearchOptions {
                mode: SearchMode::PlainText,
                match_case: true,
                whole_word: false,
            },
            || true,
        )
        .expect("search should complete");

        assert_eq!(matches.matches, vec![11..15]);
    }

    #[test]
    fn interruptible_ascii_search_can_cancel_mid_scan() {
        let text = "a".repeat(1024);
        let mut checks = 0;
        let result = search_text_interruptible(&text, "b", SearchOptions::default(), || {
            checks += 1;
            checks < 2
        });

        assert_eq!(result, None);
    }
}
