use eframe::egui::{Rect, Vec2};

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
