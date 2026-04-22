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
    let mut pos = index.min(piece_tree.len_chars());
    while pos > 0
        && piece_tree
            .char_at(pos - 1)
            .is_some_and(|ch| classify(ch) == CharClass::Whitespace)
    {
        pos -= 1;
    }
    if pos == 0 {
        return 0;
    }

    let class = classify(piece_tree.char_at(pos - 1).unwrap_or_default());
    while pos > 0
        && piece_tree
            .char_at(pos - 1)
            .is_some_and(|ch| classify(ch) == class)
    {
        pos -= 1;
    }
    pos
}

/// Move right to the end of the current word, then skip whitespace to land on
/// the start of the next word.
pub(super) fn find_word_boundary_right(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let total = piece_tree.len_chars();
    let mut pos = index.min(total);
    if pos >= total {
        return total;
    }

    let class = classify(piece_tree.char_at(pos).unwrap_or_default());
    while pos < total
        && piece_tree
            .char_at(pos)
            .is_some_and(|ch| classify(ch) == class)
    {
        pos += 1;
    }

    while pos < total
        && piece_tree
            .char_at(pos)
            .is_some_and(|ch| classify(ch) == CharClass::Whitespace)
    {
        pos += 1;
    }
    pos
}

/// Find the start of the word surrounding `index` (for double-click selection).
pub(super) fn word_start(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let mut pos = index.min(piece_tree.len_chars());
    if pos == 0 {
        return 0;
    }
    let class = classify(piece_tree.char_at(pos - 1).unwrap_or_default());
    while pos > 0
        && piece_tree
            .char_at(pos - 1)
            .is_some_and(|ch| classify(ch) == class)
    {
        pos -= 1;
    }
    pos
}

/// Find the end of the word surrounding `index` (for double-click selection).
pub(super) fn word_end(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let total = piece_tree.len_chars();
    let mut pos = index.min(total);
    if pos >= total {
        return total;
    }
    let class = classify(piece_tree.char_at(pos).unwrap_or_default());
    while pos < total
        && piece_tree
            .char_at(pos)
            .is_some_and(|ch| classify(ch) == class)
    {
        pos += 1;
    }
    pos
}
