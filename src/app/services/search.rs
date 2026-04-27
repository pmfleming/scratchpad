mod matchers;

use matchers::{collect_regex_matches, plain_text_matches};
use regex::RegexBuilder;
use regex_syntax::parse;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    PlainText,
    Regex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchError {
    InvalidRegex(String),
    UnsupportedRegex(String),
}

impl SearchError {
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidRegex(message) | Self::UnsupportedRegex(message) => message,
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

pub fn validate_search_query(query: &str, options: SearchOptions) -> Option<SearchError> {
    if query.is_empty() {
        return None;
    }

    match options.mode {
        SearchMode::PlainText => None,
        SearchMode::Regex => compile_supported_regex(query, options).err(),
    }
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

pub fn regex_max_match_chars(query: &str) -> Option<usize> {
    parse(query).ok()?.properties().maximum_len()
}

fn compile_supported_regex(
    query: &str,
    options: SearchOptions,
) -> Result<regex::Regex, SearchError> {
    let regex = compile_regex(query, options)?;
    if regex_max_match_chars(query).is_none() {
        return Err(SearchError::UnsupportedRegex(
            "Regex search requires a bounded maximum match length.".to_owned(),
        ));
    }
    Ok(regex)
}

fn plain_text_search(text: &str, query: &str, options: SearchOptions) -> SearchOutcome {
    let mut should_continue = || true;
    SearchOutcome::with_matches(
        plain_text_matches(text, query, options, false, &mut should_continue).unwrap_or_default(),
    )
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
    plain_text_matches(text, query, options, true, &mut should_continue)
        .map(SearchOutcome::with_matches)
}

fn regex_search(text: &str, query: &str, options: SearchOptions) -> SearchOutcome {
    match compile_supported_regex(query, options) {
        Ok(regex) => {
            let mut should_continue = || true;
            SearchOutcome::with_matches(
                collect_regex_matches(
                    text,
                    &regex,
                    options.whole_word,
                    false,
                    &mut should_continue,
                )
                .unwrap_or_default(),
            )
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
    let regex = match compile_supported_regex(query, options) {
        Ok(regex) => regex,
        Err(error) => return Some(SearchOutcome::with_error(error)),
    };

    collect_regex_matches(text, &regex, options.whole_word, true, &mut should_continue)
        .map(SearchOutcome::with_matches)
}

fn compile_regex(query: &str, options: SearchOptions) -> Result<regex::Regex, SearchError> {
    RegexBuilder::new(query)
        .case_insensitive(!options.match_case)
        .build()
        .map_err(|error| SearchError::InvalidRegex(error.to_string()))
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
    fn regex_search_reports_unbounded_queries_as_unsupported() {
        let outcome = search_text(
            "Alpha beta alpha",
            "alpha+",
            SearchOptions {
                mode: SearchMode::Regex,
                match_case: true,
                whole_word: false,
            },
        );

        assert!(outcome.matches.is_empty());
        assert!(matches!(
            outcome.error,
            Some(SearchError::UnsupportedRegex(_))
        ));
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
    fn case_insensitive_ascii_search_handles_single_byte_queries() {
        let matches = find_matches("AaA", "a", SearchOptions::default());
        assert_eq!(matches, vec![0..1, 1..2, 2..3]);
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
