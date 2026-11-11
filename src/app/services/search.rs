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
            limit,
            &mut should_continue,
        )
        .map(SearchOutcome::with_matches),
        SearchMode::Regex => program.regex.as_ref().and_then(|regex| {
            collect_regex_matches(
                text,
                regex,
                program.options.whole_word,
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

#[cfg(test)]
mod tests {
    use super::{
        SearchError, SearchMode, SearchOptions, SearchProgram, find_matches,
        find_matches_interruptible, search_program, search_program_interruptible, search_text,
        search_text_interruptible, validate_search_query,
    };

    #[test]
    fn matcher_matrix_preserves_ranges_limits_and_compiled_search() {
        let folded = SearchOptions::default();
        let exact = SearchOptions {
            match_case: true,
            ..folded
        };
        let folded_word = SearchOptions {
            whole_word: true,
            ..folded
        };
        let exact_word = SearchOptions {
            whole_word: true,
            ..exact
        };
        let regex = SearchOptions {
            mode: SearchMode::Regex,
            ..exact
        };
        let regex_word = SearchOptions {
            whole_word: true,
            ..regex
        };
        for (text, query, options, expected) in [
            ("abc ABC abc", "abc", exact, vec![0..3, 8..11]),
            ("aA_aa", "a", folded, vec![0..1, 1..2, 3..4, 4..5]),
            (
                "foo FOO food _foo foo_",
                "foo",
                folded_word,
                vec![0..3, 4..7],
            ),
            (
                "Needle needle NEEDLE hay",
                "needle",
                folded,
                vec![0..6, 7..13, 14..20],
            ),
            ("12 12 1", "12", folded, vec![0..2, 3..5]),
            ("1 11", "1", folded, vec![0..1, 2..3, 3..4]),
            (
                "café needle 東京 needle",
                "needle",
                exact,
                vec![5..11, 15..21],
            ),
            ("a café café", "café", exact, vec![2..6, 7..11]),
            ("α beta αbeta α", "α", exact_word, vec![0..1, 13..14]),
            ("α β βx xβ β", "β", exact_word, vec![2..3, 10..11]),
            ("éÉéÉ", "é", folded, vec![0..1, 1..2, 2..3, 3..4]),
            ("ab12 xx ab99", r"ab[0-9]{2}", regex, vec![0..4, 8..12]),
            (
                "α café42 東京9 café",
                r"café[0-9]{2}|東京[0-9]",
                regex,
                vec![2..8, 9..12],
            ),
            ("a aa a", "a", regex_word, vec![0..1, 5..6]),
            ("aaaa", "a?", regex, vec![0..1, 1..2, 2..3, 3..4]),
            ("fo fof f", "foo", folded, vec![]),
        ] {
            assert_eq!(
                find_matches(text, query, options),
                expected,
                "{query} in {text}"
            );
            let program = SearchProgram::compile(query, options).unwrap();
            assert_eq!(
                find_matches_interruptible(text, query, options, || true),
                Some(expected.clone())
            );
            assert!(search_text_interruptible(text, query, options, || false).is_none());
            for limit in [0, 1, expected.len(), usize::MAX] {
                let bounded = program.clone().with_result_limit(limit);
                let result = search_program_interruptible(text, &bounded, || true);
                assert_eq!(bounded.result_limit_exceeded(), expected.len() > limit);
                if expected.len() > limit {
                    assert!(result.is_none());
                } else {
                    assert_eq!(result.unwrap().matches, expected, "limit={limit}");
                }
            }
        }
        for (text, query) in [("a", "long"), ("é", "東京"), ("", "a")] {
            for match_case in [true, false] {
                let options = SearchOptions {
                    match_case,
                    ..SearchOptions::default()
                };
                assert!(search_text(text, query, options).matches.is_empty());
                let program = SearchProgram::compile(query, options).unwrap();
                assert!(search_program(text, &program).matches.is_empty());
            }
        }
    }

    #[test]
    fn cancellation_is_checked_during_scans_and_before_publication() {
        for (text, query, mode, match_case) in [
            ("a".repeat(128 * 1024), "a", SearchMode::PlainText, true),
            ("A".repeat(128 * 1024), "a", SearchMode::PlainText, false),
            ("Ab".repeat(64 * 1024), "ab", SearchMode::PlainText, false),
            ("é".repeat(128 * 1024), "é", SearchMode::PlainText, true),
            ("É".repeat(128 * 1024), "é", SearchMode::PlainText, false),
            ("é".repeat(128 * 1024), "é", SearchMode::Regex, true),
            ("a".to_owned(), "a", SearchMode::PlainText, true),
        ] {
            let mut calls = 0;
            assert!(
                search_text_interruptible(
                    &text,
                    query,
                    SearchOptions {
                        mode,
                        match_case,
                        whole_word: false
                    },
                    || {
                        calls += 1;
                        calls < 2
                    }
                )
                .is_none()
            );
            assert_eq!(calls, 2);
        }
    }

    #[test]
    fn regex_compilation_and_replacement_fail_without_partial_edits() {
        let options = SearchOptions {
            mode: SearchMode::Regex,
            match_case: true,
            whole_word: false,
        };
        for query in ["a*", "["] {
            let error = validate_search_query(query, options).unwrap();
            assert!(matches!(error, SearchError::UnsupportedRegex(_)));
            assert!(!error.message().is_empty());
            let outcome = search_text("a", query, options);
            assert!(outcome.matches.is_empty());
            assert_eq!(outcome.error, Some(error));
        }
        let program = SearchProgram::compile(r"([a-z]{1,8})-([0-9]{1,4})", options).unwrap();
        assert_eq!(
            program.expand_replacement("item-42", "$2:$1").unwrap(),
            "42:item"
        );
        assert!(matches!(
            program.expand_replacement("stale", "$2:$1"),
            Err(SearchError::ReplacementMismatch(_))
        ));
    }

    #[test]
    fn shared_result_budget_accepts_the_exact_limit() {
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
