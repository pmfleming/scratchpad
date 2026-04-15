use std::ops::Range;

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
        return find_matches_ascii_case_insensitive(text, query, options.whole_word);
    }

    let query_char_len = query.chars().count();
    let text_chars = text.chars().collect::<Vec<_>>();
    if query_char_len > text_chars.len() {
        return Vec::new();
    }

    let char_to_byte = char_to_byte_map(text);
    let folded_query = query.to_lowercase();
    let mut matches = Vec::new();

    for start in 0..=text_chars.len() - query_char_len {
        let end = start + query_char_len;
        let candidate = &text[char_to_byte[start]..char_to_byte[end]];
        if candidate.to_lowercase() != folded_query {
            continue;
        }
        if options.whole_word && !is_whole_word_match(&text_chars, start, end) {
            continue;
        }
        matches.push(start..end);
    }

    matches
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

    let query_char_len = query.chars().count();
    let text_chars = text.chars().collect::<Vec<_>>();
    if query_char_len > text_chars.len() {
        return Some(Vec::new());
    }

    let char_to_byte = char_to_byte_map(text);
    let folded_query = (!options.match_case).then(|| query.to_lowercase());
    let mut matches = Vec::new();

    for start in 0..=text_chars.len() - query_char_len {
        if start % 256 == 0 && !should_continue() {
            return None;
        }
        let end = start + query_char_len;
        let candidate = &text[char_to_byte[start]..char_to_byte[end]];
        let candidate_matches = if options.match_case {
            candidate == query
        } else {
            candidate.to_lowercase() == folded_query.as_deref().unwrap_or_default()
        };
        if !candidate_matches {
            continue;
        }
        if options.whole_word && !is_whole_word_match(&text_chars, start, end) {
            continue;
        }
        matches.push(start..end);
    }

    if !should_continue() {
        return None;
    }

    Some(matches)
}

pub fn next_match_index(total_matches: usize, current: Option<usize>) -> Option<usize> {
    match total_matches {
        0 => None,
        _ => Some(current.map_or(0, |index| (index + 1) % total_matches)),
    }
}

pub fn previous_match_index(total_matches: usize, current: Option<usize>) -> Option<usize> {
    match total_matches {
        0 => None,
        _ => Some(current.map_or(total_matches - 1, |index| {
            if index == 0 {
                total_matches - 1
            } else {
                index - 1
            }
        })),
    }
}

fn find_matches_case_sensitive(text: &str, query: &str, whole_word: bool) -> Vec<Range<usize>> {
    let mut matches = Vec::new();
    let byte_to_char = byte_to_char_map(text);
    let text_chars = whole_word.then(|| text.chars().collect::<Vec<_>>());

    for (start_byte, candidate) in text.match_indices(query) {
        let start = byte_to_char[start_byte];
        let end = start + candidate.chars().count();
        if whole_word
            && text_chars
                .as_deref()
                .is_some_and(|chars| !is_whole_word_match(chars, start, end))
        {
            continue;
        }
        matches.push(start..end);
    }

    matches
}

fn find_matches_ascii_case_insensitive(
    text: &str,
    query: &str,
    whole_word: bool,
) -> Vec<Range<usize>> {
    let folded_text = text.to_ascii_lowercase();
    let folded_query = query.to_ascii_lowercase();
    let mut matches = Vec::new();
    let text_chars = whole_word.then(|| text.chars().collect::<Vec<_>>());

    for (start_byte, candidate) in folded_text.match_indices(&folded_query) {
        let start = start_byte;
        let end = start + candidate.len();
        if whole_word
            && text_chars
                .as_deref()
                .is_some_and(|chars| !is_whole_word_match(chars, start, end))
        {
            continue;
        }
        matches.push(start..end);
    }

    matches
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

#[cfg(test)]
mod tests {
    use super::{SearchOptions, find_matches, next_match_index, previous_match_index};

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
}
