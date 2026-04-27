//! Stable scroll anchor for the editor.
//!
//! The locked design calls for a piece-tree anchor + fractional display-row
//! offset. The current piece tree does not expose stable anchors, so v1 uses a
//! logical-line + intra-line byte offset surrogate. The surface API is the
//! same; only the inner representation needs to change when piece-tree anchors
//! land.

/// A stable position in the document used as a top-of-viewport reference.
///
/// `display_row_offset` is a fractional offset (0.0..1.0+) measured in display
/// rows from the start of the wrapped block that begins at this logical line.
/// 0.0 means the wrapped block's first display row sits at the viewport top.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollAnchor {
    pub logical_line: u32,
    pub byte_in_line: u32,
    pub display_row_offset: f32,
}

impl ScrollAnchor {
    pub const TOP: Self = Self {
        logical_line: 0,
        byte_in_line: 0,
        display_row_offset: 0.0,
    };

    pub fn at_line(line: u32) -> Self {
        Self {
            logical_line: line,
            byte_in_line: 0,
            display_row_offset: 0.0,
        }
    }
}
