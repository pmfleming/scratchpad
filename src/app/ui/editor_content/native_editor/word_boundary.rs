use crate::app::domain::buffer::PieceTreeLite;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Whitespace,
    Punctuation,
    Word,
}

fn classify(ch: char) -> CharClass {
    if ch.is_whitespace() {
        CharClass::Whitespace
    } else if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

/// Move left to the start of the previous word, skipping whitespace first then
/// stopping at a character-class transition.
pub(super) fn find_word_boundary_left(piece_tree: &PieceTreeLite, index: usize) -> usize {
    if index == 0 {
        return 0;
    }
    let text = piece_tree.extract_range(0..index);
    let chars: Vec<char> = text.chars().collect();
    let mut pos = chars.len();

    // Skip trailing whitespace
    while pos > 0 && classify(chars[pos - 1]) == CharClass::Whitespace {
        pos -= 1;
    }
    if pos == 0 {
        return 0;
    }

    // Skip run of same class
    let class = classify(chars[pos - 1]);
    while pos > 0 && classify(chars[pos - 1]) == class {
        pos -= 1;
    }
    pos
}

/// Move right to the end of the current word, then skip whitespace to land on
/// the start of the next word.
pub(super) fn find_word_boundary_right(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let total = piece_tree.len_chars();
    if index >= total {
        return total;
    }
    let text = piece_tree.extract_range(index..total);
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;

    // Skip run of same class as current char
    let class = classify(chars[pos]);
    while pos < chars.len() && classify(chars[pos]) == class {
        pos += 1;
    }

    // Skip whitespace after the word
    while pos < chars.len() && classify(chars[pos]) == CharClass::Whitespace {
        pos += 1;
    }
    index + pos
}

/// Find the start of the word surrounding `index` (for double-click selection).
pub(super) fn word_start(piece_tree: &PieceTreeLite, index: usize) -> usize {
    if index == 0 {
        return 0;
    }
    let text = piece_tree.extract_range(0..index);
    let chars: Vec<char> = text.chars().collect();
    let mut pos = chars.len();
    if pos == 0 {
        return 0;
    }

    let class = classify(chars[pos - 1]);
    while pos > 0 && classify(chars[pos - 1]) == class {
        pos -= 1;
    }
    pos
}

/// Find the end of the word surrounding `index` (for double-click selection).
pub(super) fn word_end(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let total = piece_tree.len_chars();
    if index >= total {
        return total;
    }
    let text = piece_tree.extract_range(index..total);
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    if chars.is_empty() {
        return index;
    }

    let class = classify(chars[0]);
    while pos < chars.len() && classify(chars[pos]) == class {
        pos += 1;
    }
    index + pos
}
