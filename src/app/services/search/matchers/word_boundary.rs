pub(super) struct WholeWordMatcher<'a> {
    text: &'a str,
    enabled: bool,
}

impl<'a> WholeWordMatcher<'a> {
    pub(super) fn new(text: &'a str, enabled: bool) -> Self {
        Self { text, enabled }
    }

    pub(super) fn allows(&self, start: usize, end: usize) -> bool {
        !self.enabled || is_whole_word_match(self.text, start, end)
    }

    pub(super) fn allows_byte_range(&self, start_byte: usize, end_byte: usize) -> bool {
        !self.enabled || is_whole_word_byte_match(self.text, start_byte, end_byte)
    }
}

pub(super) fn whole_word_allows(
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

pub(super) fn ascii_whole_word_allows(
    text_bytes: &[u8],
    whole_word: bool,
    start: usize,
    end: usize,
) -> bool {
    !whole_word || is_ascii_whole_word_match(text_bytes, start, end)
}

fn is_ascii_whole_word_match(text_bytes: &[u8], start: usize, end: usize) -> bool {
    whole_word_boundary_allows(text_bytes, start, end, |byte| is_ascii_word_char(*byte))
}

fn is_whole_word_match(text: &str, start: usize, end: usize) -> bool {
    let before_is_word = start > 0 && text.chars().nth(start - 1).is_some_and(is_word_char);
    let after_is_word = text.chars().nth(end).is_some_and(is_word_char);
    !before_is_word && !after_is_word
}

fn is_whole_word_byte_match(text: &str, start_byte: usize, end_byte: usize) -> bool {
    if !text.is_char_boundary(start_byte) || !text.is_char_boundary(end_byte) {
        return false;
    }

    let before_is_word = start_byte > 0
        && text[..start_byte]
            .chars()
            .next_back()
            .is_some_and(is_word_char);
    let after_is_word =
        end_byte < text.len() && text[end_byte..].chars().next().is_some_and(is_word_char);
    !before_is_word && !after_is_word
}

fn whole_word_boundary_allows<T, F>(
    items: &[T],
    start: usize,
    end: usize,
    mut is_word_item: F,
) -> bool
where
    F: FnMut(&T) -> bool,
{
    let before_is_word = start > 0 && is_word_item(&items[start - 1]);
    let after_is_word = end < items.len() && is_word_item(&items[end]);
    !before_is_word && !after_is_word
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn is_ascii_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_whole_word_treats_underscore_as_word_character() {
        let text = b"foo _foo foo_ foo";

        assert!(ascii_whole_word_allows(text, true, 0, 3));
        assert!(!ascii_whole_word_allows(text, true, 5, 8));
        assert!(!ascii_whole_word_allows(text, true, 9, 12));
        assert!(ascii_whole_word_allows(text, true, 14, 17));
    }

    #[test]
    fn unicode_whole_word_treats_letters_digits_and_underscore_as_word_characters() {
        let matcher = WholeWordMatcher::new("α β42 α_beta α", true);

        assert!(matcher.allows(0, 1));
        assert!(!matcher.allows(2, 3));
        assert!(!matcher.allows(6, 7));
        assert!(matcher.allows(13, 14));
    }

    #[test]
    fn disabled_whole_word_matcher_allows_embedded_matches() {
        let matcher = WholeWordMatcher::new("prefixneedle", false);

        assert!(matcher.allows(6, 12));
        assert!(matcher.allows_byte_range(6, 12));
        assert!(ascii_whole_word_allows(b"prefixneedle", false, 6, 12));
    }

    #[test]
    fn unicode_byte_range_whole_word_checks_adjacent_scalars() {
        let matcher = WholeWordMatcher::new("α β βx xβ β", true);

        assert!(matcher.allows_byte_range("α ".len(), "α β".len()));
        assert!(!matcher.allows_byte_range("α β ".len(), "α β β".len()));
        assert!(!matcher.allows_byte_range("α β βx x".len(), "α β βx xβ".len()));
        assert!(matcher.allows_byte_range("α β βx xβ ".len(), "α β βx xβ β".len()));
    }

    #[test]
    fn dispatcher_uses_ascii_fast_path_when_text_is_ascii() {
        let matcher = WholeWordMatcher::new("ignored", true);

        assert!(whole_word_allows(true, b"a foo b", &matcher, true, 2, 5));
        assert!(!whole_word_allows(
            true,
            b"a foobar b",
            &matcher,
            true,
            2,
            5
        ));
    }
}
