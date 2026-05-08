use super::anchor::ScrollAnchor;
use super::display::DisplaySnapshot;
use super::intent::{Axis, ScrollIntent};
use super::metrics::{ContentExtent, ViewportMetrics};
use super::target::ScrollAlign;
use crate::app::domain::buffer::AnchorId;

#[cfg(test)]
mod tests;

/// Per-view scroll state. One instance per editor view. Owns the single source
/// of truth for scroll position, all input arbitration, and reveal requests.
///
/// The vertical position is stored as fractional display rows (locked v1
/// decision). The horizontal position is stored as pixels.
#[derive(Clone, Debug, Default)]
pub struct ScrollManager {
    /// Top-of-viewport anchor. Stable across edits and resizes.
    anchor: ScrollAnchor,
    /// Horizontal scroll offset, pixels.
    horizontal_px: f32,
    /// Most recent layout metrics, populated each frame by the renderer.
    metrics: ViewportMetrics,
    /// Most recent content extent, populated each frame by the renderer.
    extent: ContentExtent,
    /// True if the user has scrolled since the last reveal/programmatic move.
    /// Suppresses cursor snap-back when reveal margins would overrule a manual
    /// scroll position the user is happy with.
    user_scrolled: bool,
    /// Pending edge-autoscroll velocity (pixels/sec on Y) from a selection
    /// drag. Applied per-frame until cleared.
    edge_autoscroll_y: f32,
    /// Pending edge-autoscroll velocity (pixels/sec on X) from a selection
    /// drag. Applied per-frame until cleared.
    edge_autoscroll_x: f32,
}

impl ScrollManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn anchor(&self) -> ScrollAnchor {
        self.anchor
    }

    pub fn horizontal_px(&self) -> f32 {
        self.horizontal_px
    }

    pub fn metrics(&self) -> ViewportMetrics {
        self.metrics
    }

    pub fn extent(&self) -> ContentExtent {
        self.extent
    }

    pub fn user_scrolled(&self) -> bool {
        self.user_scrolled
    }

    pub fn set_metrics(&mut self, metrics: ViewportMetrics) {
        self.metrics = metrics;
    }

    /// Replace the current anchor wholesale. Used by the renderer when
    /// upgrading from a v1 logical anchor to a piece-tree-backed one.
    pub fn replace_anchor(&mut self, anchor: ScrollAnchor) {
        self.anchor = anchor;
    }

    pub fn set_extent(&mut self, extent: ContentExtent) {
        self.extent = extent;
    }

    /// Total fractional display row at the top of the viewport. Useful for
    /// rendering and for converting to pixel offset for the underlying
    /// `ScrollArea`.
    pub fn top_display_row(&self, anchor_to_row: impl Fn(ScrollAnchor) -> f32) -> f32 {
        anchor_to_row(self.anchor) + self.anchor.display_row_offset()
    }

    /// Convert top-of-viewport display row back into pixel offset for the
    /// pixel-level `ScrollArea`.
    pub fn pixel_offset_y(&self, anchor_to_row: impl Fn(ScrollAnchor) -> f32) -> f32 {
        self.top_display_row(anchor_to_row) * self.metrics.row_height
    }

    /// Apply a scroll intent. The single mutation entry point.
    pub fn apply_intent(
        &mut self,
        intent: ScrollIntent,
        anchor_to_row: impl Fn(ScrollAnchor) -> f32,
        row_to_anchor: impl Fn(f32) -> ScrollAnchor,
    ) {
        match intent {
            ScrollIntent::Wheel { delta_x, delta_y } => {
                self.apply_wheel(delta_x, delta_y, &anchor_to_row, &row_to_anchor)
            }
            ScrollIntent::ScrollbarTo {
                axis,
                offset_pixels,
            } => self.apply_scrollbar(axis, offset_pixels, &row_to_anchor),
            ScrollIntent::Lines(n) => self.scroll_rows(n as f32, &anchor_to_row, &row_to_anchor),
            ScrollIntent::Pages(n) => self.scroll_pages(n, &anchor_to_row, &row_to_anchor),
            ScrollIntent::Top => self.jump_to_top(),
            ScrollIntent::Bottom => self.jump_to_bottom(&row_to_anchor),
            ScrollIntent::Reveal {
                rect,
                align_y,
                align_x,
            } => {
                self.reveal(rect, align_y, align_x, &anchor_to_row, &row_to_anchor);
                self.user_scrolled = false;
            }
            ScrollIntent::RestoreAnchor(anchor) => {
                self.anchor = anchor;
                self.user_scrolled = false;
            }
            ScrollIntent::EdgeAutoscroll { axis, velocity } => match axis {
                Axis::Y => self.edge_autoscroll_y = velocity,
                Axis::X => self.edge_autoscroll_x = velocity,
            },
        }
        self.clamp(&anchor_to_row, &row_to_anchor);
    }

    /// Apply per-frame edge-autoscroll velocity. `dt` is seconds since the last
    /// frame.
    pub fn tick_edge_autoscroll(
        &mut self,
        dt: f32,
        anchor_to_row: impl Fn(ScrollAnchor) -> f32,
        row_to_anchor: impl Fn(f32) -> ScrollAnchor,
    ) {
        if self.edge_autoscroll_y != 0.0 || self.edge_autoscroll_x != 0.0 {
            self.scroll_pixels(
                self.edge_autoscroll_x * dt,
                self.edge_autoscroll_y * dt,
                &anchor_to_row,
                &row_to_anchor,
            );
            self.clamp(&anchor_to_row, &row_to_anchor);
        }
    }

    pub fn clear_edge_autoscroll(&mut self) {
        self.edge_autoscroll_y = 0.0;
        self.edge_autoscroll_x = 0.0;
    }

    fn apply_wheel(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        anchor_to_row: &dyn Fn(ScrollAnchor) -> f32,
        row_to_anchor: &dyn Fn(f32) -> ScrollAnchor,
    ) {
        self.scroll_pixels(-delta_x, -delta_y, anchor_to_row, row_to_anchor);
        self.user_scrolled = true;
    }

    fn apply_scrollbar(
        &mut self,
        axis: Axis,
        offset_pixels: f32,
        row_to_anchor: &dyn Fn(f32) -> ScrollAnchor,
    ) {
        match axis {
            Axis::X => self.horizontal_px = offset_pixels.max(0.0),
            Axis::Y => self.set_pixel_offset_y(offset_pixels, row_to_anchor),
        }
        self.user_scrolled = true;
    }

    fn scroll_rows(
        &mut self,
        delta_rows: f32,
        anchor_to_row: &dyn Fn(ScrollAnchor) -> f32,
        row_to_anchor: &dyn Fn(f32) -> ScrollAnchor,
    ) {
        self.anchor = row_to_anchor(self.offset_row(anchor_to_row, delta_rows));
        self.user_scrolled = true;
    }

    fn scroll_pages(
        &mut self,
        page_count: i32,
        anchor_to_row: &dyn Fn(ScrollAnchor) -> f32,
        row_to_anchor: &dyn Fn(f32) -> ScrollAnchor,
    ) {
        let visible_rows = self.metrics.visible_rows.max(1) as f32;
        self.scroll_rows(
            page_count as f32 * visible_rows,
            anchor_to_row,
            row_to_anchor,
        );
    }

    fn jump_to_top(&mut self) {
        self.anchor = ScrollAnchor::TOP;
        self.user_scrolled = false;
    }

    fn jump_to_bottom(&mut self, row_to_anchor: &dyn Fn(f32) -> ScrollAnchor) {
        let last_row = self.extent.display_rows.saturating_sub(1) as f32;
        self.anchor = row_to_anchor(last_row);
        self.user_scrolled = false;
    }

    fn scroll_pixels(
        &mut self,
        dx: f32,
        dy: f32,
        anchor_to_row: &dyn Fn(ScrollAnchor) -> f32,
        row_to_anchor: &dyn Fn(f32) -> ScrollAnchor,
    ) {
        if dx != 0.0 {
            self.horizontal_px = (self.horizontal_px + dx).max(0.0);
        }
        if dy != 0.0 && self.metrics.row_height > 0.0 {
            let drows = dy / self.metrics.row_height;
            self.anchor = row_to_anchor(self.offset_row(anchor_to_row, drows));
        }
    }

    fn offset_row(&self, anchor_to_row: &dyn Fn(ScrollAnchor) -> f32, delta_rows: f32) -> f32 {
        (anchor_to_row(self.anchor) + self.anchor.display_row_offset() + delta_rows).max(0.0)
    }

    fn set_pixel_offset_y(&mut self, pixels: f32, row_to_anchor: &dyn Fn(f32) -> ScrollAnchor) {
        if self.metrics.row_height <= 0.0 {
            return;
        }
        let row = (pixels / self.metrics.row_height).max(0.0);
        self.anchor = row_to_anchor(row);
    }

    fn reveal(
        &mut self,
        rect: eframe::egui::Rect,
        align_y: Option<ScrollAlign>,
        align_x: Option<ScrollAlign>,
        anchor_to_row: &dyn Fn(ScrollAnchor) -> f32,
        row_to_anchor: &dyn Fn(f32) -> ScrollAnchor,
    ) {
        if let Some(align_y) = align_y {
            let viewport_h = self.metrics.viewport_rect.height();
            let content_h = self.extent.height;
            let cur_y = self.pixel_offset_y(anchor_to_row);
            let new_y = align_y.resolve(
                eframe::egui::Rangef::new(rect.min.y, rect.max.y),
                viewport_h,
                content_h,
                cur_y,
            );
            self.set_pixel_offset_y(new_y, row_to_anchor);
        }

        if let Some(align_x) = align_x {
            let viewport_w = self.metrics.viewport_rect.width();
            let content_w = self.extent.max_line_width;
            self.horizontal_px = align_x.resolve(
                eframe::egui::Rangef::new(rect.min.x, rect.max.x),
                viewport_w,
                content_w,
                self.horizontal_px,
            );
        }
    }

    fn clamp(
        &mut self,
        anchor_to_row: &dyn Fn(ScrollAnchor) -> f32,
        row_to_anchor: &dyn Fn(f32) -> ScrollAnchor,
    ) {
        if self.metrics.row_height > 0.0 && self.extent.height > 0.0 {
            let max_y = (self.extent.height - self.metrics.viewport_rect.height()).max(0.0);
            let current_y = self.pixel_offset_y(anchor_to_row);
            let clamped_y = current_y.clamp(0.0, max_y);
            if clamped_y != current_y {
                self.anchor = row_to_anchor(clamped_y / self.metrics.row_height);
            }
        }
        if self.anchor.display_row_offset() < 0.0 {
            self.anchor = self.anchor.with_display_row_offset(0.0);
        }
        if self.extent.max_line_width > 0.0 {
            let max_x = (self.extent.max_line_width - self.metrics.viewport_rect.width()).max(0.0);
            self.horizontal_px = self.horizontal_px.clamp(0.0, max_x);
        }
    }
}

/// Default approximation for callers that have not yet plumbed a real
/// display-map. Treats every logical line as exactly one display row, ignoring
/// wrap and folds. Returns the anchor's base row only; callers that need the
/// top-of-viewport row add `display_row_offset` exactly once.
pub fn naive_anchor_to_row(anchor: ScrollAnchor) -> f32 {
    match anchor {
        ScrollAnchor::Logical { logical_line, .. } => logical_line as f32,
        // For piece-backed anchors, the renderer should provide a display-map
        // closure; fall back to the top base row if the naive helper is used
        // directly.
        ScrollAnchor::Piece { .. } => 0.0,
    }
}

pub fn naive_row_to_anchor(row: f32) -> ScrollAnchor {
    let line = row.max(0.0).floor() as u32;
    let frac = (row - line as f32).max(0.0);
    ScrollAnchor::Logical {
        logical_line: line,
        byte_in_line: 0,
        display_row_offset: frac,
    }
}

/// Build an `anchor_to_row` closure that resolves piece-tree-backed anchors
/// through the active `DisplaySnapshot`. Returns the anchor's base row only;
/// callers that need the top-of-viewport row add `display_row_offset` exactly
/// once. Falls back to the top base row when a piece anchor cannot be resolved.
pub fn display_aware_anchor_to_row<'a>(
    snapshot: Option<&'a DisplaySnapshot>,
    resolve_piece: impl Fn(AnchorId) -> Option<usize> + 'a,
) -> impl Fn(ScrollAnchor) -> f32 + 'a {
    move |anchor| match anchor {
        ScrollAnchor::Logical { logical_line, .. } => logical_line as f32,
        ScrollAnchor::Piece { anchor: id, .. } => {
            let Some(snapshot) = snapshot else {
                return 0.0;
            };
            let Some(char_offset) = resolve_piece(id) else {
                return 0.0;
            };
            let Some(row) = snapshot.document_row_for_char_offset(char_offset as u32) else {
                return 0.0;
            };
            row
        }
    }
}
