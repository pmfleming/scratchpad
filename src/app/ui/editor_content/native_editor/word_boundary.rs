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

fn class_before(piece_tree: &PieceTreeLite, pos: usize) -> CharClass {
    classify(piece_tree.char_at(pos - 1).unwrap_or_default())
}

fn class_at(piece_tree: &PieceTreeLite, pos: usize) -> CharClass {
    classify(piece_tree.char_at(pos).unwrap_or_default())
}

fn scan_left_while(piece_tree: &PieceTreeLite, mut pos: usize, class: CharClass) -> usize {
    while pos > 0
        && piece_tree
            .char_at(pos - 1)
            .is_some_and(|ch| classify(ch) == class)
    {
        pos -= 1;
    }
    pos
}

fn scan_right_while(
    piece_tree: &PieceTreeLite,
    mut pos: usize,
    total: usize,
    class: CharClass,
) -> usize {
    while pos < total
        && piece_tree
            .char_at(pos)
            .is_some_and(|ch| classify(ch) == class)
    {
        pos += 1;
    }
    pos
}

/// Move left to the start of the previous word, skipping whitespace first then
/// stopping at a character-class transition.
pub(super) fn find_word_boundary_left(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let mut pos = index.min(piece_tree.len_chars());
    pos = scan_left_while(piece_tree, pos, CharClass::Whitespace);
    if pos == 0 {
        return 0;
    }

    scan_left_while(piece_tree, pos, class_before(piece_tree, pos))
}

/// Move right to the end of the current word, then skip whitespace to land on
/// the start of the next word.
pub(super) fn find_word_boundary_right(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let total = piece_tree.len_chars();
    let mut pos = index.min(total);
    if pos >= total {
        return total;
    }

    pos = scan_right_while(piece_tree, pos, total, class_at(piece_tree, pos));
    scan_right_while(piece_tree, pos, total, CharClass::Whitespace)
}

/// Find the start of the word surrounding `index` (for double-click selection).
pub(super) fn word_start(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let pos = index.min(piece_tree.len_chars());
    if pos == 0 {
        return 0;
    }
    scan_left_while(piece_tree, pos, class_before(piece_tree, pos))
}

/// Find the end of the word surrounding `index` (for double-click selection).
pub(super) fn word_end(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let total = piece_tree.len_chars();
    let pos = index.min(total);
    if pos >= total {
        return total;
    }
    scan_right_while(piece_tree, pos, total, class_at(piece_tree, pos))
}

#[cfg(test)]
mod tests {
    use super::{
        PieceTreeLite, find_word_boundary_left, find_word_boundary_right, word_end, word_start,
    };

    fn tree(text: &str) -> PieceTreeLite {
        PieceTreeLite::from_string(text.to_owned())
    }

    #[test]
    fn left_boundary_skips_whitespace_then_previous_word() {
        let tree = tree("alpha  beta");

        assert_eq!(find_word_boundary_left(&tree, 12), 7);
        assert_eq!(find_word_boundary_left(&tree, 7), 0);
    }

    #[test]
    fn right_boundary_consumes_current_class_then_whitespace() {
        let tree = tree("alpha  beta");

        assert_eq!(find_word_boundary_right(&tree, 0), 7);
        assert_eq!(find_word_boundary_right(&tree, 7), 11);
    }

    #[test]
    fn underscores_are_word_characters() {
        let tree = tree("foo_bar baz");

        assert_eq!(find_word_boundary_right(&tree, 0), 8);
        assert_eq!(word_start(&tree, 6), 0);
        assert_eq!(word_end(&tree, 2), 7);
    }

    #[test]
    fn punctuation_forms_its_own_boundary_class() {
        let tree = tree("alpha..beta");

        assert_eq!(find_word_boundary_right(&tree, 5), 7);
        assert_eq!(find_word_boundary_left(&tree, 7), 5);
    }

    #[test]
    fn unicode_letters_and_digits_are_word_characters() {
        let tree = tree("naive α42 beta");

        assert_eq!(word_start(&tree, 8), 6);
        assert_eq!(word_end(&tree, 6), 9);
    }

    #[test]
    fn boundaries_clamp_to_document_edges() {
        let tree = tree("alpha");

        assert_eq!(find_word_boundary_left(&tree, usize::MAX), 0);
        assert_eq!(find_word_boundary_right(&tree, usize::MAX), 5);
        assert_eq!(word_start(&tree, usize::MAX), 0);
        assert_eq!(word_end(&tree, usize::MAX), 5);
    }
}
