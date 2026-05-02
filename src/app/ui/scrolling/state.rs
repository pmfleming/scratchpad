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

    pub fn sanitize(&mut self) {
        self.offset = finite_vec2(self.offset);
        self.content_size = finite_vec2(self.content_size);
        self.viewport_size = finite_vec2(self.viewport_size);
    }

    /// Maximum permissible offset for the given content/viewport, including
    /// one viewport-height of vertical overscroll past EOF.
    pub fn max_offset(content: Vec2, viewport: Vec2, eof_overscroll: bool) -> Vec2 {
        let content = finite_vec2(content);
        let viewport = finite_vec2(viewport);
        let extra_y = if eof_overscroll { viewport.y } else { 0.0 };
        Vec2::new(
            (content.x - viewport.x).max(0.0),
            (content.y + extra_y - viewport.y).max(0.0),
        )
    }

    pub fn clamp_offset(&mut self, eof_overscroll: bool) {
        self.sanitize();
        let max = Self::max_offset(self.content_size, self.viewport_size, eof_overscroll);
        self.offset.x = self.offset.x.clamp(0.0, max.x);
        self.offset.y = self.offset.y.clamp(0.0, max.y);
    }
}

fn finite_axis(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

pub(super) fn finite_vec2(value: Vec2) -> Vec2 {
    Vec2::new(finite_axis(value.x), finite_axis(value.y))
}
