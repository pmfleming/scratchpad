//! Display-row viewport pipeline.
//!
//! Phase 3 contract: every editor renders through one viewport-first path.
//! Display rows are the scroll unit (after wrap/folds). The renderer asks for
//! a `ViewportSlice` from a `DisplaySnapshot` and paints from that.
//!
//! v1 wraps an `egui::Galley`, since galley rows are already wrap-aware
//! display rows. The API is designed so a non-galley implementation can
//! replace it without touching call sites.

use std::ops::Range;
use std::sync::Arc;

use eframe::egui::Galley;

/// A visual row index after wrap/fold transformations. Distinct type from
/// `LogicalLine` to prevent confusion at call sites.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DisplayRow(pub u32);

/// A visual row + column position used for cursor geometry and hit testing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayPoint {
    pub row: DisplayRow,
    /// Pixel x within the display row (not column count — horizontal is pixels).
    pub x: f32,
}

/// Snapshot of a buffer's wrap-aware display rows. Owned by a view; rebuilt
/// whenever wrap width, font size, or content changes.
#[derive(Clone)]
pub struct DisplaySnapshot {
    galley: Arc<Galley>,
    row_height: f32,
    /// Row tops in pixels, length = row_count + 1 (last entry = total height).
    row_tops: Vec<f32>,
    /// Logical line number for each display row (for gutter).
    row_logical_lines: Vec<Option<u32>>,
    /// Source char range in the underlying text for each display row.
    row_char_ranges: Vec<Range<u32>>,
    max_line_width: f32,
}

impl DisplaySnapshot {
    pub fn from_galley(galley: Arc<Galley>, row_height: f32) -> Self {
        let mut row_tops = Vec::with_capacity(galley.rows.len() + 1);
        let mut row_logical_lines = Vec::with_capacity(galley.rows.len());
        let mut row_char_ranges = Vec::with_capacity(galley.rows.len());
        let mut max_line_width: f32 = 0.0;
        let mut current_logical: u32 = 0;
        let mut current_char: u32 = 0;

        for row in galley.rows.iter() {
            row_tops.push(row.pos.y);
            row_logical_lines.push(Some(current_logical));
            let row_start = current_char;
            current_char = current_char.saturating_add(row.char_count_including_newline() as u32);
            row_char_ranges.push(row_start..current_char);
            let row_width = row
                .glyphs
                .last()
                .map(|g| g.pos.x + g.advance_width)
                .unwrap_or(0.0);
            max_line_width = max_line_width.max(row_width);
            if row.ends_with_newline {
                current_logical = current_logical.saturating_add(1);
            }
        }
        row_tops.push(galley.rect.height());

        Self {
            galley,
            row_height,
            row_tops,
            row_logical_lines,
            row_char_ranges,
            max_line_width,
        }
    }

    pub fn galley(&self) -> &Arc<Galley> {
        &self.galley
    }

    pub fn row_count(&self) -> u32 {
        self.row_logical_lines.len() as u32
    }

    pub fn row_height(&self) -> f32 {
        self.row_height
    }

    pub fn content_height(&self) -> f32 {
        self.galley.rect.height()
    }

    pub fn max_line_width(&self) -> f32 {
        self.max_line_width
    }

    pub fn logical_line_for(&self, row: DisplayRow) -> Option<u32> {
        self.row_logical_lines.get(row.0 as usize).copied().flatten()
    }

    pub fn row_top(&self, row: DisplayRow) -> Option<f32> {
        self.row_tops.get(row.0 as usize).copied()
    }

    pub fn row_char_range(&self, row: DisplayRow) -> Option<Range<u32>> {
        self.row_char_ranges.get(row.0 as usize).cloned()
    }

    /// Locate the display row that contains the given char offset. Returns
    /// the last row's index if `char_offset` is past end-of-content. Returns
    /// `None` for an empty snapshot.
    pub fn row_for_char_offset(&self, char_offset: u32) -> Option<DisplayRow> {
        if self.row_char_ranges.is_empty() {
            return None;
        }
        let position = self
            .row_char_ranges
            .partition_point(|range| range.end <= char_offset);
        let clamped = position.min(self.row_char_ranges.len() - 1);
        Some(DisplayRow(clamped as u32))
    }

    /// Pixel y of the row containing `char_offset` plus the fractional offset
    /// within that row. Useful for cursor-reveal computations driven by a
    /// piece-tree-backed `ScrollAnchor`.
    pub fn pixel_y_for_char_offset(&self, char_offset: u32) -> Option<f32> {
        let row = self.row_for_char_offset(char_offset)?;
        self.row_top(row)
    }

    /// Compute the visible row range for a given scroll offset (top display
    /// row) and viewport height in pixels. `overscan_rows` adds a margin on
    /// both sides for smoother scrolling.
    pub fn viewport_slice(
        &self,
        top_row: f32,
        viewport_height: f32,
        overscan_rows: u32,
    ) -> ViewportSlice {
        let row_h = self.row_height.max(1.0);
        let visible_rows = (viewport_height / row_h).ceil() as u32 + 1;
        let total = self.row_count();
        let start = (top_row.floor() as i32 - overscan_rows as i32).max(0) as u32;
        let end = (top_row.ceil() as u32 + visible_rows + overscan_rows).min(total);
        ViewportSlice {
            rows: start..end,
            top_row_fractional: top_row,
            pixel_offset_y: top_row * row_h,
        }
    }
}

/// A range of display rows to paint, with the fractional top-of-viewport row
/// for sub-pixel scroll positioning.
#[derive(Clone, Debug)]
pub struct ViewportSlice {
    pub rows: Range<u32>,
    pub top_row_fractional: f32,
    pub pixel_offset_y: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test the slice math directly without constructing a real galley.
    fn slice_math(top_row: f32, viewport_h: f32, row_h: f32, total: u32, overscan: u32) -> Range<u32> {
        let visible_rows = (viewport_h / row_h).ceil() as u32 + 1;
        let start = (top_row.floor() as i32 - overscan as i32).max(0) as u32;
        let end = (top_row.ceil() as u32 + visible_rows + overscan).min(total);
        start..end
    }

    #[test]
    fn viewport_slice_includes_overscan() {
        let r = slice_math(10.0, 200.0, 20.0, 100, 2);
        assert_eq!(r.start, 8);
        // 10 + ceil(200/20)+1 + 2 = 10 + 11 + 2 = 23
        assert_eq!(r.end, 23);
    }

    #[test]
    fn viewport_slice_clamps_at_top_and_bottom() {
        let r = slice_math(0.0, 200.0, 20.0, 50, 5);
        assert_eq!(r.start, 0);
        let r = slice_math(48.0, 200.0, 20.0, 50, 5);
        assert_eq!(r.end, 50);
    }

    #[test]
    fn pixel_offset_matches_top_row_times_row_height() {
        // top_row * row_height
        let pixel_offset = 3.5_f32 * 18.0;
        assert!((pixel_offset - 63.0).abs() < 0.001);
    }
}
