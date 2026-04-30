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
