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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleWindowLayoutKey {
    pub wrap_width_bits: u32,
    pub font_size_bits: u32,
    pub dark_mode: bool,
    pub text_color: eframe::egui::Color32,
    pub highlight_background: eframe::egui::Color32,
    pub highlight_text: eframe::egui::Color32,
    pub selection_range: Option<Range<usize>>,
    pub search_highlight_signature: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedTextWindow {
    pub row_range: Range<usize>,
    pub line_range: Range<usize>,
    pub char_range: Range<usize>,
    pub layout_row_offset: usize,
    pub text: String,
    pub truncated_start: bool,
    pub truncated_end: bool,
}

#[derive(Clone)]
pub struct RenderedLayout {
    galley: Arc<eframe::egui::Galley>,
    content_height: f32,
    row_tops: Vec<f32>,
    pub row_line_numbers: Vec<Option<usize>>,
    row_char_ranges: Vec<Range<usize>>,
    row_height_bits: Option<u32>,
    pub visible_text: Option<RenderedTextWindow>,
    visible_window_layout_key: Option<VisibleWindowLayoutKey>,
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
            visible_text: None,
            visible_window_layout_key: None,
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

    pub fn visible_row_range(&self) -> Range<usize> {
        self.visible_text
            .as_ref()
            .map(|window| window.row_range.clone())
            .unwrap_or(0..self.row_count())
    }

    pub fn visible_line_range(&self) -> Range<usize> {
        self.visible_text
            .as_ref()
            .map(|window| window.line_range.clone())
            .unwrap_or_else(|| {
                let line_count = self
                    .row_line_numbers
                    .iter()
                    .flatten()
                    .copied()
                    .next_back()
                    .unwrap_or(1);
                0..line_count
            })
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

    pub fn set_visible_text(&mut self, visible_text: RenderedTextWindow) {
        self.visible_window_layout_key = None;
        self.visible_text = Some(visible_text);
    }

    pub fn set_visible_text_with_cache_key(
        &mut self,
        visible_text: RenderedTextWindow,
        cache_key: VisibleWindowLayoutKey,
    ) {
        self.visible_text = Some(visible_text);
        self.visible_window_layout_key = Some(cache_key);
    }

    pub fn matches_visible_window_layout(
        &self,
        visible_text: &RenderedTextWindow,
        cache_key: &VisibleWindowLayoutKey,
    ) -> bool {
        self.visible_text
            .as_ref()
            .is_some_and(|cached| visible_window_matches(cached, visible_text))
            && self.visible_window_layout_key.as_ref() == Some(cache_key)
    }
}

fn visible_window_matches(cached: &RenderedTextWindow, current: &RenderedTextWindow) -> bool {
    cached.line_range == current.line_range
        && cached.char_range == current.char_range
        && cached.layout_row_offset == current.layout_row_offset
        && cached.text == current.text
        && cached.truncated_start == current.truncated_start
        && cached.truncated_end == current.truncated_end
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
    use super::{RenderedLayout, RenderedTextWindow, VisibleWindowLayoutKey};
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
    fn visible_ranges_default_to_full_layout() {
        let layout = test_layout(4);

        assert_eq!(layout.visible_row_range(), 0..4);
        assert_eq!(layout.visible_line_range(), 0..4);
    }

    #[test]
    fn visible_ranges_follow_visible_text_window() {
        let mut layout = test_layout(5);
        layout.set_visible_text(RenderedTextWindow {
            row_range: 1..4,
            line_range: 1..4,
            char_range: 7..28,
            layout_row_offset: 0,
            text: "line 1\nline 2\nline 3".to_owned(),
            truncated_start: true,
            truncated_end: true,
        });

        assert_eq!(layout.visible_row_range(), 1..4);
        assert_eq!(layout.visible_line_range(), 1..4);
    }

    #[test]
    fn visible_window_layout_match_ignores_layout_row_range() {
        let mut layout = test_layout(3);
        let cache_key = VisibleWindowLayoutKey {
            wrap_width_bits: 0.0f32.to_bits(),
            font_size_bits: 14.0f32.to_bits(),
            dark_mode: true,
            text_color: egui::Color32::WHITE,
            highlight_background: egui::Color32::LIGHT_BLUE,
            highlight_text: egui::Color32::BLACK,
            selection_range: Some(0..4),
            search_highlight_signature: 17,
        };

        layout.set_visible_text_with_cache_key(
            RenderedTextWindow {
                row_range: 0..5,
                line_range: 40..43,
                char_range: 100..118,
                layout_row_offset: 40,
                text: "line 40\nline 41\nline 42\n".to_owned(),
                truncated_start: true,
                truncated_end: true,
            },
            cache_key.clone(),
        );

        assert!(layout.matches_visible_window_layout(
            &RenderedTextWindow {
                row_range: 0..0,
                line_range: 40..43,
                char_range: 100..118,
                layout_row_offset: 40,
                text: "line 40\nline 41\nline 42\n".to_owned(),
                truncated_start: true,
                truncated_end: true,
            },
            &cache_key,
        ));
    }

    #[test]
    fn line_number_offsets_shift_visible_rows() {
        let mut layout = test_layout(3);

        layout.offset_line_numbers(5);

        assert_eq!(layout.row_line_numbers, vec![Some(6), Some(7), Some(8)]);
    }
}
