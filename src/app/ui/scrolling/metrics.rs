use eframe::egui::{Rect, Vec2};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollInvariantError {
    InvalidRowHeight,
    InvalidViewportDimensions,
    InvalidContentExtent,
}

impl fmt::Display for ScrollInvariantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRowHeight => write!(f, "invalid editor row height"),
            Self::InvalidViewportDimensions => write!(f, "invalid editor viewport dimensions"),
            Self::InvalidContentExtent => write!(f, "invalid editor content extent"),
        }
    }
}

/// Per-frame layout measurements that the scroll manager needs to resolve
/// intents. Populated by the renderer after layout, before input arbitration.
#[derive(Clone, Copy, Debug)]
pub struct ViewportMetrics {
    /// Viewport rect on screen (pixels), excluding gutter and scrollbar.
    pub viewport_rect: Rect,
    /// Pixel height of one display row at current font.
    pub row_height: f32,
    /// Approximate pixel width of one column ("M") at current font. Used for
    /// horizontal page math and may be inaccurate for proportional fonts —
    /// horizontal scrolling is in pixels, not columns, so this is advisory only.
    pub column_width: f32,
    /// Whole display rows that fit in the viewport.
    pub visible_rows: u32,
    /// Whole columns that fit in the viewport (advisory).
    pub visible_columns: u32,
}

impl Default for ViewportMetrics {
    fn default() -> Self {
        Self {
            viewport_rect: Rect::from_min_size(eframe::egui::Pos2::ZERO, Vec2::ZERO),
            row_height: 0.0,
            column_width: 0.0,
            visible_rows: 0,
            visible_columns: 0,
        }
    }
}

impl ViewportMetrics {
    pub fn viewport_size(&self) -> Vec2 {
        self.viewport_rect.size()
    }

    pub fn validate(&self) -> Result<(), ScrollInvariantError> {
        if !self.row_height.is_finite() || self.row_height <= 0.0 {
            return Err(ScrollInvariantError::InvalidRowHeight);
        }
        let size = self.viewport_rect.size();
        if !size.x.is_finite() || !size.y.is_finite() || size.x < 0.0 || size.y < 0.0 {
            return Err(ScrollInvariantError::InvalidViewportDimensions);
        }
        Ok(())
    }
}

/// Total content extent. Derived from the display-row pipeline (Phase 3), not
/// from logical line count.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContentExtent {
    /// Total number of display rows (after wrap and folds).
    pub display_rows: u32,
    /// Total content height in pixels (`display_rows * row_height`).
    pub height: f32,
    /// Maximum line width across all display rows (pixels).
    pub max_line_width: f32,
}

impl ContentExtent {
    pub fn validate(&self) -> Result<(), ScrollInvariantError> {
        if !self.height.is_finite()
            || self.height < 0.0
            || !self.max_line_width.is_finite()
            || self.max_line_width < 0.0
        {
            return Err(ScrollInvariantError::InvalidContentExtent);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Pos2, vec2};

    #[test]
    fn viewport_metrics_reject_invalid_row_height() {
        let metrics = ViewportMetrics {
            viewport_rect: Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0)),
            row_height: f32::NAN,
            column_width: 8.0,
            visible_rows: 10,
            visible_columns: 50,
        };

        assert_eq!(
            metrics.validate(),
            Err(ScrollInvariantError::InvalidRowHeight)
        );
    }

    #[test]
    fn content_extent_rejects_non_finite_width() {
        let extent = ContentExtent {
            display_rows: 10,
            height: 200.0,
            max_line_width: f32::INFINITY,
        };

        assert_eq!(
            extent.validate(),
            Err(ScrollInvariantError::InvalidContentExtent)
        );
    }
}
