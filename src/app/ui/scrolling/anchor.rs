//! Stable scroll anchor for the editor.
//!
//! Two representations coexist:
//!
//! * `ScrollAnchor::Piece(AnchorId, display_row_offset)` — a stable
//!   piece-tree anchor that survives edits above the viewport. The renderer
//!   resolves the anchor through `PieceTreeLite::anchor_position` to a
//!   current `char_offset`, then converts that to a logical line / display
//!   row via the active `DisplaySnapshot`.
//! * `ScrollAnchor::Logical { logical_line, byte_in_line, display_row_offset }`
//!   — the v1 fallback used when the renderer has not yet handed the
//!   `ScrollManager` a piece-tree-backed anchor (e.g. unit tests, the
//!   first frame before any document is loaded). Edits above the viewport
//!   produce visible jumps under this fallback; that is acceptable for v1
//!   per the locked design and called out in the plan.
//!
//! `display_row_offset` is a fractional offset (0.0..1.0+) measured in
//! display rows from the start of the wrapped block that contains this
//! anchor's char position. 0.0 means the wrapped block's first display row
//! sits at the viewport top.

use crate::app::domain::buffer::AnchorId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollAnchor {
    /// Stable piece-tree-backed anchor. Survives edits above the viewport.
    Piece {
        anchor: AnchorId,
        display_row_offset: f32,
    },
    /// v1 fallback: logical line + intra-line byte offset. Used until a
    /// piece-tree-backed anchor is supplied.
    Logical {
        logical_line: u32,
        byte_in_line: u32,
        display_row_offset: f32,
    },
}

impl Default for ScrollAnchor {
    fn default() -> Self {
        Self::TOP
    }
}

impl ScrollAnchor {
    pub const TOP: Self = Self::Logical {
        logical_line: 0,
        byte_in_line: 0,
        display_row_offset: 0.0,
    };

    pub fn at_line(line: u32) -> Self {
        Self::Logical {
            logical_line: line,
            byte_in_line: 0,
            display_row_offset: 0.0,
        }
    }

    pub fn at_piece(anchor: AnchorId) -> Self {
        Self::Piece {
            anchor,
            display_row_offset: 0.0,
        }
    }

    pub fn display_row_offset(self) -> f32 {
        match self {
            Self::Piece { display_row_offset, .. }
            | Self::Logical { display_row_offset, .. } => display_row_offset,
        }
    }

    pub fn with_display_row_offset(self, offset: f32) -> Self {
        match self {
            Self::Piece { anchor, .. } => Self::Piece {
                anchor,
                display_row_offset: offset,
            },
            Self::Logical {
                logical_line,
                byte_in_line,
                ..
            } => Self::Logical {
                logical_line,
                byte_in_line,
                display_row_offset: offset,
            },
        }
    }

    /// Logical line component, when this anchor is a v1 fallback. `None` for
    /// piece-tree-backed anchors (which require tree resolution).
    pub fn logical_line(self) -> Option<u32> {
        match self {
            Self::Logical { logical_line, .. } => Some(logical_line),
            Self::Piece { .. } => None,
        }
    }

    /// Piece-tree anchor handle, when this is a piece-backed anchor.
    pub fn piece_anchor(self) -> Option<AnchorId> {
        match self {
            Self::Piece { anchor, .. } => Some(anchor),
            Self::Logical { .. } => None,
        }
    }
}
