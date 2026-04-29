use super::anchor::ScrollAnchor;
use super::display::DisplaySnapshot;
use super::intent::{Axis, ScrollIntent};
use super::metrics::{ContentExtent, ViewportMetrics};
use super::target::ScrollAlign;
use crate::app::domain::buffer::AnchorId;

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
                self.scroll_pixels(-delta_x, -delta_y, &anchor_to_row, &row_to_anchor);
                self.user_scrolled = true;
            }
            ScrollIntent::ScrollbarTo {
                axis,
                offset_pixels,
            } => match axis {
                Axis::X => {
                    self.horizontal_px = offset_pixels.max(0.0);
                    self.user_scrolled = true;
                }
                Axis::Y => {
                    self.set_pixel_offset_y(offset_pixels, &row_to_anchor);
                    self.user_scrolled = true;
                }
            },
            ScrollIntent::Lines(n) => {
                let delta_rows = n as f32;
                let new_row =
                    (anchor_to_row(self.anchor) + self.anchor.display_row_offset() + delta_rows)
                        .max(0.0);
                self.anchor = row_to_anchor(new_row);
                self.user_scrolled = true;
            }
            ScrollIntent::Pages(n) => {
                let delta_rows = (n as f32) * self.metrics.visible_rows.max(1) as f32;
                let new_row =
                    (anchor_to_row(self.anchor) + self.anchor.display_row_offset() + delta_rows)
                        .max(0.0);
                self.anchor = row_to_anchor(new_row);
                self.user_scrolled = true;
            }
            ScrollIntent::Top => {
                self.anchor = ScrollAnchor::TOP;
                self.user_scrolled = false;
            }
            ScrollIntent::Bottom => {
                let last_row = self.extent.display_rows.saturating_sub(1) as f32;
                self.anchor = row_to_anchor(last_row);
                self.user_scrolled = false;
            }
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
        self.clamp(&row_to_anchor);
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
            self.clamp(&row_to_anchor);
        }
    }

    pub fn clear_edge_autoscroll(&mut self) {
        self.edge_autoscroll_y = 0.0;
        self.edge_autoscroll_x = 0.0;
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
            let cur = anchor_to_row(self.anchor) + self.anchor.display_row_offset();
            let next = (cur + drows).max(0.0);
            self.anchor = row_to_anchor(next);
        }
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
        align_y: ScrollAlign,
        align_x: Option<ScrollAlign>,
        anchor_to_row: &dyn Fn(ScrollAnchor) -> f32,
        row_to_anchor: &dyn Fn(f32) -> ScrollAnchor,
    ) {
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

    fn clamp(&mut self, _row_to_anchor: &dyn Fn(f32) -> ScrollAnchor) {
        // Anchor row clamping is the caller's responsibility — `apply_intent`
        // does it via `row_to_anchor` after each move. Here we only normalize
        // the fractional offset and clamp horizontal pixels.
        if self.anchor.display_row_offset() < 0.0 {
            self.anchor = self.anchor.with_display_row_offset(0.0);
        }
        let max_x = (self.extent.max_line_width - self.metrics.viewport_rect.width()).max(0.0);
        self.horizontal_px = self.horizontal_px.clamp(0.0, max_x);
    }
}

/// Default approximation for callers that have not yet plumbed a real
/// display-map. Treats every logical line as exactly one display row, ignoring
/// wrap and folds. Used as the v1 fallback when no piece-tree-backed anchor
/// is available; piece-tree-backed anchors should be resolved by the renderer
/// using the active `DisplaySnapshot` instead.
pub fn naive_anchor_to_row(anchor: ScrollAnchor) -> f32 {
    match anchor {
        ScrollAnchor::Logical {
            logical_line,
            display_row_offset,
            ..
        } => logical_line as f32 + display_row_offset,
        // For piece-backed anchors, the renderer should provide a display-map
        // closure; fall back to the fractional offset alone if the naive
        // helper is used directly.
        ScrollAnchor::Piece {
            display_row_offset, ..
        } => display_row_offset,
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
/// through the active `DisplaySnapshot`. Falls back to the naive logical
/// mapping when no snapshot is available, when the anchor cannot be located
/// in the piece tree (released anchor), or when the resolved char offset is
/// outside the snapshot's range. Logical anchors always use the naive mapping.
pub fn display_aware_anchor_to_row<'a>(
    snapshot: Option<&'a DisplaySnapshot>,
    resolve_piece: impl Fn(AnchorId) -> Option<usize> + 'a,
) -> impl Fn(ScrollAnchor) -> f32 + 'a {
    move |anchor| match anchor {
        ScrollAnchor::Logical {
            logical_line,
            display_row_offset,
            ..
        } => logical_line as f32 + display_row_offset,
        ScrollAnchor::Piece {
            anchor: id,
            display_row_offset,
        } => {
            let Some(snapshot) = snapshot else {
                return display_row_offset;
            };
            let Some(char_offset) = resolve_piece(id) else {
                return display_row_offset;
            };
            let Some(row) = snapshot.row_for_char_offset(char_offset as u32) else {
                return display_row_offset;
            };
            row.0 as f32 + display_row_offset
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Rect, Vec2, pos2};

    fn metrics(row_height: f32, visible_rows: u32, viewport_h: f32) -> ViewportMetrics {
        ViewportMetrics {
            viewport_rect: Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(800.0, viewport_h)),
            row_height,
            column_width: 8.0,
            visible_rows,
            visible_columns: 80,
        }
    }

    fn extent(rows: u32, row_height: f32, max_line_width: f32) -> ContentExtent {
        ContentExtent {
            display_rows: rows,
            height: rows as f32 * row_height,
            max_line_width,
        }
    }

    #[test]
    fn lines_intent_advances_anchor_by_rows() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::Lines(5),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor().logical_line().unwrap(), 5);
    }

    #[test]
    fn pages_intent_uses_visible_rows() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::Pages(2),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor().logical_line().unwrap(), 50);
    }

    #[test]
    fn top_intent_clears_anchor() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::Lines(50),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        sm.apply_intent(ScrollIntent::Top, naive_anchor_to_row, naive_row_to_anchor);
        assert_eq!(sm.anchor(), ScrollAnchor::TOP);
    }

    #[test]
    fn wheel_marks_user_scrolled() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::Wheel {
                delta_x: 0.0,
                delta_y: -40.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert!(sm.user_scrolled());
        assert_eq!(sm.anchor().logical_line().unwrap(), 2);
    }

    #[test]
    fn horizontal_clamps_to_max_line_width() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(10, 20.0, 600.0));
        sm.apply_intent(
            ScrollIntent::Wheel {
                delta_x: -1000.0,
                delta_y: 0.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        // viewport width 800, content 600 -> max_x = 0
        assert_eq!(sm.horizontal_px(), 0.0);
    }

    #[test]
    fn edge_autoscroll_y_advances_anchor_after_tick() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::Y,
                velocity: 40.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        // Setting velocity alone does not advance the anchor.
        assert_eq!(sm.anchor().logical_line().unwrap(), 0);
        sm.tick_edge_autoscroll(1.0, naive_anchor_to_row, naive_row_to_anchor);
        // 40 px / 20 px-per-row = 2 rows.
        assert_eq!(sm.anchor().logical_line().unwrap(), 2);
    }

    #[test]
    fn edge_autoscroll_x_advances_horizontal_after_tick() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 4000.0));
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::X,
                velocity: 30.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        sm.tick_edge_autoscroll(1.0, naive_anchor_to_row, naive_row_to_anchor);
        assert!((sm.horizontal_px() - 30.0).abs() < 0.001);
    }

    #[test]
    fn clear_edge_autoscroll_zeros_both_axes() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 4000.0));
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::Y,
                velocity: 50.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::X,
                velocity: 25.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        sm.clear_edge_autoscroll();
        // After clearing, a tick should produce no movement.
        let before_row = sm.anchor();
        let before_x = sm.horizontal_px();
        sm.tick_edge_autoscroll(1.0, naive_anchor_to_row, naive_row_to_anchor);
        assert_eq!(sm.anchor(), before_row);
        assert_eq!(sm.horizontal_px(), before_x);
    }
}
