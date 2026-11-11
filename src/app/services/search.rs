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
    ResultLimitExceeded(String),
}

impl SearchError {
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidRegex(message)
            | Self::UnsupportedRegex(message)
            | Self::ResultLimitExceeded(message)
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
    result_budget: Option<std::sync::Arc<SearchResultBudget>>,
}

#[derive(Debug)]
struct SearchResultBudget {
    limit: usize,
    used: std::sync::atomic::AtomicUsize,
    exceeded: std::sync::atomic::AtomicBool,
}

impl SearchProgram {
    pub(crate) fn with_result_limit(mut self, limit: usize) -> Self {
        if limit == usize::MAX {
            return self;
        }
        self.result_budget = Some(std::sync::Arc::new(SearchResultBudget {
            limit,
            used: std::sync::atomic::AtomicUsize::new(0),
            exceeded: std::sync::atomic::AtomicBool::new(false),
        }));
        self
    }

    pub(crate) fn result_limit(&self) -> usize {
        self.result_budget
            .as_ref()
            .map_or(usize::MAX, |budget| budget.limit)
    }

    pub(crate) fn result_limit_exceeded(&self) -> bool {
        self.result_budget
            .as_ref()
            .is_some_and(|budget| budget.exceeded.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub(crate) fn accept_match_count(&self, count: usize) -> bool {
        self.accept_window_match_count(count, self.result_limit())
    }

    fn accept_window_match_count(&self, count: usize, limit: usize) -> bool {
        if count <= limit {
            return true;
        }
        self.result_budget
            .as_ref()
            .unwrap()
            .exceeded
            .store(true, std::sync::atomic::Ordering::Relaxed);
        false
    }

    // Reserve once per completed target, not once per match. Shared by all workers.
    pub(crate) fn reserve_result_matches(&self, count: usize) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        let Some(budget) = &self.result_budget else {
            return true;
        };
        if budget.exceeded.load(Relaxed) {
            return false;
        }
        let reserved = budget
            .used
            .fetch_update(Relaxed, Relaxed, |used| {
                used.checked_add(count)
                    .filter(|total| *total <= budget.limit)
            })
            .is_ok();
        if !reserved {
            budget.exceeded.store(true, Relaxed);
        }
        reserved
    }

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
            result_budget: None,
        })
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[must_use]
    pub fn options(&self) -> SearchOptions {
        self.options
    }

    #[must_use]
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

#[must_use]
pub fn find_matches(text: &str, query: &str, options: SearchOptions) -> Vec<Range<usize>> {
    search_text(text, query, options).matches
}

#[must_use]
pub fn validate_search_query(query: &str, options: SearchOptions) -> Option<SearchError> {
    if query.is_empty() {
        return None;
    }

    match options.mode {
        SearchMode::PlainText => None,
        SearchMode::Regex => SearchProgram::compile(query, options).err(),
    }
}

#[must_use]
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

#[must_use]
pub fn regex_max_match_chars(query: &str) -> Option<usize> {
    parse(query).ok()?.properties().maximum_len()
}

#[must_use]
pub fn search_program(text: &str, program: &SearchProgram) -> SearchOutcome {
    let mut should_continue = || true;
    search_program_interruptible(text, program, &mut should_continue).unwrap_or_default()
}

pub fn search_program_interruptible<F>(
    text: &str,
    program: &SearchProgram,
    should_continue: F,
) -> Option<SearchOutcome>
where
    F: FnMut() -> bool,
{
    search_program_window_interruptible(text, program, 0, should_continue)
}

/// Allow temporary overlap hits; the fragment collector budgets only core hits.
pub(crate) fn search_program_window_interruptible<F>(
    text: &str,
    program: &SearchProgram,
    overlap_chars: usize,
    mut should_continue: F,
) -> Option<SearchOutcome>
where
    F: FnMut() -> bool,
{
    let limit = program.result_limit().saturating_add(overlap_chars);
    let mut should_continue = || !program.result_limit_exceeded() && should_continue();
    let outcome = match program.options.mode {
        SearchMode::PlainText => plain_text_matches(
            text,
            program.query(),
            program.options,
            true,
            limit,
            &mut should_continue,
        )
        .map(SearchOutcome::with_matches),
        SearchMode::Regex => program.regex.as_ref().and_then(|regex| {
            collect_regex_matches(
                text,
                regex,
                program.options.whole_word,
                true,
                limit,
                &mut should_continue,
            )
            .map(SearchOutcome::with_matches)
        }),
    };
    outcome.filter(|outcome| program.accept_window_match_count(outcome.matches.len(), limit))
}

fn compile_regex(query: &str, options: SearchOptions) -> Result<regex::Regex, SearchError> {
    RegexBuilder::new(query)
        .case_insensitive(!options.match_case)
        .build()
        .map_err(|error| SearchError::InvalidRegex(error.to_string()))
}

#[must_use]
pub fn next_match_index(total_matches: usize, current: Option<usize>) -> Option<usize> {
    (total_matches > 0).then(|| current.map_or(0, |index| (index + 1) % total_matches))
}

#[must_use]
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

#[cfg(test)]
mod result_budget_tests {
    use super::*;

    #[test]
    fn bounded_collection_covers_ascii_unicode_whole_word_and_regex() {
        for (text, query, mode, match_case, whole_word) in [
            ("aaaa", "a", SearchMode::PlainText, true, false),
            ("aAaA", "a", SearchMode::PlainText, false, false),
            ("ABabABab", "ab", SearchMode::PlainText, false, false),
            ("éééé", "é", SearchMode::PlainText, true, false),
            ("éÉéÉ", "é", SearchMode::PlainText, false, false),
            ("a a a a", "a", SearchMode::PlainText, false, true),
            ("aaaa", "a?", SearchMode::Regex, true, false),
        ] {
            let program = SearchProgram::compile(
                query,
                SearchOptions {
                    mode,
                    match_case,
                    whole_word,
                },
            )
            .unwrap()
            .with_result_limit(3);
            assert!(
                search_program_interruptible(text, &program, || true).is_none(),
                "{query} in {text}"
            );
            assert!(program.result_limit_exceeded());
        }
    }

    #[test]
    fn complete_results_at_the_limit_and_shared_target_budget_are_exact() {
        let program = SearchProgram::compile("a", SearchOptions::default())
            .unwrap()
            .with_result_limit(3);
        assert_eq!(
            search_program_interruptible("aaa", &program, || true)
                .unwrap()
                .matches
                .len(),
            3
        );
        assert!(!program.result_limit_exceeded());
        assert!(program.reserve_result_matches(2));
        let clone = program.clone();
        assert!(clone.reserve_result_matches(1));
        assert!(!clone.reserve_result_matches(1));
        assert!(program.result_limit_exceeded());
    }
}
