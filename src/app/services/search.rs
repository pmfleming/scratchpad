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
    ReplacementMismatch(String),
}

impl SearchError {
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidRegex(message)
            | Self::UnsupportedRegex(message)
            | Self::ReplacementMismatch(message) => message,
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

#[derive(Clone, Debug)]
pub struct SearchProgram {
    query: String,
    options: SearchOptions,
    regex: Option<regex::Regex>,
    max_match_chars: usize,
}

impl SearchProgram {
    pub fn compile(query: &str, options: SearchOptions) -> Result<Self, SearchError> {
        let max_match_chars = match options.mode {
            SearchMode::PlainText => query.chars().count(),
            SearchMode::Regex => regex_max_match_chars(query).ok_or_else(|| {
                SearchError::UnsupportedRegex(
                    "Regex search requires a bounded maximum match length.".to_owned(),
                )
            })?,
        };
        let regex = match options.mode {
            SearchMode::PlainText => None,
            SearchMode::Regex => Some(compile_regex(query, options)?),
        };
        Ok(Self {
            query: query.to_owned(),
            options,
            regex,
            max_match_chars: max_match_chars.max(1),
        })
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn options(&self) -> SearchOptions {
        self.options
    }

    pub fn max_match_chars(&self) -> usize {
        self.max_match_chars
    }

    pub fn expand_replacement(
        &self,
        matched_text: &str,
        replacement: &str,
    ) -> Result<String, SearchError> {
        let Some(regex) = &self.regex else {
            return Ok(replacement.to_owned());
        };
        let Some(captures) = regex.captures(matched_text) else {
            return Err(SearchError::ReplacementMismatch(
                "Regex replacement could not be expanded for stale search results.".to_owned(),
            ));
        };
        let mut expanded = String::new();
        captures.expand(replacement, &mut expanded);
        Ok(expanded)
    }
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
        SearchMode::Regex => SearchProgram::compile(query, options).err(),
    }
}

pub fn search_text(text: &str, query: &str, options: SearchOptions) -> SearchOutcome {
    if query.is_empty() {
        return SearchOutcome::default();
    }

    match SearchProgram::compile(query, options) {
        Ok(program) => search_program(text, &program),
        Err(error) => SearchOutcome::with_error(error),
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

    match SearchProgram::compile(query, options) {
        Ok(program) => search_program_interruptible(text, &program, should_continue),
        Err(error) => Some(SearchOutcome::with_error(error)),
    }
}

pub fn regex_max_match_chars(query: &str) -> Option<usize> {
    parse(query).ok()?.properties().maximum_len()
}

pub fn search_program(text: &str, program: &SearchProgram) -> SearchOutcome {
    let mut should_continue = || true;
    search_program_interruptible(text, program, &mut should_continue).unwrap_or_default()
}

pub fn search_program_interruptible<F>(
    text: &str,
    program: &SearchProgram,
    mut should_continue: F,
) -> Option<SearchOutcome>
where
    F: FnMut() -> bool,
{
    match program.options.mode {
        SearchMode::PlainText => plain_text_matches(
            text,
            program.query(),
            program.options,
            true,
            &mut should_continue,
        )
        .map(SearchOutcome::with_matches),
        SearchMode::Regex => collect_regex_matches(
            text,
            program
                .regex
                .as_ref()
                .expect("regex program requires regex"),
            program.options.whole_word,
            true,
            &mut should_continue,
        )
        .map(SearchOutcome::with_matches),
    }
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
        SearchError, SearchMode, SearchOptions, SearchProgram, find_matches, search_program,
        search_text, search_text_interruptible,
    };

    fn plain_options() -> SearchOptions {
        SearchOptions {
            mode: SearchMode::PlainText,
            match_case: true,
            whole_word: false,
        }
    }

    fn regex_options() -> SearchOptions {
        SearchOptions {
            mode: SearchMode::Regex,
            match_case: true,
            whole_word: false,
        }
    }

    #[test]
    fn compiled_program_matches_plain_search() {
        let options = plain_options();
        let program = SearchProgram::compile("needle", options).unwrap();
        let text = "needle hay needle";

        assert_eq!(
            search_text(text, "needle", options).matches,
            search_program(text, &program).matches
        );
    }

    #[test]
    fn regex_program_rejects_unbounded_matches() {
        let error = SearchProgram::compile("a*", regex_options()).unwrap_err();

        assert!(matches!(error, SearchError::UnsupportedRegex(_)));
    }

    #[test]
    fn regex_program_matches_bounded_query() {
        let program = SearchProgram::compile(r"ab[0-9]{2}", regex_options()).unwrap();
        let matches = search_program("ab12 xx ab99", &program).matches;

        assert_eq!(matches, vec![0..4, 8..12]);
    }

    #[test]
    fn regex_program_reports_utf8_char_ranges() {
        let program = SearchProgram::compile(r"café[0-9]{2}|東京[0-9]", regex_options()).unwrap();
        let matches = search_program("α café42 東京9 café", &program).matches;

        assert_eq!(matches, vec![2..8, 9..12]);
    }

    #[test]
    fn regex_program_expands_capture_replacements() {
        let program =
            SearchProgram::compile(r"([a-z]{1,8})-([0-9]{1,4})", regex_options()).unwrap();

        assert_eq!(
            program.expand_replacement("item-42", "$2:$1").unwrap(),
            "42:item"
        );
    }

    #[test]
    fn regex_program_rejects_replacement_for_non_matching_text() {
        let program =
            SearchProgram::compile(r"([a-z]{1,8})-([0-9]{1,4})", regex_options()).unwrap();

        assert!(matches!(
            program.expand_replacement("stale", "$2:$1"),
            Err(SearchError::ReplacementMismatch(_))
        ));
    }

    #[test]
    fn whole_word_unicode_does_not_allocate_full_char_vector_contract() {
        let options = SearchOptions {
            mode: SearchMode::PlainText,
            match_case: true,
            whole_word: true,
        };

        assert_eq!(
            find_matches("α beta αbeta α", "α", options),
            vec![0..1, 13..14]
        );
    }

    #[test]
    fn case_sensitive_search_reports_char_ranges_in_utf8_text() {
        let options = SearchOptions {
            mode: SearchMode::PlainText,
            match_case: true,
            whole_word: false,
        };

        assert_eq!(
            find_matches("café needle 東京 needle", "needle", options),
            vec![5..11, 15..21]
        );
        assert_eq!(
            find_matches("a café café", "café", options),
            vec![2..6, 7..11]
        );
    }

    #[test]
    fn whole_word_case_sensitive_search_checks_unicode_boundaries() {
        let options = SearchOptions {
            mode: SearchMode::PlainText,
            match_case: true,
            whole_word: true,
        };

        assert_eq!(
            find_matches("α β βx xβ β", "β", options),
            vec![2..3, 10..11]
        );
    }

    #[test]
    fn ascii_case_insensitive_search_uses_same_ranges() {
        let options = SearchOptions {
            mode: SearchMode::PlainText,
            match_case: false,
            whole_word: false,
        };

        assert_eq!(
            find_matches("Needle needle NEEDLE hay", "needle", options),
            vec![0..6, 7..13, 14..20]
        );
    }

    #[test]
    fn interruptible_search_can_cancel_large_scan() {
        let text = "a".repeat(128 * 1024);
        let mut calls = 0usize;
        let result = search_text_interruptible(&text, "a", plain_options(), || {
            calls += 1;
            calls < 2
        });

        assert!(result.is_none());
    }
}
