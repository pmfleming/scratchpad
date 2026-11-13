//! Display-row viewport pipeline.
//!
//! Phase 3 contract: every editor renders through one viewport-first path.
//! Display rows are the scroll unit (after wrap/folds).
//!
//! The snapshot stores only the row metadata it needs (tops, char ranges,
//! logical-line mapping). It does NOT retain the `egui::Galley` it was built
//! from; the painter constructs an ephemeral galley at paint time from the
//! cached layout. This guarantees we cannot accidentally hold onto a
//! full-document galley behind the snapshot.

use std::ops::Range;

use eframe::egui::Galley;

/// A visual row index after wrap/fold transformations. Distinct type from
/// `LogicalLine` to prevent confusion at call sites.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DisplayRow(pub u32);

/// Snapshot of a buffer's wrap-aware display rows. Owned by a view; rebuilt
/// whenever wrap width, font size, or content changes.
#[derive(Clone)]
pub struct DisplaySnapshot {
    row_height: f32,
    content_height: f32,
    /// Document display row represented by snapshot-local row 0.
    display_row_base: u32,
    /// Row tops in pixels, length = row_count + 1 (last entry = total height).
    row_tops: Vec<f32>,
    /// Logical line number for each display row (for gutter).
    row_logical_lines: Vec<Option<u32>>,
    /// Source char range in the underlying text for each display row.
    row_char_ranges: Vec<Range<u32>>,
}

impl DisplaySnapshot {
    /// Extract row metadata from the given galley. The galley itself is
    /// dropped at the end of this call -- only row metadata is retained.
    #[must_use]
    pub fn from_galley(
        galley: &Galley,
        row_height: f32,
        char_offset_base: usize,
        logical_line_base: usize,
    ) -> Self {
        let mut row_tops = Vec::with_capacity(galley.rows.len() + 1);
        let mut row_logical_lines = Vec::with_capacity(galley.rows.len());
        let mut row_char_ranges = Vec::with_capacity(galley.rows.len());
        let mut current_logical = saturating_u32(logical_line_base);
        let mut current_char = saturating_u32(char_offset_base);
        let y_base = logical_line_base as f32 * row_height;

        for row in &galley.rows {
            row_tops.push(y_base + row.pos.y);
            row_logical_lines.push(Some(current_logical));
            let row_start = current_char;
            current_char = current_char.saturating_add(saturating_u32(usize::from(
                row.char_count_including_newline(),
            )));
            row_char_ranges.push(row_start..current_char);
            if row.ends_with_newline {
                current_logical = current_logical.saturating_add(1);
            }
        }
        let content_height = y_base + galley.rect.height();
        row_tops.push(content_height);

        Self {
            row_height,
            content_height,
            display_row_base: saturating_u32(logical_line_base),
            row_tops,
            row_logical_lines,
            row_char_ranges,
        }
    }

    #[must_use]
    pub fn row_count(&self) -> u32 {
        self.row_logical_lines.len() as u32
    }

    #[must_use]
    pub fn row_height(&self) -> f32 {
        self.row_height
    }

    #[must_use]
    pub fn content_height(&self) -> f32 {
        self.content_height
    }

    #[must_use]
    pub fn logical_line_for(&self, row: DisplayRow) -> Option<u32> {
        self.row_logical_lines
            .get(row.0 as usize)
            .copied()
            .flatten()
    }

    #[must_use]
    pub fn row_top(&self, row: DisplayRow) -> Option<f32> {
        self.row_tops.get(row.0 as usize).copied()
    }

    #[must_use]
    pub fn row_char_range(&self, row: DisplayRow) -> Option<Range<u32>> {
        self.row_char_ranges.get(row.0 as usize).cloned()
    }

    /// Locate the snapshot-local display row that contains the given char
    /// offset. Returns `None` when the offset is outside this snapshot's
    /// sliced char range.
    #[must_use]
    pub fn row_for_char_offset(&self, char_offset: u32) -> Option<DisplayRow> {
        let first = self.row_char_ranges.first()?;
        let last = self.row_char_ranges.last()?;
        if char_offset < first.start || char_offset > last.end {
            return None;
        }
        let position = self
            .row_char_ranges
            .partition_point(|range| range.end <= char_offset);
        let clamped = position.min(self.row_char_ranges.len() - 1);
        Some(DisplayRow(clamped as u32))
    }

    /// Document display row for a snapshot-local row.
    #[must_use]
    pub fn document_row_for_snapshot_row(&self, row: DisplayRow) -> Option<f32> {
        (row.0 < self.row_count()).then(|| self.display_row_base.saturating_add(row.0) as f32)
    }

    #[must_use]
    pub fn row_for_document_row(&self, document_row: f32) -> Option<DisplayRow> {
        if !document_row.is_finite() || document_row < 0.0 {
            return None;
        }
        let target = document_row.floor() as u32;
        let local = target.checked_sub(self.display_row_base)?;
        (local < self.row_count()).then_some(DisplayRow(local))
    }

    #[must_use]
    pub fn document_row_for_char_offset(&self, char_offset: u32) -> Option<f32> {
        let row = self.row_for_char_offset(char_offset)?;
        self.document_row_for_snapshot_row(row)
    }

    /// Pixel y of the row containing `char_offset` plus the fractional offset
    /// within that row. Useful for cursor-reveal computations driven by a
    /// piece-tree-backed `ScrollAnchor`.
    #[must_use]
    pub fn pixel_y_for_char_offset(&self, char_offset: u32) -> Option<f32> {
        Some(self.document_row_for_char_offset(char_offset)? * self.row_height)
    }
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
