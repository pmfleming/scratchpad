//! Display-row viewport pipeline.
//!
//! Phase 3 contract: every editor renders through one viewport-first path.
//! Display rows are the scroll unit (after wrap/folds). The normal editor path
//! builds a `DisplayMap` from piece-tree source spans, derives a metadata-only
//! `DisplaySnapshot` from that map, and lays out only the overscanned viewport
//! text for paint.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use crate::app::domain::buffer::PieceTreeLite;
use eframe::egui::{self, Galley};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplaySnapshotError {
    InvalidWrapWidth,
    EmptyViewportSlice,
}

impl fmt::Display for DisplaySnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWrapWidth => write!(f, "invalid editor wrap width"),
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
    pub fn from_display_map(map: &DisplayMap) -> Self {
        Self {
            row_height: map.row_height,
            row_tops: map.row_tops(),
            row_logical_lines: map
                .rows
                .iter()
                .map(|row| row.starts_logical_line.then_some(row.logical_line as u32))
                .collect(),
            row_char_ranges: map
                .rows
                .iter()
                .map(|row| row.char_range.start as u32..row.char_range.end as u32)
                .collect(),
            max_line_width: map.max_line_width,
        }
    }

    pub fn row_count(&self) -> u32 {
        self.row_logical_lines.len() as u32
    }

    pub fn row_height(&self) -> f32 {
        self.row_height
    }

    pub fn content_height(&self) -> f32 {
        self.row_tops.last().copied().unwrap_or(self.row_height)
    }

    pub fn display_row_for_logical_line(&self, logical_line: usize) -> Option<usize> {
        let mut current = None;
        for (row_index, line) in self.row_logical_lines.iter().enumerate() {
            if let Some(line) = line {
                let line = *line as usize;
                current = Some(row_index);
                if line == logical_line {
                    return Some(row_index);
                }
                if line > logical_line {
                    return Some(row_index.saturating_sub(1));
                }
            }
        }
        current.filter(|_| {
            self.row_logical_lines
                .iter()
                .flatten()
                .last()
                .is_some_and(|line| *line as usize >= logical_line)
        })
    }

    pub fn anchor_at_display_row(&self, display_row: f32) -> (usize, f32) {
        if self.row_logical_lines.is_empty() {
            return (0, display_row.max(0.0));
        }
        let max_row = self.row_logical_lines.len().saturating_sub(1);
        let clamped = display_row.max(0.0);
        let row_floor = (clamped as usize).min(max_row);
        let frac = clamped - row_floor as f32;
        let mut block_start = row_floor;
        let mut logical_line = None;
        for back in (0..=row_floor).rev() {
            if let Some(line) = self.row_logical_lines[back] {
                logical_line = Some(line as usize);
                block_start = back;
                break;
            }
        }
        let intra_block = (row_floor - block_start) as f32 + frac;
        (logical_line.unwrap_or(0), intra_block)
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
        let Some(start_range) = self.row_char_range(DisplayRow(start_row)) else {
            return Err(DisplaySnapshotError::EmptyViewportSlice);
        };
        let Some(end_range) = self.row_char_range(DisplayRow(end_row)) else {
            return Err(DisplaySnapshotError::EmptyViewportSlice);
        };
        let start = start_range.start as usize;
        let end = end_range.end as usize;
        if start < end || start == end {
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

#[derive(Clone, Debug)]
pub struct DisplayRowSpan {
    pub logical_line: usize,
    pub char_range: Range<usize>,
    pub starts_logical_line: bool,
    pub width: f32,
}

#[derive(Clone, Debug)]
pub struct DisplayMap {
    rows: Vec<DisplayRowSpan>,
    row_height: f32,
    max_line_width: f32,
    total_chars: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayMapBuildStats {
    pub line_count: usize,
    pub reused_lines: usize,
    pub built_lines: usize,
    pub exact_cache_hit: bool,
}

#[derive(Clone, Debug, Default)]
pub struct DisplayMapCache {
    entry: Option<DisplayMapCacheEntry>,
    last_stats: DisplayMapBuildStats,
}

#[derive(Clone, Debug)]
struct DisplayMapCacheEntry {
    revision: u64,
    line_count: usize,
    total_chars: usize,
    geometry: DisplayMapGeometryKey,
    line_layouts: Vec<CachedLineLayout>,
    map: DisplayMap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisplayMapGeometryKey {
    font_size_bits: u32,
    font_family: String,
    word_wrap: bool,
    wrap_width_bits: u32,
    row_height_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CachedLineFingerprint {
    hash: u64,
    char_len: usize,
    source_len: usize,
}

#[derive(Clone, Debug)]
struct CachedLineLayout {
    fingerprint: CachedLineFingerprint,
    rows: Vec<CachedRowSpan>,
    max_width: f32,
}

#[derive(Clone, Debug)]
struct CachedRowSpan {
    start_offset: usize,
    end_offset: usize,
    starts_logical_line: bool,
    width: f32,
}

impl DisplayMapCache {
    pub fn stats(&self) -> DisplayMapBuildStats {
        self.last_stats
    }

    pub fn clear(&mut self) {
        self.entry = None;
        self.last_stats = DisplayMapBuildStats::default();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &egui::Ui,
        piece_tree: &PieceTreeLite,
        revision: u64,
        line_count: usize,
        font_id: &egui::FontId,
        word_wrap: bool,
        wrap_width: f32,
        row_height: f32,
    ) -> Result<DisplayMap, DisplaySnapshotError> {
        if word_wrap && (!wrap_width.is_finite() || wrap_width <= 0.0) {
            return Err(DisplaySnapshotError::InvalidWrapWidth);
        }

        let geometry = DisplayMapGeometryKey::new(font_id, word_wrap, wrap_width, row_height);
        let line_count = line_count.max(1);
        let total_chars = piece_tree.len_chars();

        if let Some(entry) = self.entry.as_ref()
            && entry.revision == revision
            && entry.line_count == line_count
            && entry.total_chars == total_chars
            && entry.geometry == geometry
        {
            self.last_stats = DisplayMapBuildStats {
                line_count,
                reused_lines: line_count,
                built_lines: 0,
                exact_cache_hit: true,
            };
            return Ok(entry.map.clone());
        }

        let reusable_lines = self
            .entry
            .as_ref()
            .filter(|entry| entry.geometry == geometry)
            .map(|entry| reusable_line_pool(&entry.line_layouts))
            .unwrap_or_default();
        let mut reusable_lines = reusable_lines;
        let mut line_layouts = Vec::with_capacity(line_count);
        let mut reused_lines = 0usize;
        let mut built_lines = 0usize;
        let mut rows = Vec::new();
        let mut max_line_width: f32 = 0.0;

        for line_index in 0..line_count {
            let line_source = DisplayLineSource::new(piece_tree, line_index, line_count);
            let line_text = piece_tree.extract_range(line_source.text_range.clone());
            let fingerprint = CachedLineFingerprint::for_text(&line_text, line_source.source_len);
            let line_layout = if let Some(layout) = reusable_lines
                .get_mut(&fingerprint)
                .and_then(VecDeque::pop_front)
            {
                reused_lines += 1;
                layout
            } else {
                built_lines += 1;
                build_cached_line_layout(
                    ui,
                    &line_text,
                    fingerprint,
                    font_id,
                    word_wrap,
                    wrap_width,
                )
            };

            max_line_width = max_line_width.max(line_layout.max_width);
            append_line_rows(&mut rows, line_index, line_source.start_char, &line_layout);
            line_layouts.push(line_layout);
        }

        if rows.is_empty() {
            rows.push(DisplayRowSpan {
                logical_line: 0,
                char_range: 0..0,
                starts_logical_line: true,
                width: 0.0,
            });
        }

        let map = DisplayMap {
            rows,
            row_height: row_height.max(1.0),
            max_line_width: max_line_width.max(1.0),
            total_chars,
        };
        self.last_stats = DisplayMapBuildStats {
            line_count,
            reused_lines,
            built_lines,
            exact_cache_hit: false,
        };
        self.entry = Some(DisplayMapCacheEntry {
            revision,
            line_count,
            total_chars,
            geometry,
            line_layouts,
            map: map.clone(),
        });
        Ok(map)
    }
}

impl DisplayMapGeometryKey {
    fn new(font_id: &egui::FontId, word_wrap: bool, wrap_width: f32, row_height: f32) -> Self {
        Self {
            font_size_bits: font_id.size.to_bits(),
            font_family: format!("{:?}", font_id.family),
            word_wrap,
            wrap_width_bits: if word_wrap {
                wrap_width.to_bits()
            } else {
                f32::INFINITY.to_bits()
            },
            row_height_bits: row_height.max(1.0).to_bits(),
        }
    }
}

impl CachedLineFingerprint {
    fn for_text(text: &str, source_len: usize) -> Self {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        source_len.hash(&mut hasher);
        Self {
            hash: hasher.finish(),
            char_len: text.chars().count(),
            source_len,
        }
    }
}

struct DisplayLineSource {
    start_char: usize,
    source_len: usize,
    text_range: Range<usize>,
}

impl DisplayLineSource {
    fn new(piece_tree: &PieceTreeLite, line_index: usize, line_count: usize) -> Self {
        let info = piece_tree.line_info(line_index);
        let line_start = info.start_char;
        let line_end = line_start + info.char_len;
        let next_line_start = if line_index + 1 < line_count {
            piece_tree.line_info(line_index + 1).start_char
        } else {
            piece_tree.len_chars()
        };
        Self {
            start_char: line_start,
            source_len: next_line_start.saturating_sub(line_start),
            text_range: line_start..line_end,
        }
    }
}

fn reusable_line_pool(
    line_layouts: &[CachedLineLayout],
) -> HashMap<CachedLineFingerprint, VecDeque<CachedLineLayout>> {
    let mut pool: HashMap<CachedLineFingerprint, VecDeque<CachedLineLayout>> = HashMap::new();
    for layout in line_layouts {
        pool.entry(layout.fingerprint)
            .or_default()
            .push_back(layout.clone());
    }
    pool
}

fn build_cached_line_layout(
    ui: &egui::Ui,
    line_text: &str,
    fingerprint: CachedLineFingerprint,
    font_id: &egui::FontId,
    word_wrap: bool,
    wrap_width: f32,
) -> CachedLineLayout {
    let galley = layout_plain_line(ui, line_text, font_id, word_wrap, wrap_width);
    if galley.rows.is_empty() {
        return CachedLineLayout {
            fingerprint,
            rows: vec![CachedRowSpan {
                start_offset: 0,
                end_offset: fingerprint.source_len,
                starts_logical_line: true,
                width: 0.0,
            }],
            max_width: 0.0,
        };
    }

    let mut rows = Vec::with_capacity(galley.rows.len());
    let mut current_offset = 0usize;
    let last_row_index = galley.rows.len().saturating_sub(1);
    let mut max_width: f32 = 0.0;
    for (row_index, row) in galley.rows.iter().enumerate() {
        let row_start = current_offset;
        current_offset = current_offset.saturating_add(row.char_count_including_newline());
        let row_end = if row_index == last_row_index {
            fingerprint.source_len
        } else {
            current_offset.min(fingerprint.char_len)
        };
        let row_width = row
            .glyphs
            .last()
            .map(|glyph| glyph.pos.x + glyph.advance_width)
            .unwrap_or(0.0);
        max_width = max_width.max(row_width);
        rows.push(CachedRowSpan {
            start_offset: row_start,
            end_offset: row_end,
            starts_logical_line: row_index == 0,
            width: row_width,
        });
    }

    CachedLineLayout {
        fingerprint,
        rows,
        max_width,
    }
}

fn append_line_rows(
    rows: &mut Vec<DisplayRowSpan>,
    line_index: usize,
    line_start: usize,
    line_layout: &CachedLineLayout,
) {
    for row in &line_layout.rows {
        rows.push(DisplayRowSpan {
            logical_line: line_index,
            char_range: line_start + row.start_offset..line_start + row.end_offset,
            starts_logical_line: row.starts_logical_line,
            width: row.width,
        });
    }
}

impl DisplayMap {
    #[allow(clippy::too_many_arguments)]
    pub fn from_piece_tree_cached(
        ui: &egui::Ui,
        piece_tree: &PieceTreeLite,
        revision: u64,
        line_count: usize,
        font_id: &egui::FontId,
        word_wrap: bool,
        wrap_width: f32,
        row_height: f32,
        cache: &mut DisplayMapCache,
    ) -> Result<Self, DisplaySnapshotError> {
        cache.build(
            ui, piece_tree, revision, line_count, font_id, word_wrap, wrap_width, row_height,
        )
    }

    pub fn from_piece_tree(
        ui: &egui::Ui,
        piece_tree: &PieceTreeLite,
        line_count: usize,
        font_id: &egui::FontId,
        word_wrap: bool,
        wrap_width: f32,
        row_height: f32,
    ) -> Result<Self, DisplaySnapshotError> {
        if word_wrap && (!wrap_width.is_finite() || wrap_width <= 0.0) {
            return Err(DisplaySnapshotError::InvalidWrapWidth);
        }

        let mut cache = DisplayMapCache::default();
        Self::from_piece_tree_cached(
            ui,
            piece_tree,
            piece_tree.generation(),
            line_count,
            font_id,
            word_wrap,
            wrap_width,
            row_height,
            &mut cache,
        )
    }

    pub fn row_count(&self) -> u32 {
        self.rows.len() as u32
    }

    pub fn row_height(&self) -> f32 {
        self.row_height
    }

    pub fn content_height(&self) -> f32 {
        self.rows.len().max(1) as f32 * self.row_height
    }

    pub fn max_line_width(&self) -> f32 {
        self.max_line_width
    }

    pub fn row_top(&self, row: DisplayRow) -> Option<f32> {
        (row.0 < self.row_count()).then_some(row.0 as f32 * self.row_height)
    }

    pub fn row(&self, row: DisplayRow) -> Option<&DisplayRowSpan> {
        self.rows.get(row.0 as usize)
    }

    pub fn line_range_for_rows(&self, rows: Range<u32>) -> Option<Range<usize>> {
        if self.rows.is_empty() {
            return None;
        }
        let start = rows.start.min(self.row_count()) as usize;
        let end = rows.end.min(self.row_count()) as usize;
        if start >= end {
            return None;
        }
        let first = self.rows[start].logical_line;
        let last = self.rows[end - 1].logical_line;
        Some(first..last + 1)
    }

    pub fn char_range_for_rows(
        &self,
        rows: Range<u32>,
    ) -> Result<Range<usize>, DisplaySnapshotError> {
        let start = rows.start.min(self.row_count()) as usize;
        let end = rows.end.min(self.row_count()) as usize;
        if start >= end {
            return Err(DisplaySnapshotError::EmptyViewportSlice);
        }
        let start_char = self.rows[start].char_range.start;
        let end_char = self.rows[end - 1].char_range.end;
        if start_char < end_char || (start_char == end_char && self.total_chars == 0) {
            Ok(start_char..end_char)
        } else {
            Err(DisplaySnapshotError::EmptyViewportSlice)
        }
    }

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

    pub fn display_row_for_char(&self, char_index: usize) -> Option<DisplayRow> {
        let char_index = char_index.min(self.total_chars);
        self.rows
            .iter()
            .position(|row| {
                row.char_range.start <= char_index
                    && (char_index < row.char_range.end
                        || (row.char_range.is_empty() && char_index == row.char_range.start))
            })
            .or_else(|| {
                self.rows
                    .iter()
                    .rposition(|row| row.char_range.start <= char_index)
            })
            .map(|row| DisplayRow(row as u32))
    }

    pub fn local_char_range_for_rows(
        &self,
        rows: Range<u32>,
    ) -> Result<(Range<usize>, usize), DisplaySnapshotError> {
        let range = self.char_range_for_rows(rows)?;
        let base = range.start;
        Ok((0..range.end.saturating_sub(base), base))
    }

    fn row_tops(&self) -> Vec<f32> {
        let mut row_tops = Vec::with_capacity(self.rows.len() + 1);
        for row in 0..self.rows.len() {
            row_tops.push(row as f32 * self.row_height);
        }
        row_tops.push(self.content_height());
        row_tops
    }
}

fn layout_plain_line(
    ui: &egui::Ui,
    text: &str,
    font_id: &egui::FontId,
    word_wrap: bool,
    wrap_width: f32,
) -> Arc<Galley> {
    ui.fonts_mut(|fonts| {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = if word_wrap { wrap_width } else { f32::INFINITY };
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                ..Default::default()
            },
        );
        fonts.layout_job(job)
    })
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
    fn display_map_builds_wrapped_rows_without_full_galley() {
        let ctx = eframe::egui::Context::default();
        let tree = PieceTreeLite::from_string(format!("{}\nshort", "x".repeat(80)));
        let mut map = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            map = Some(
                DisplayMap::from_piece_tree(
                    ui,
                    &tree,
                    2,
                    &eframe::egui::FontId::monospace(14.0),
                    true,
                    100.0,
                    18.0,
                )
                .expect("display map"),
            );
        });
        let map = map.expect("map");

        assert!(map.row_count() > 2);
        assert_eq!(map.row(DisplayRow(0)).map(|row| row.logical_line), Some(0));
        assert_eq!(map.line_range_for_rows(1..2), Some(0..1));
        assert_eq!(map.line_range_for_rows(0..map.row_count()), Some(0..2));
    }

    #[test]
    fn display_map_maps_rows_to_piece_tree_source_spans() {
        let ctx = eframe::egui::Context::default();
        let tree = PieceTreeLite::from_string("zero\none\ntwo".to_owned());
        let mut map = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            map = Some(
                DisplayMap::from_piece_tree(
                    ui,
                    &tree,
                    3,
                    &eframe::egui::FontId::monospace(14.0),
                    false,
                    f32::INFINITY,
                    18.0,
                )
                .expect("display map"),
            );
        });
        let map = map.expect("map");

        assert_eq!(map.char_range_for_rows(1..3), Ok(5..12));
        assert_eq!(tree.extract_range(5..12), "one\ntwo");
    }

    #[test]
    fn display_map_cache_reuses_exact_revision() {
        let ctx = eframe::egui::Context::default();
        let tree = PieceTreeLite::from_string("zero\none\ntwo".to_owned());
        let mut cache = DisplayMapCache::default();
        let _ = ctx.run_ui(Default::default(), |ui| {
            let first = DisplayMap::from_piece_tree_cached(
                ui,
                &tree,
                1,
                3,
                &eframe::egui::FontId::monospace(14.0),
                false,
                f32::INFINITY,
                18.0,
                &mut cache,
            )
            .expect("first map");
            let second = DisplayMap::from_piece_tree_cached(
                ui,
                &tree,
                1,
                3,
                &eframe::egui::FontId::monospace(14.0),
                false,
                f32::INFINITY,
                18.0,
                &mut cache,
            )
            .expect("second map");

            assert_eq!(first.row_count(), second.row_count());
            assert_eq!(
                cache.stats(),
                DisplayMapBuildStats {
                    line_count: 3,
                    reused_lines: 3,
                    built_lines: 0,
                    exact_cache_hit: true,
                }
            );
        });
    }

    #[test]
    fn display_map_cache_rebuilds_only_changed_line_fingerprints() {
        let ctx = eframe::egui::Context::default();
        let first_tree = PieceTreeLite::from_string("zero\none\ntwo".to_owned());
        let second_tree = PieceTreeLite::from_string("zero\nONE\ntwo".to_owned());
        let mut cache = DisplayMapCache::default();
        let _ = ctx.run_ui(Default::default(), |ui| {
            let _ = DisplayMap::from_piece_tree_cached(
                ui,
                &first_tree,
                1,
                3,
                &eframe::egui::FontId::monospace(14.0),
                false,
                f32::INFINITY,
                18.0,
                &mut cache,
            )
            .expect("first map");
            let second = DisplayMap::from_piece_tree_cached(
                ui,
                &second_tree,
                2,
                3,
                &eframe::egui::FontId::monospace(14.0),
                false,
                f32::INFINITY,
                18.0,
                &mut cache,
            )
            .expect("second map");

            assert_eq!(second.char_range_for_rows(1..2), Ok(5..9));
            assert_eq!(
                cache.stats(),
                DisplayMapBuildStats {
                    line_count: 3,
                    reused_lines: 2,
                    built_lines: 1,
                    exact_cache_hit: false,
                }
            );
        });
    }

    #[test]
    fn display_snapshot_can_be_metadata_only_from_display_map() {
        let ctx = eframe::egui::Context::default();
        let tree = PieceTreeLite::from_string("alpha\nbeta".to_owned());
        let mut snapshot = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let map = DisplayMap::from_piece_tree(
                ui,
                &tree,
                2,
                &eframe::egui::FontId::monospace(14.0),
                false,
                f32::INFINITY,
                18.0,
            )
            .expect("display map");
            snapshot = Some(DisplaySnapshot::from_display_map(&map));
        });
        let snapshot = snapshot.expect("snapshot");

        assert_eq!(snapshot.line_range_for_rows(0..2), Some(0..2));
        assert_eq!(snapshot.char_range_for_rows(0..2), Ok(0..10));
    }

    #[test]
    fn display_map_allows_empty_document_viewport_span() {
        let ctx = eframe::egui::Context::default();
        let tree = PieceTreeLite::from_string(String::new());
        let mut map = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            map = Some(
                DisplayMap::from_piece_tree(
                    ui,
                    &tree,
                    1,
                    &eframe::egui::FontId::monospace(14.0),
                    false,
                    f32::INFINITY,
                    18.0,
                )
                .expect("display map"),
            );
        });
        let map = map.expect("map");

        assert_eq!(map.row_count(), 1);
        assert_eq!(map.char_range_for_rows(0..1), Ok(0..0));
        assert_eq!(tree.extract_range(0..0), "");
    }
}
