use eframe::egui::{Id, Pos2, Ui, Vec2};

use super::target::ScrollTarget;

/// Persistent scroll state stored in `egui::Memory` keyed by a stable `Id`.
///
/// Coordinates are in pixels at this layer. The row-aware layer (Phase 2's
/// `ScrollManager`) translates between display rows / piece-tree anchors and
/// the pixel offset stored here.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollState {
    /// Current scroll offset, in pixels. `(0,0)` is content origin.
    pub offset: Vec2,
    /// Pending programmatic target to reveal on the next frame.
    pub pending_target: Option<ScrollTarget>,
    /// Last observed content size (pixels).
    pub content_size: Vec2,
    /// Last observed inner viewport size (pixels, excluding scrollbars).
    pub viewport_size: Vec2,
    /// Scrollbar drag state (per axis): origin pointer position when drag began,
    /// and the offset value at that moment. `None` when not dragging.
    pub scrollbar_drag: [Option<ScrollbarDragState>; 2],
    /// True if the user has interacted with the scrollbar/wheel since the last
    /// programmatic scroll. Used to suppress automatic snap-back behaviors.
    pub user_scrolled: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ScrollbarDragState {
    pub origin_pointer: Pos2,
    pub origin_offset: f32,
}

impl ScrollState {
    pub fn load(ui: &Ui, id: Id) -> Self {
        ui.ctx()
            .data(|d| d.get_temp::<Self>(id))
            .unwrap_or_default()
    }

    pub fn store(self, ui: &Ui, id: Id) {
        ui.ctx().data_mut(|d| d.insert_temp::<Self>(id, self));
    }

    pub fn request_target(&mut self, target: ScrollTarget) {
        self.pending_target = Some(target);
    }

    /// Maximum permissible offset for the given content/viewport, including
    /// one viewport-height of vertical overscroll past EOF.
    pub fn max_offset(content: Vec2, viewport: Vec2, eof_overscroll: bool) -> Vec2 {
        let extra_y = if eof_overscroll { viewport.y } else { 0.0 };
        Vec2::new(
            (content.x - viewport.x).max(0.0),
            (content.y + extra_y - viewport.y).max(0.0),
        )
    }

    pub fn clamp_offset(&mut self, eof_overscroll: bool) {
        let max = Self::max_offset(self.content_size, self.viewport_size, eof_overscroll);
        self.offset.x = self.offset.x.clamp(0.0, max.x);
        self.offset.y = self.offset.y.clamp(0.0, max.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_offset_includes_one_viewport_of_overscroll_when_enabled() {
        let content = Vec2::new(800.0, 1000.0);
        let viewport = Vec2::new(400.0, 300.0);
        let max = ScrollState::max_offset(content, viewport, true);
        assert_eq!(max.x, 400.0); // horizontal unaffected
        assert_eq!(max.y, 1000.0); // 1000 + 300 - 300 = 1000
    }

    #[test]
    fn max_offset_no_overscroll_when_disabled() {
        let content = Vec2::new(800.0, 1000.0);
        let viewport = Vec2::new(400.0, 300.0);
        let max = ScrollState::max_offset(content, viewport, false);
        assert_eq!(max.y, 700.0);
    }

    #[test]
    fn max_offset_clamps_to_zero_when_content_smaller_than_viewport() {
        let content = Vec2::new(100.0, 100.0);
        let viewport = Vec2::new(400.0, 300.0);
        let max = ScrollState::max_offset(content, viewport, false);
        assert_eq!(max, Vec2::ZERO);
    }

    // ---- Phase 6b: EOF overscroll behavior ----

    #[test]
    fn eof_overscroll_one_full_viewport_height_past_content_end() {
        // With a tall wrapped document (e.g. 2000px content) and a small
        // viewport (300px), enabling EOF overscroll should expose exactly one
        // viewport-height of additional travel beyond the natural max.
        let content = Vec2::new(800.0, 2_000.0);
        let viewport = Vec2::new(400.0, 300.0);
        let max_no = ScrollState::max_offset(content, viewport, false);
        let max_yes = ScrollState::max_offset(content, viewport, true);
        assert_eq!(max_no.y, 1_700.0);
        assert_eq!(max_yes.y - max_no.y, viewport.y);
    }

    #[test]
    fn clamp_offset_keeps_y_inside_overscroll_region() {
        let mut state = ScrollState {
            offset: Vec2::new(0.0, 5_000.0),
            content_size: Vec2::new(800.0, 2_000.0),
            viewport_size: Vec2::new(400.0, 300.0),
            ..ScrollState::default()
        };
        state.clamp_offset(true);
        // max with overscroll = 1700 + 300 = 2000.
        assert_eq!(state.offset.y, 2_000.0);
    }

    #[test]
    fn clamp_offset_disallows_negative_offsets_on_both_axes() {
        let mut state = ScrollState {
            offset: Vec2::new(-50.0, -50.0),
            content_size: Vec2::new(800.0, 2_000.0),
            viewport_size: Vec2::new(400.0, 300.0),
            ..ScrollState::default()
        };
        state.clamp_offset(true);
        assert_eq!(state.offset, Vec2::ZERO);
    }

    #[test]
    fn clamp_offset_caps_x_at_horizontal_max() {
        let mut state = ScrollState {
            offset: Vec2::new(9_999.0, 0.0),
            content_size: Vec2::new(800.0, 2_000.0),
            viewport_size: Vec2::new(400.0, 300.0),
            ..ScrollState::default()
        };
        state.clamp_offset(false);
        assert_eq!(state.offset.x, 400.0); // 800 - 400
    }
}
