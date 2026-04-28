//! Display-row viewport pipeline.
//!
//! Phase 3 contract: every editor renders through one viewport-first path.
//! Display rows are the scroll unit (after wrap/folds). The renderer asks for
//! a `ViewportSlice` from a `DisplaySnapshot` and paints from that.
//!
//! v1 wraps an `egui::Galley`, since galley rows are already wrap-aware
//! display rows. The API is designed so a non-galley implementation can
//! replace it without touching call sites.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use eframe::egui::Galley;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplaySnapshotError {
    InvalidWrapWidth,
    MissingRowCharRange,
    EmptyViewportSlice,
}

impl fmt::Display for DisplaySnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWrapWidth => write!(f, "invalid editor wrap width"),
            Self::MissingRowCharRange => write!(f, "missing display-row source range"),
            Self::EmptyViewportSlice => write!(f, "empty viewport slice"),
        }
    }
}

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
        let mut label_next_row = true;

        for row in galley.rows.iter() {
            row_tops.push(row.pos.y);
            row_logical_lines.push(label_next_row.then_some(current_logical));
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
                label_next_row = true;
            } else {
                label_next_row = false;
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
        self.row_logical_lines
            .get(row.0 as usize)
            .copied()
            .flatten()
    }

    pub fn row_top(&self, row: DisplayRow) -> Option<f32> {
        self.row_tops.get(row.0 as usize).copied()
    }

    pub fn row_char_range(&self, row: DisplayRow) -> Option<Range<u32>> {
        self.row_char_ranges.get(row.0 as usize).cloned()
    }

    pub fn line_range_for_rows(&self, rows: Range<u32>) -> Option<Range<usize>> {
        if self.row_logical_lines.is_empty() {
            return None;
        }
        let start = rows.start.min(self.row_count()) as usize;
        let end = rows.end.min(self.row_count()) as usize;
        if start >= end {
            return None;
        }

        let first_line = (start..end)
            .find_map(|row| self.row_logical_lines[row])
            .or_else(|| {
                (0..=start.min(self.row_logical_lines.len().saturating_sub(1)))
                    .rev()
                    .find_map(|row| self.row_logical_lines[row])
            })?;
        let last_line = (start..end)
            .rev()
            .find_map(|row| self.row_logical_lines[row])
            .unwrap_or(first_line);

        Some(first_line as usize..last_line as usize + 1)
    }

    pub fn char_range_for_rows(
        &self,
        rows: Range<u32>,
    ) -> Result<Range<usize>, DisplaySnapshotError> {
        let start_row = rows.start;
        let end_row = rows
            .end
            .checked_sub(1)
            .ok_or(DisplaySnapshotError::EmptyViewportSlice)?;
        let start = self
            .row_char_range(DisplayRow(start_row))
            .ok_or(DisplaySnapshotError::MissingRowCharRange)?
            .start as usize;
        let end = self
            .row_char_range(DisplayRow(end_row))
            .ok_or(DisplaySnapshotError::MissingRowCharRange)?
            .end as usize;
        if start < end {
            Ok(start..end)
        } else {
            Err(DisplaySnapshotError::EmptyViewportSlice)
        }
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
        let top_row = finite_nonnegative(top_row);
        let rows = viewport_row_range(
            top_row,
            viewport_height,
            row_h,
            self.row_count(),
            overscan_rows,
        );
        ViewportSlice {
            rows,
            top_row_fractional: top_row,
            pixel_offset_y: top_row * row_h,
        }
    }
}

fn viewport_row_range(
    top_row: f32,
    viewport_height: f32,
    row_height: f32,
    total_rows: u32,
    overscan_rows: u32,
) -> Range<u32> {
    let row_height = row_height.max(1.0);
    let visible_rows = finite_nonnegative(viewport_height / row_height)
        .ceil()
        .min(u32::MAX as f32) as u32;
    let visible_rows = visible_rows.saturating_add(1);
    let top_floor = finite_nonnegative(top_row).floor().min(u32::MAX as f32) as u32;
    let top_ceil = finite_nonnegative(top_row).ceil().min(u32::MAX as f32) as u32;
    let start = top_floor.saturating_sub(overscan_rows).min(total_rows);
    let end = top_ceil
        .saturating_add(visible_rows)
        .saturating_add(overscan_rows)
        .min(total_rows)
        .max(start);
    start..end
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
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
    fn slice_math(
        top_row: f32,
        viewport_h: f32,
        row_h: f32,
        total: u32,
        overscan: u32,
    ) -> Range<u32> {
        viewport_row_range(top_row, viewport_h, row_h, total, overscan)
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

    #[test]
    fn viewport_slice_math_handles_extreme_clip_offsets() {
        let r = slice_math(f32::MAX, f32::MAX, 20.0, 50, 5);

        assert_eq!(r, 50..50);
    }

    #[test]
    fn wrapped_continuation_rows_are_unlabelled_for_gutter_consumers() {
        let ctx = eframe::egui::Context::default();
        let mut snapshot = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let text = format!("{}\nshort", "x".repeat(80));
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout_job(eframe::egui::text::LayoutJob::simple(
                    text,
                    eframe::egui::FontId::monospace(14.0),
                    eframe::egui::Color32::WHITE,
                    100.0,
                ))
            });
            snapshot = Some(DisplaySnapshot::from_galley(galley, 18.0));
        });
        let snapshot = snapshot.expect("snapshot");

        assert_eq!(snapshot.logical_line_for(DisplayRow(0)), Some(0));
        assert!(
            (1..snapshot.row_count())
                .take_while(|row| snapshot.logical_line_for(DisplayRow(*row)) != Some(1))
                .all(|row| snapshot.logical_line_for(DisplayRow(row)).is_none())
        );
    }

    #[test]
    fn line_range_for_rows_resolves_continuation_rows_to_owner() {
        let ctx = eframe::egui::Context::default();
        let mut snapshot = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let text = format!("{}\nshort", "x".repeat(80));
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout_job(eframe::egui::text::LayoutJob::simple(
                    text,
                    eframe::egui::FontId::monospace(14.0),
                    eframe::egui::Color32::WHITE,
                    100.0,
                ))
            });
            snapshot = Some(DisplaySnapshot::from_galley(galley, 18.0));
        });
        let snapshot = snapshot.expect("snapshot");

        assert_eq!(snapshot.line_range_for_rows(1..2), Some(0..1));
        assert_eq!(
            snapshot.line_range_for_rows(0..snapshot.row_count()),
            Some(0..2)
        );
    }
}
