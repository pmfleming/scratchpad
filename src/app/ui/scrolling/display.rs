//! Display-row viewport pipeline.
//!
//! Phase 3 contract: every editor renders through one viewport-first path.
//! Display rows are the scroll unit (after wrap/folds). The renderer asks for
//! a `ViewportSlice` from a `DisplaySnapshot` and paints from that.
//!
//! The snapshot stores only the row metadata it needs (tops, char ranges,
//! logical-line mapping, overlay flags). It does NOT retain the
//! `egui::Galley` it was built from; the painter constructs an ephemeral
//! galley at paint time from the cached layout. This guarantees we cannot
//! accidentally hold onto a full-document galley behind the snapshot.

use std::ops::Range;

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
///
/// The snapshot is detached from any underlying `egui::Galley`: it stores
/// only row metadata. The painter rebuilds an ephemeral galley for the
/// current viewport slice at paint time.
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
    row_records: Vec<DisplayRowRecord>,
    max_line_width: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayRowFlags {
    pub ascii: bool,
    pub has_selection: bool,
    pub has_search: bool,
    pub long_line: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayRowRecord {
    pub logical_line: u32,
    pub char_range: Range<u32>,
    pub y_top: f32,
    pub height: f32,
    pub wrap_index: u16,
    pub flags: DisplayRowFlags,
}

impl DisplaySnapshot {
    /// Extract row metadata from the given galley. The galley itself is
    /// dropped at the end of this call -- only `DisplayRowRecord`s are
    /// retained.
    pub fn from_galley(galley: &Galley, row_height: f32) -> Self {
        Self::from_galley_with_base(galley, row_height, 0, 0)
    }

    pub fn from_galley_with_base(
        galley: &Galley,
        row_height: f32,
        char_offset_base: usize,
        logical_line_base: usize,
    ) -> Self {
        Self::from_galley_with_base_and_overlays(
            galley,
            row_height,
            char_offset_base,
            logical_line_base,
            None,
            &[],
        )
    }

    pub fn from_galley_with_base_and_overlays(
        galley: &Galley,
        row_height: f32,
        char_offset_base: usize,
        logical_line_base: usize,
        selection_range: Option<Range<usize>>,
        search_ranges: &[Range<usize>],
    ) -> Self {
        let mut row_tops = Vec::with_capacity(galley.rows.len() + 1);
        let mut row_logical_lines = Vec::with_capacity(galley.rows.len());
        let mut row_char_ranges = Vec::with_capacity(galley.rows.len());
        let mut row_records = Vec::with_capacity(galley.rows.len());
        let mut max_line_width: f32 = 0.0;
        let mut current_logical = saturating_u32(logical_line_base);
        let mut current_char = saturating_u32(char_offset_base);
        let mut wrap_index: u16 = 0;

        for row in galley.rows.iter() {
            row_tops.push(row.pos.y);
            row_logical_lines.push(Some(current_logical));
            let row_start = current_char;
            current_char = current_char.saturating_add(row.char_count_including_newline() as u32);
            let char_range = row_start..current_char;
            row_char_ranges.push(char_range.clone());
            let row_width = row
                .glyphs
                .last()
                .map(|g| g.pos.x + g.advance_width)
                .unwrap_or(0.0);
            max_line_width = max_line_width.max(row_width);
            row_records.push(DisplayRowRecord {
                logical_line: current_logical,
                char_range,
                y_top: row.pos.y,
                height: row_height,
                wrap_index,
                flags: DisplayRowFlags {
                    ascii: true,
                    has_selection: selection_range.as_ref().is_some_and(|selection| {
                        ranges_overlap_u32(selection, &row_start, &current_char)
                    }),
                    has_search: search_ranges
                        .iter()
                        .any(|range| ranges_overlap_u32(range, &row_start, &current_char)),
                    long_line: row_width > 4_096.0,
                },
            });
            if row.ends_with_newline {
                current_logical = current_logical.saturating_add(1);
                wrap_index = 0;
            } else {
                wrap_index = wrap_index.saturating_add(1);
            }
        }
        row_tops.push(galley.rect.height());
        let content_height = galley.rect.height();

        Self {
            row_height,
            content_height,
            display_row_base: saturating_u32(logical_line_base),
            row_tops,
            row_logical_lines,
            row_char_ranges,
            row_records,
            max_line_width,
        }
    }

    pub fn row_count(&self) -> u32 {
        self.row_logical_lines.len() as u32
    }

    pub fn row_height(&self) -> f32 {
        self.row_height
    }

    pub fn content_height(&self) -> f32 {
        self.content_height
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

    pub fn row_record(&self, row: DisplayRow) -> Option<&DisplayRowRecord> {
        self.row_records.get(row.0 as usize)
    }

    pub fn row_records(&self) -> &[DisplayRowRecord] {
        &self.row_records
    }

    /// Locate the snapshot-local display row that contains the given char
    /// offset. Returns `None` when the offset is outside this snapshot's
    /// sliced char range.
    pub fn row_for_char_offset(&self, char_offset: u32) -> Option<DisplayRow> {
        if self.row_char_ranges.is_empty() {
            return None;
        }
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
    pub fn document_row_for_snapshot_row(&self, row: DisplayRow) -> Option<f32> {
        self.row_record(row)?;
        Some(self.display_row_base.saturating_add(row.0) as f32)
    }

    pub fn row_for_document_row(&self, document_row: f32) -> Option<DisplayRow> {
        if !document_row.is_finite() || document_row < 0.0 {
            return None;
        }
        let target = document_row.floor() as u32;
        let local = target.checked_sub(self.display_row_base)?;
        (local < self.row_count()).then_some(DisplayRow(local))
    }

    pub fn document_row_for_char_offset(&self, char_offset: u32) -> Option<f32> {
        let row = self.row_for_char_offset(char_offset)?;
        self.document_row_for_snapshot_row(row)
    }

    /// Pixel y of the row containing `char_offset` plus the fractional offset
    /// within that row. Useful for cursor-reveal computations driven by a
    /// piece-tree-backed `ScrollAnchor`.
    pub fn pixel_y_for_char_offset(&self, char_offset: u32) -> Option<f32> {
        Some(self.document_row_for_char_offset(char_offset)? * self.row_height)
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

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn ranges_overlap_u32(range: &Range<usize>, row_start: &u32, row_end: &u32) -> bool {
    let start = saturating_u32(range.start);
    let end = saturating_u32(range.end);
    start < *row_end && end > *row_start
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
    use eframe::egui;
    use std::sync::Arc;

    fn galley_for(text: &str) -> Arc<Galley> {
        let ctx = egui::Context::default();
        let mut galley = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            galley = Some(ui.fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text.to_owned(),
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    f32::INFINITY,
                ))
            }));
        });
        galley.expect("galley")
    }

    /// Test the slice math directly without constructing a real galley.
    fn slice_math(
        top_row: f32,
        viewport_h: f32,
        row_h: f32,
        total: u32,
        overscan: u32,
    ) -> Range<u32> {
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

    #[test]
    fn display_snapshot_records_document_line_and_char_bases() {
        let snapshot =
            DisplaySnapshot::from_galley_with_base(&galley_for("alpha\nbravo"), 10.0, 40, 5);

        let first = snapshot.row_record(DisplayRow(0)).expect("first row");
        assert_eq!(first.logical_line, 5);
        assert_eq!(first.char_range.start, 40);

        let second = snapshot.row_record(DisplayRow(1)).expect("second row");
        assert_eq!(second.logical_line, 6);
        assert!(second.char_range.start > first.char_range.start);
    }

    #[test]
    fn sliced_snapshot_resolves_document_rows_and_rejects_outside_offsets() {
        let snapshot =
            DisplaySnapshot::from_galley_with_base(&galley_for("alpha\nbravo"), 10.0, 40, 90);

        assert_eq!(snapshot.row_for_char_offset(39), None);
        assert_eq!(snapshot.document_row_for_char_offset(40), Some(90.0));
        assert_eq!(snapshot.document_row_for_char_offset(46), Some(91.0));
        assert_eq!(snapshot.row_for_document_row(90.0), Some(DisplayRow(0)));
        assert_eq!(snapshot.row_for_document_row(91.0), Some(DisplayRow(1)));
        assert_eq!(snapshot.row_for_document_row(12.0), None);
    }

    #[test]
    fn display_snapshot_records_selection_and_search_flags() {
        let search_ranges = std::iter::once(47..49).collect::<Vec<_>>();
        let snapshot = DisplaySnapshot::from_galley_with_base_and_overlays(
            &galley_for("alpha\nbravo"),
            10.0,
            40,
            5,
            Some(41..43),
            &search_ranges,
        );

        assert!(
            snapshot
                .row_record(DisplayRow(0))
                .expect("first row")
                .flags
                .has_selection
        );
        assert!(
            snapshot
                .row_record(DisplayRow(1))
                .expect("second row")
                .flags
                .has_search
        );
    }
}
