//! Character-index selections shared by documents, history, views and adapters.
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharCursor {
    pub index: usize,
    pub prefer_next_row: bool,
}

impl CharCursor {
    #[must_use]
    pub fn new(index: usize) -> Self {
        Self {
            index,
            prefer_next_row: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorRange {
    pub primary: CharCursor,
    pub secondary: CharCursor,
}

impl CursorRange {
    #[must_use]
    pub fn one(cursor: CharCursor) -> Self {
        Self {
            primary: cursor,
            secondary: cursor,
        }
    }

    #[must_use]
    pub fn two(min: usize, max: usize) -> Self {
        Self {
            primary: CharCursor::new(max),
            secondary: CharCursor::new(min),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.primary.index == self.secondary.index
    }

    #[must_use]
    pub fn sorted_indices(&self) -> (usize, usize) {
        let a = self.primary.index;
        let b = self.secondary.index;
        (a.min(b), a.max(b))
    }

    #[must_use]
    pub fn as_sorted_char_range(&self) -> Range<usize> {
        let (start, end) = self.sorted_indices();
        start..end
    }
}
