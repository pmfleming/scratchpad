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
    #[must_use]
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
        if let Some(budget) = &self.result_budget {
            budget
                .exceeded
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
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
        // Compile first so malformed patterns retain their error and position.
        let regex = match options.mode {
            SearchMode::PlainText => None,
            SearchMode::Regex => Some(compile_regex(query, options)?),
        };
        let max_match_chars = match options.mode {
            SearchMode::PlainText => query.chars().count(),
            SearchMode::Regex => regex_max_match_chars(query).ok_or_else(|| {
                SearchError::UnsupportedRegex(
                    "Regex search requires a bounded maximum match length.".to_owned(),
                )
            })?,
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

    /// The range is in bytes within the original context. Assertions and
    /// captures must agree with the exact match that was reviewed.
    pub fn expand_replacement_at(
        &self,
        context: &str,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<String, SearchError> {
        let Some(regex) = &self.regex else {
            return Ok(replacement.to_owned());
        };
        let captures = regex
            .captures_at(context, range.start)
            .filter(|c| c.get(0).is_some_and(|m| m.range() == range))
            .ok_or_else(|| {
                SearchError::ReplacementMismatch(
                    "Search results changed; refresh before replacing.".into(),
                )
            })?;
        let mut expanded = String::new();
        captures.expand(replacement, &mut expanded);
        Ok(expanded)
    }
}

#[must_use]
pub fn find_matches(text: &str, query: &str, options: SearchOptions) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    SearchProgram::compile(query, options)
        .map(|program| search_program(text, &program).matches)
        .unwrap_or_default()
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

/// Match the source's uniform lower, upper or title case. Mixed case and
/// uncased text leave the user's replacement unchanged.
pub(crate) fn preserve_replacement_case(source: &str, replacement: &str) -> String {
    match uniform_letter_case(source) {
        Some(LetterCase::Upper) => replacement.to_uppercase(),
        Some(LetterCase::Lower) => replacement.to_lowercase(),
        Some(LetterCase::Title) => title_case(replacement),
        None => replacement.to_owned(),
    }
}

enum LetterCase {
    Upper,
    Lower,
    Title,
}

fn is_cased(c: char) -> bool {
    c.is_lowercase() || c.is_uppercase()
}

fn uniform_letter_case(text: &str) -> Option<LetterCase> {
    let mut letters = text.chars().filter(|c| is_cased(*c));
    let first_upper = letters.next()?.is_uppercase();
    let (mut rest_upper, mut rest_lower) = (true, true);
    for c in letters {
        rest_upper &= c.is_uppercase();
        rest_lower &= c.is_lowercase();
    }
    match (first_upper, rest_upper, rest_lower) {
        (true, true, _) => Some(LetterCase::Upper),
        (false, _, true) => Some(LetterCase::Lower),
        (true, false, true) => Some(LetterCase::Title),
        _ => None,
    }
}

/// Lowercase `text`, then uppercase its first cased character.
fn title_case(text: &str) -> String {
    let lower = text.to_lowercase();
    let Some((index, first)) = lower.char_indices().find(|(_, c)| is_cased(*c)) else {
        return lower;
    };
    let mut titled = String::with_capacity(lower.len());
    titled.push_str(&lower[..index]);
    titled.extend(first.to_uppercase());
    titled.push_str(&lower[index + first.len_utf8()..]);
    titled
}

#[cfg(test)]
mod tests {
    use super::{
        SearchError, SearchMode, SearchOptions, SearchProgram, find_matches,
        preserve_replacement_case, search_program, search_program_interruptible,
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
            assert!(search_program_interruptible(text, &program, || false).is_none());
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
            let options = SearchOptions {
                mode,
                match_case,
                whole_word: false,
            };
            let program = SearchProgram::compile(query, options).unwrap();
            let outcome = search_program_interruptible(&text, &program, || {
                calls += 1;
                calls < 2
            });
            assert!(outcome.is_none());
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
        let error = SearchProgram::compile("a*", options).unwrap_err();
        assert!(matches!(error, SearchError::UnsupportedRegex(_)));
        assert!(!error.message().is_empty());
        assert!(find_matches("a", "a*", options).is_empty());
        assert!(matches!(
            SearchProgram::compile("(", options),
            Err(SearchError::InvalidRegex(_))
        ));
        let program = SearchProgram::compile(r"([a-z]{1,8})-([0-9]{1,4})", options).unwrap();
        assert_eq!(
            program
                .expand_replacement_at("item-42", 0..7, "$2:$1")
                .unwrap(),
            "42:item"
        );
        assert!(matches!(
            program.expand_replacement_at("stale", 0..5, "$2:$1"),
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

    #[test]
    fn replacement_case_follows_uniform_source_case_only() {
        for (source, replacement, expected) in [
            ("FOO", "bar baz", "BAR BAZ"),
            ("X", "bar", "BAR"),
            ("foo", "Bar BAZ", "bar baz"),
            ("Foo", "bAR baz", "Bar baz"),
            ("Éclair", "1 école", "1 École"),
            ("fOO", "bar", "bar"),
            ("123", "Bar", "Bar"),
            ("Foo", "123", "123"),
        ] {
            assert_eq!(
                preserve_replacement_case(source, replacement),
                expected,
                "{source} -> {replacement}"
            );
        }
    }
}
