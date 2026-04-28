mod analysis;
mod document;
mod piece_tree;
mod snapshot;
mod state;

pub(crate) use analysis::display_line_count_from_piece_tree;
pub(crate) use analysis::{
    BufferTextMetadata, buffer_text_metadata, buffer_text_metadata_from_piece_tree,
    detected_text_format_and_metadata,
};
pub use analysis::{
    EncodingSource, LineEndingCounts, LineEndingStyle, TextArtifactSummary, TextFormatMetadata,
    analyze_line_endings, display_line_count, platform_default_line_ending,
};
pub use document::TextDocument;
pub(crate) use document::{TextDocumentOperationRecord, TextReplacementError, TextReplacements};
pub use piece_tree::{
    PieceTreeCharPosition, PieceTreeInternalNode, PieceTreeLeaf, PieceTreeLineInfo, PieceTreeLite,
    PieceTreeMetrics, PieceTreeSlice, PieceTreeSpan,
};
pub use snapshot::DocumentSnapshot;
pub use state::{
    BufferFreshness, BufferId, BufferState, BufferViewStatus, DiskFileState, RestoredBufferState,
};

use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BufferLength {
    pub(crate) bytes: usize,
    pub(crate) chars: usize,
    pub(crate) lines: usize,
}

impl BufferLength {
    pub(crate) fn from_metrics(metrics: PieceTreeMetrics, lines: usize) -> Self {
        Self {
            bytes: metrics.bytes,
            chars: metrics.chars,
            lines,
        }
    }
}

#[derive(Clone)]
pub struct RenderedLayout {
    galley: Arc<eframe::egui::Galley>,
    content_height: f32,
    row_tops: Vec<f32>,
    pub row_line_numbers: Vec<Option<usize>>,
    row_char_ranges: Vec<Range<usize>>,
    row_height_bits: Option<u32>,
}

impl RenderedLayout {
    pub fn from_galley(galley: std::sync::Arc<eframe::egui::Galley>) -> Self {
        let content_height = galley.rect.height();
        let row_tops = row_tops_for_galley(&galley);
        let row_line_numbers = row_line_numbers_for_galley(&galley);
        let row_char_ranges = row_char_ranges_for_galley(&galley);
        Self {
            galley,
            content_height,
            row_tops,
            row_line_numbers,
            row_char_ranges,
            row_height_bits: None,
        }
    }

    pub fn visual_row_count(&self) -> usize {
        self.row_line_numbers.len().max(1)
    }

    pub fn row_count(&self) -> usize {
        self.row_char_ranges.len()
    }

    pub fn content_height(&self) -> f32 {
        self.content_height
    }

    pub fn set_row_height(&mut self, row_height: f32) {
        self.row_height_bits = Some(row_height.to_bits());
    }

    pub fn matches_row_height(&self, row_height: f32) -> bool {
        self.row_height_bits == Some(row_height.to_bits())
    }

    pub fn galley(&self) -> &Arc<eframe::egui::Galley> {
        &self.galley
    }

    pub fn row_top(&self, row_index: usize) -> Option<f32> {
        self.row_tops.get(row_index).copied()
    }

    /// Compute the 0-indexed logical line range that owns the given display
    /// row range, using only the layout's own metadata (no buffer access).
    /// Returns `None` if the row range is empty or out of bounds.
    pub fn line_range_for_rows(&self, rows: Range<usize>) -> Option<Range<usize>> {
        if self.row_line_numbers.is_empty() {
            return None;
        }
        let start = rows.start.min(self.row_line_numbers.len());
        let end = rows.end.min(self.row_line_numbers.len());
        if start >= end {
            return None;
        }

        // Walk forward from `start` to find the first 1-indexed line number.
        let first_line = (start..end)
            .find_map(|row| self.row_line_numbers[row])
            .or_else(|| {
                // No labelled rows in the range; fall back to the most recent
                // labelled row at or before `start`.
                (0..=start.min(self.row_line_numbers.len().saturating_sub(1)))
                    .rev()
                    .find_map(|row| self.row_line_numbers[row])
            })?;
        // Walk backward from `end-1` to find the last labelled row that begins
        // a wrapped block, then the line range extends through that line.
        let last_line = (start..end)
            .rev()
            .find_map(|row| self.row_line_numbers[row])
            .unwrap_or(first_line);

        let start_line = first_line.saturating_sub(1);
        let end_line = last_line; // 1-indexed last line → exclusive 0-indexed end.
        Some(start_line..end_line)
    }

    pub fn offset_line_numbers(&mut self, line_offset: usize) {
        if line_offset == 0 {
            return;
        }

        for line_number in &mut self.row_line_numbers {
            if let Some(line_number) = line_number.as_mut() {
                *line_number += line_offset;
            }
        }
    }

    pub fn char_range_for_rows(&self, rows: Range<usize>) -> Option<Range<usize>> {
        if self.row_char_ranges.is_empty() {
            return None;
        }

        let start = rows.start.min(self.row_char_ranges.len());
        let end = rows.end.min(self.row_char_ranges.len());
        if start >= end {
            return None;
        }

        Some(self.row_char_ranges[start].start..self.row_char_ranges[end - 1].end)
    }

    /// First display row that belongs to the given 0-indexed logical line.
    /// Returns `None` if the logical line is past the layout's last row.
    pub fn display_row_for_logical_line(&self, logical_line: usize) -> Option<usize> {
        let target = logical_line.saturating_add(1);
        let mut current = 0usize;
        for (row_index, line_number) in self.row_line_numbers.iter().enumerate() {
            if let Some(num) = *line_number {
                current = num;
                if num == target {
                    return Some(row_index);
                }
                if num > target {
                    return Some(row_index.saturating_sub(1));
                }
            }
        }
        if current >= target {
            Some(self.row_line_numbers.len().saturating_sub(1))
        } else {
            None
        }
    }

    /// 0-indexed logical line that owns the given display row, plus the
    /// fractional offset (in display rows) within that wrapped block.
    pub fn anchor_at_display_row(&self, display_row: f32) -> (usize, f32) {
        if self.row_line_numbers.is_empty() {
            return (0, display_row.max(0.0));
        }
        let max_row = self.row_line_numbers.len().saturating_sub(1);
        let clamped = display_row.max(0.0);
        let row_floor = (clamped as usize).min(max_row);
        let frac = clamped - row_floor as f32;

        // Walk backwards from row_floor to find the start of its wrapped block,
        // and pick up the (1-indexed) logical line number.
        let mut block_start = row_floor;
        let mut line_number = None;
        for back in (0..=row_floor).rev() {
            if let Some(num) = self.row_line_numbers[back] {
                line_number = Some(num);
                block_start = back;
                break;
            }
        }
        let logical_line = line_number.map(|n| n.saturating_sub(1)).unwrap_or(0);
        let intra_block = (row_floor - block_start) as f32 + frac;
        (logical_line, intra_block)
    }
}

fn row_tops_for_galley(galley: &eframe::egui::Galley) -> Vec<f32> {
    galley.rows.iter().map(|row| row.pos.y).collect()
}

fn row_line_numbers_for_galley(galley: &eframe::egui::Galley) -> Vec<Option<usize>> {
    let mut current_line = 1usize;
    let mut starts_new_line = true;
    let mut row_line_numbers = Vec::with_capacity(galley.rows.len());

    for row in &galley.rows {
        row_line_numbers.push(starts_new_line.then_some(current_line));
        starts_new_line = row.ends_with_newline;
        if row.ends_with_newline {
            current_line += 1;
        }
    }

    row_line_numbers
}

fn row_char_ranges_for_galley(galley: &eframe::egui::Galley) -> Vec<Range<usize>> {
    let mut row_char_ranges = Vec::with_capacity(galley.rows.len());
    let mut current_char = 0usize;

    for row in &galley.rows {
        let row_start = current_char;
        current_char += row.char_count_including_newline();
        row_char_ranges.push(row_start..current_char);
    }

    row_char_ranges
}

#[cfg(test)]
mod tests {
    use super::RenderedLayout;
    use eframe::egui;

    fn test_layout(line_count: usize) -> RenderedLayout {
        let ctx = egui::Context::default();
        let mut layout = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let text = (0..line_count)
                .map(|index| format!("line {index}"))
                .collect::<Vec<_>>()
                .join("\n");
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text,
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    400.0,
                ))
            });
            layout = Some(RenderedLayout::from_galley(galley));
        });
        layout.expect("layout should be captured")
    }

    #[test]
    fn line_number_offsets_shift_visible_rows() {
        let mut layout = test_layout(3);

        layout.offset_line_numbers(5);

        assert_eq!(layout.row_line_numbers, vec![Some(6), Some(7), Some(8)]);
    }

    /// Build a layout where the first logical line wraps across multiple
    /// display rows, exercising the `Some(n), None, None, ...` shape that
    /// `line_range_for_rows` must navigate.
    fn wrapped_layout() -> RenderedLayout {
        let ctx = egui::Context::default();
        let mut layout = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            // Two long lines that will wrap, plus two short lines.
            let text = format!("{}\n{}\nshort\nlast", "x".repeat(40), "y".repeat(40));
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text,
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    100.0, // narrow width forces wrapping
                ))
            });
            layout = Some(RenderedLayout::from_galley(galley));
        });
        layout.expect("layout should be captured")
    }

    #[test]
    fn line_range_for_rows_returns_none_for_empty_or_out_of_bounds() {
        // An empty galley still has one row, but ranges that cannot resolve
        // should still return None.
        let layout = test_layout(3);
        // Empty range
        assert!(layout.line_range_for_rows(2..2).is_none());
        // Range entirely past row_count.
        assert!(layout.line_range_for_rows(10..20).is_none());
    }

    #[test]
    fn line_range_for_rows_unwrapped_one_row_per_line() {
        let layout = test_layout(5);
        assert_eq!(layout.line_range_for_rows(0..1), Some(0..1));
        assert_eq!(layout.line_range_for_rows(0..3), Some(0..3));
        assert_eq!(layout.line_range_for_rows(2..5), Some(2..5));
        assert_eq!(layout.line_range_for_rows(0..5), Some(0..5));
    }

    #[test]
    fn line_range_for_rows_handles_wrapped_lines() {
        let layout = wrapped_layout();
        // Sanity: we should have at least 4 display rows but only 4 logical lines,
        // so something must have wrapped.
        assert!(
            layout.row_count() > 4,
            "wrapped layout should produce more rows than logical lines, got {}",
            layout.row_count()
        );
        let labelled_rows: Vec<usize> = layout
            .row_line_numbers
            .iter()
            .enumerate()
            .filter_map(|(idx, n)| n.map(|_| idx))
            .collect();
        assert!(
            labelled_rows.len() >= 4,
            "expected 4 line-start rows, got {labelled_rows:?}"
        );

        // A row range entirely inside the first wrapped block (a continuation
        // row only) must still resolve to the owning logical line 0.
        if labelled_rows[1] > 1 {
            // labelled_rows[1] is the start of line 2 (1-indexed); rows in
            // 1..labelled_rows[1] are continuation rows of line 1.
            let cont_row = 1;
            let range = layout.line_range_for_rows(cont_row..cont_row + 1);
            assert_eq!(range, Some(0..1));
        }

        // A row range covering all rows of the document spans every logical line.
        let full = layout.line_range_for_rows(0..layout.row_count());
        assert_eq!(full, Some(0..4));

        // A row range covering just the last row resolves to the last logical line.
        let last_row = layout.row_count() - 1;
        let last = layout.line_range_for_rows(last_row..last_row + 1);
        assert_eq!(last, Some(3..4));
    }

    #[test]
    fn line_range_for_rows_clamps_overrun_end() {
        let layout = test_layout(3);
        // Asking for more rows than exist should clamp at row_count.
        assert_eq!(layout.line_range_for_rows(0..100), Some(0..3));
    }
}
