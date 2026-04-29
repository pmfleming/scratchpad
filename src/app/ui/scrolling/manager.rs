use super::anchor::ScrollAnchor;
use super::intent::{Axis, ScrollIntent};
use super::metrics::{ContentExtent, ViewportMetrics};
use super::target::ScrollAlign;

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
    /// Pending edge-autoscroll velocity (pixels/sec) from a selection
    /// drag. Applied per-frame until cleared.
    edge_autoscroll_x: f32,
    edge_autoscroll_y: f32,
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

    pub fn set_extent(&mut self, extent: ContentExtent) {
        self.extent = extent;
    }

    /// Total fractional display row at the top of the viewport. The
    /// `anchor_to_row` callback must return the full fractional display row,
    /// including `ScrollAnchor::display_row_offset`.
    pub fn top_display_row(&self, anchor_to_row: impl Fn(ScrollAnchor) -> f32) -> f32 {
        anchor_to_row(self.anchor)
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
                let new_row = (anchor_to_row(self.anchor) + delta_rows).max(0.0);
                self.anchor = row_to_anchor(new_row);
                self.user_scrolled = true;
            }
            ScrollIntent::Pages(n) => {
                let delta_rows = (n as f32) * self.metrics.visible_rows.max(1) as f32;
                let new_row = (anchor_to_row(self.anchor) + delta_rows).max(0.0);
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
        if self.edge_autoscroll_x != 0.0 || self.edge_autoscroll_y != 0.0 {
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
        self.edge_autoscroll_x = 0.0;
        self.edge_autoscroll_y = 0.0;
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
            let cur = anchor_to_row(self.anchor);
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

    fn clamp(
        &mut self,
        anchor_to_row: &dyn Fn(ScrollAnchor) -> f32,
        row_to_anchor: &dyn Fn(f32) -> ScrollAnchor,
    ) {
        let max_y = if self.metrics.row_height > 0.0 {
            self.extent.height.max(0.0) / self.metrics.row_height
        } else {
            self.extent.display_rows as f32
        };
        let row = anchor_to_row(self.anchor).clamp(0.0, max_y.max(0.0));
        self.anchor = row_to_anchor(row);
        if self.anchor.display_row_offset < 0.0 {
            self.anchor.display_row_offset = 0.0;
        }
        let max_x = (self.extent.max_line_width - self.metrics.viewport_rect.width()).max(0.0);
        self.horizontal_px = self.horizontal_px.clamp(0.0, max_x);
    }
}

/// Default approximation for callers that have not yet plumbed a real
/// display-map. Treats every logical line as exactly one display row, ignoring
/// wrap and folds. Use only as a placeholder during incremental wiring.
pub fn naive_anchor_to_row(anchor: ScrollAnchor) -> f32 {
    anchor.logical_line as f32 + anchor.display_row_offset
}

pub fn naive_row_to_anchor(row: f32) -> ScrollAnchor {
    let line = row.max(0.0).floor() as u32;
    let frac = (row - line as f32).max(0.0);
    ScrollAnchor {
        logical_line: line,
        byte_in_line: 0,
        display_row_offset: frac,
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
        assert_eq!(sm.anchor().logical_line, 5);
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
        assert_eq!(sm.anchor().logical_line, 50);
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
        assert_eq!(sm.anchor().logical_line, 2);
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
    fn top_display_row_does_not_double_count_anchor_fraction() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::RestoreAnchor(ScrollAnchor {
                logical_line: 1,
                byte_in_line: 0,
                display_row_offset: 0.5,
            }),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );

        assert_eq!(sm.top_display_row(naive_anchor_to_row), 1.5);
    }

    #[test]
    fn wheel_scroll_preserves_fractional_display_row() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::Wheel {
                delta_x: 0.0,
                delta_y: -30.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );

        assert_eq!(sm.anchor().logical_line, 1);
        assert!((sm.anchor().display_row_offset - 0.5).abs() < 0.001);
        assert_eq!(sm.pixel_offset_y(naive_anchor_to_row), 30.0);
    }

    #[test]
    fn pages_intent_preserves_fractional_display_row() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::RestoreAnchor(ScrollAnchor {
                logical_line: 3,
                byte_in_line: 0,
                display_row_offset: 0.5,
            }),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        sm.apply_intent(
            ScrollIntent::Pages(1),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );

        assert_eq!(sm.anchor().logical_line, 28);
        assert!((sm.anchor().display_row_offset - 0.5).abs() < 0.001);
    }

    // ---- Phase 6b: reveal margins, EOF overscroll, page-nav under wrap ----

    #[test]
    fn pages_intent_uses_visible_rows_not_logical_lines() {
        // Same logical-line count, different visible_rows → different page step.
        let mut a = ScrollManager::new();
        a.set_metrics(metrics(20.0, 25, 500.0));
        a.set_extent(extent(1000, 20.0, 800.0));
        a.apply_intent(
            ScrollIntent::Pages(1),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );

        let mut b = ScrollManager::new();
        b.set_metrics(metrics(20.0, 10, 200.0));
        b.set_extent(extent(1000, 20.0, 800.0));
        b.apply_intent(
            ScrollIntent::Pages(1),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );

        assert_eq!(a.anchor().logical_line, 25);
        assert_eq!(b.anchor().logical_line, 10);
    }

    #[test]
    fn pages_intent_with_zero_visible_rows_advances_at_least_one_row() {
        // Defensive: if metrics report 0 visible rows, page nav must still
        // make forward progress (visible_rows.max(1)).
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 0, 0.0));
        sm.set_extent(extent(100, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::Pages(1),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor().logical_line, 1);
    }

    #[test]
    fn pages_intent_negative_does_not_underflow_at_top() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::Pages(-1),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor(), ScrollAnchor::TOP);
    }

    #[test]
    fn bottom_intent_lands_on_last_display_row() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::Bottom,
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        // Last display row is `display_rows - 1` = 999 in 0-indexed terms.
        assert_eq!(sm.anchor().logical_line, 999);
        assert!(!sm.user_scrolled());
    }

    #[test]
    fn restore_anchor_intent_clears_user_scrolled() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        // Mark user_scrolled via wheel.
        sm.apply_intent(
            ScrollIntent::Wheel {
                delta_x: 0.0,
                delta_y: 40.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert!(sm.user_scrolled());

        let saved = ScrollAnchor {
            logical_line: 42,
            byte_in_line: 0,
            display_row_offset: 0.0,
        };
        sm.apply_intent(
            ScrollIntent::RestoreAnchor(saved),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor(), saved);
        assert!(!sm.user_scrolled());
    }

    #[test]
    fn scrollbar_to_y_maps_pixels_to_anchor_row() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::ScrollbarTo {
                axis: Axis::Y,
                offset_pixels: 200.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        // 200 / 20 = row 10
        assert_eq!(sm.anchor().logical_line, 10);
        assert!(sm.user_scrolled());
    }

    #[test]
    fn scrollbar_to_x_sets_horizontal_pixels() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(10, 20.0, 2_000.0));
        sm.apply_intent(
            ScrollIntent::ScrollbarTo {
                axis: Axis::X,
                offset_pixels: 350.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.horizontal_px(), 350.0);
    }

    #[test]
    fn scrollbar_to_x_clamps_horizontal_to_max_line_width() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        // viewport width 800, content 1000 → max_x = 200
        sm.set_extent(extent(10, 20.0, 1_000.0));
        sm.apply_intent(
            ScrollIntent::ScrollbarTo {
                axis: Axis::X,
                offset_pixels: 9_999.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.horizontal_px(), 200.0);
    }

    #[test]
    fn reveal_with_nearest_margin_does_not_move_when_target_already_visible() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0)); // viewport 0..500
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::ScrollbarTo {
                axis: Axis::Y,
                offset_pixels: 100.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        let before = sm.anchor();
        // Target rect already inside viewport (100..600).
        sm.apply_intent(
            ScrollIntent::Reveal {
                rect: eframe::egui::Rect::from_min_size(pos2(0.0, 250.0), Vec2::new(10.0, 20.0)),
                align_y: super::ScrollAlign::NearestWithMargin(20.0),
                align_x: None,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor(), before);
        assert!(!sm.user_scrolled());
    }

    #[test]
    fn reveal_with_nearest_margin_pulls_target_inside_when_below_viewport() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0)); // viewport 0..500
        sm.set_extent(extent(1000, 20.0, 800.0));
        // Target below the viewport.
        sm.apply_intent(
            ScrollIntent::Reveal {
                rect: eframe::egui::Rect::from_min_size(pos2(0.0, 800.0), Vec2::new(10.0, 20.0)),
                align_y: super::ScrollAlign::NearestWithMargin(0.0),
                align_x: None,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        // Pull target.max (820) to viewport.max → new offset = 820 - 500 = 320 → row 16.
        assert_eq!(sm.anchor().logical_line, 16);
    }

    #[test]
    fn reveal_with_center_align_centers_target_in_viewport() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::Reveal {
                rect: eframe::egui::Rect::from_min_size(pos2(0.0, 1_000.0), Vec2::new(10.0, 20.0)),
                align_y: super::ScrollAlign::Center,
                align_x: None,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        // mid = 1010, new_offset = 1010 - 250 = 760 → row 38
        assert_eq!(sm.anchor().logical_line, 38);
    }

    #[test]
    fn edge_autoscroll_advances_anchor_per_tick() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 800.0));
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::Y,
                velocity: 200.0, // px/sec down
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        // After 1s of ticks, anchor should advance by 200px / 20px = 10 rows.
        sm.tick_edge_autoscroll(1.0, naive_anchor_to_row, naive_row_to_anchor);
        assert_eq!(sm.anchor().logical_line, 10);

        sm.clear_edge_autoscroll();
        sm.tick_edge_autoscroll(1.0, naive_anchor_to_row, naive_row_to_anchor);
        // No further advance after clear.
        assert_eq!(sm.anchor().logical_line, 10);
    }

    #[test]
    fn horizontal_edge_autoscroll_advances_per_tick() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 1_500.0));
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::X,
                velocity: 100.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );

        sm.tick_edge_autoscroll(0.5, naive_anchor_to_row, naive_row_to_anchor);

        assert_eq!(sm.horizontal_px(), 50.0);
    }

    #[test]
    fn horizontal_edge_autoscroll_clamps_to_content_width() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 1_000.0));
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::X,
                velocity: 9_999.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );

        sm.tick_edge_autoscroll(1.0, naive_anchor_to_row, naive_row_to_anchor);

        assert_eq!(sm.horizontal_px(), 200.0);
    }

    #[test]
    fn clear_edge_autoscroll_stops_both_axes() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0));
        sm.set_extent(extent(1000, 20.0, 1_500.0));
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::X,
                velocity: 100.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::Y,
                velocity: 200.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );

        sm.clear_edge_autoscroll();
        sm.tick_edge_autoscroll(1.0, naive_anchor_to_row, naive_row_to_anchor);

        assert_eq!(sm.horizontal_px(), 0.0);
        assert_eq!(sm.anchor(), ScrollAnchor::TOP);
    }

    /// End-to-end acceptance harness — verifies that every input class in the
    /// plan's acceptance criteria ("search jumps, arrow keys, page keys,
    /// mouse wheel, scrollbar drag, and selection edge autoscroll") flows
    /// through the unified `apply_intent` entry point and produces the
    /// expected final state. This is the "single authority" check.
    #[test]
    fn unified_intent_pipeline_routes_every_input_class() {
        let mut sm = ScrollManager::new();
        sm.set_metrics(metrics(20.0, 25, 500.0)); // 25 rows visible, 20 px each
        sm.set_extent(extent(1000, 20.0, 1_500.0)); // wide content for X scrolling

        // 1. Mouse wheel — scroll down 200 px (10 rows).
        sm.apply_intent(
            ScrollIntent::Wheel {
                delta_x: 0.0,
                delta_y: -200.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor().logical_line, 10);
        assert!(sm.user_scrolled());

        // 2. PageDown — Pages(1) advances by visible_rows (25).
        sm.apply_intent(
            ScrollIntent::Pages(1),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor().logical_line, 35);

        // 3. Arrow-down style — Lines(3) advances three rows.
        sm.apply_intent(
            ScrollIntent::Lines(3),
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor().logical_line, 38);

        // 4. Selection edge autoscroll — 100 px/sec for 0.5 s = 50 px = 2.5 rows.
        sm.apply_intent(
            ScrollIntent::EdgeAutoscroll {
                axis: Axis::Y,
                velocity: 100.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        sm.tick_edge_autoscroll(0.5, naive_anchor_to_row, naive_row_to_anchor);
        sm.clear_edge_autoscroll();
        // 38 + 2.5 → row 40 (anchor floors), with .5 frac in display_row_offset.
        assert_eq!(sm.anchor().logical_line, 40);

        // 5. Scrollbar drag (Y) — jump to pixel offset 0 (top).
        sm.apply_intent(
            ScrollIntent::ScrollbarTo {
                axis: Axis::Y,
                offset_pixels: 0.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.anchor().logical_line, 0);

        // 6. Search jump — Reveal a target rect at row 500 with Center align.
        sm.apply_intent(
            ScrollIntent::Reveal {
                rect: eframe::egui::Rect::from_min_size(pos2(0.0, 10_000.0), Vec2::new(10.0, 20.0)),
                align_y: super::ScrollAlign::Center,
                align_x: None,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        // mid = 10010, new_offset = 10010 - 250 = 9760 → row 488.
        assert_eq!(sm.anchor().logical_line, 488);

        // 7. Horizontal scrollbar drag — set horizontal_px directly.
        sm.apply_intent(
            ScrollIntent::ScrollbarTo {
                axis: Axis::X,
                offset_pixels: 250.0,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        assert_eq!(sm.horizontal_px(), 250.0);

        // 8. Top — return to absolute top, clearing user_scrolled.
        sm.apply_intent(ScrollIntent::Top, naive_anchor_to_row, naive_row_to_anchor);
        assert_eq!(sm.anchor().logical_line, 0);
        assert_eq!(sm.anchor().display_row_offset, 0.0);
        assert!(!sm.user_scrolled());
    }
}
