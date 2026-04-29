use eframe::egui::{self, Id, Rect, Sense, Ui, UiBuilder, Vec2, epaint::Shape, pos2, vec2};

use super::source::ScrollSource;
use super::state::{ScrollState, ScrollbarDragState, finite_vec2};
use super::target::ScrollbarPolicy;

#[derive(Clone, Copy)]
pub struct ScrollAreaOutput<R> {
    pub inner: R,
    pub id: Id,
    pub state: ScrollState,
    /// The viewport rect on screen (excluding scrollbar gutters).
    pub inner_rect: Rect,
    pub content_size: Vec2,
    pub did_scroll: bool,
}

/// Local replacement for `egui::ScrollArea` tailored to the editor.
pub struct ScrollArea {
    id: Id,
    source: ScrollSource,
    scrollbar_x: ScrollbarPolicy,
    scrollbar_y: ScrollbarPolicy,
    eof_overscroll: bool,
    scrollbar_thickness: f32,
    min_content_size: Vec2,
    /// Optional fixed outer size; if `None`, fills `ui.available_rect_before_wrap()`.
    max_size: Option<Vec2>,
}

impl ScrollArea {
    pub fn new(id: impl Into<Id>) -> Self {
        Self {
            id: id.into(),
            source: ScrollSource::EDITOR,
            scrollbar_x: ScrollbarPolicy::VisibleWhenNeeded,
            scrollbar_y: ScrollbarPolicy::VisibleWhenNeeded,
            eof_overscroll: true,
            scrollbar_thickness: 8.0,
            min_content_size: Vec2::ZERO,
            max_size: None,
        }
    }

    pub fn source(mut self, source: ScrollSource) -> Self {
        self.source = source;
        self
    }

    pub fn scrollbar_x(mut self, p: ScrollbarPolicy) -> Self {
        self.scrollbar_x = p;
        self
    }

    pub fn scrollbar_y(mut self, p: ScrollbarPolicy) -> Self {
        self.scrollbar_y = p;
        self
    }

    pub fn eof_overscroll(mut self, on: bool) -> Self {
        self.eof_overscroll = on;
        self
    }

    pub fn scrollbar_thickness(mut self, px: f32) -> Self {
        self.scrollbar_thickness = px;
        self
    }

    pub fn min_content_size(mut self, size: Vec2) -> Self {
        self.min_content_size = size;
        self
    }

    pub fn max_size(mut self, size: Vec2) -> Self {
        self.max_size = Some(size);
        self
    }

    /// Render the scroll area. The closure is called with the inner viewport
    /// `Ui`, the current scroll offset (pixels), and the visible viewport rect
    /// in content coordinates. Content size is taken from the inner Ui's
    /// `min_rect()` after the closure returns.
    pub fn show_viewport<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut Ui, Vec2, Rect) -> R,
    ) -> ScrollAreaOutput<R> {
        let mut state = ScrollState::load(ui, self.id);
        state.sanitize();
        state.content_size = content_size_with_minimum(state.content_size, self.min_content_size);

        let outer_rect = scroll_area_outer_rect(ui, self.max_size);

        // Determine which scrollbars will be visible from the previous frame's
        // content/viewport sizes. This causes a one-frame lag on first show
        // when scrollbar visibility flips, matching egui's behavior.
        let bars = visible_scrollbars(
            self.scrollbar_x,
            self.scrollbar_y,
            self.scrollbar_thickness,
            &state,
            outer_rect,
        );
        let inner_rect = inner_rect_for_bars(outer_rect, bars);

        state.viewport_size = inner_rect.size();
        apply_pending_target(&mut state, self.source, inner_rect);

        // Hover gates wheel/scrollbar input.
        let outer_response = ui.interact(outer_rect, self.id.with("__outer"), Sense::hover());

        // Mouse wheel.
        let prev_offset = state.offset;
        apply_mouse_wheel(ui, outer_response.hovered(), self.source, &mut state);

        state.clamp_offset(self.eof_overscroll);

        // Build a child Ui clipped to the inner rect.
        let visible_rect =
            Rect::from_min_size(pos2(state.offset.x, state.offset.y), inner_rect.size());
        let child_size = child_content_rect_size(state.content_size, inner_rect.size());
        let mut content_ui = clipped_content_ui(ui, inner_rect, state.offset, child_size);

        let inner_value = add_contents(&mut content_ui, state.offset, visible_rect);
        // Content size derived from the inner Ui's min_rect, translated back
        // out of the offset space so it represents the absolute extent.
        state.content_size =
            measured_content_size(&content_ui, inner_rect, state.offset, self.min_content_size);

        // Re-clamp after we know the latest content size.
        state.clamp_offset(self.eof_overscroll);

        // Paint scrollbars and handle drag.
        let mut paint_scrollbar =
            |axis: Axis, id_suffix: &str, cross_gap: f32, state: &mut ScrollState| {
                paint_visible_scrollbar(
                    ui,
                    state,
                    ScrollbarPaintRequest {
                        id: self.id.with(id_suffix),
                        outer_rect,
                        axis,
                        thickness: self.scrollbar_thickness,
                        cross_gap,
                        eof_overscroll: self.eof_overscroll,
                        interactive: self.source.scroll_bar,
                    },
                );
            };
        if bars.y {
            paint_scrollbar(Axis::Y, "__bar_y", bars.x_extent, &mut state);
        }
        if bars.x {
            paint_scrollbar(Axis::X, "__bar_x", bars.y_extent, &mut state);
        }

        // Reserve outer rect in parent layout.
        ui.advance_cursor_after_rect(outer_rect);

        let did_scroll = prev_offset != state.offset;
        state.store(ui, self.id);
        let content_size = state.content_size;

        ScrollAreaOutput {
            inner: inner_value,
            id: self.id,
            state,
            inner_rect,
            content_size,
            did_scroll,
        }
    }
}

#[derive(Clone, Copy)]
struct VisibleScrollbars {
    x: bool,
    y: bool,
    x_extent: f32,
    y_extent: f32,
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

impl Axis {
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
        }
    }

    fn extent(self, rect: Rect) -> f32 {
        match self {
            Self::X => rect.width(),
            Self::Y => rect.height(),
        }
    }

    fn pos_in(self, point: egui::Pos2, rect: Rect) -> f32 {
        match self {
            Self::X => point.x - rect.min.x,
            Self::Y => point.y - rect.min.y,
        }
    }

    fn delta(self, current: egui::Pos2, origin: egui::Pos2) -> f32 {
        match self {
            Self::X => current.x - origin.x,
            Self::Y => current.y - origin.y,
        }
    }

    fn thumb_rect(self, bar_rect: Rect, thumb_start: f32, thumb_extent: f32) -> Rect {
        match self {
            Self::X => Rect::from_min_size(
                pos2(bar_rect.min.x + thumb_start, bar_rect.min.y),
                vec2(thumb_extent, bar_rect.height()),
            ),
            Self::Y => Rect::from_min_size(
                pos2(bar_rect.min.x, bar_rect.min.y + thumb_start),
                vec2(bar_rect.width(), thumb_extent),
            ),
        }
    }
}

struct ScrollbarGeometry {
    axis_index: usize,
    max_offset: f32,
    track_extent: f32,
    thumb_extent: f32,
    thumb_rect: Rect,
}

struct ScrollbarPaintRequest {
    id: Id,
    outer_rect: Rect,
    axis: Axis,
    thickness: f32,
    cross_gap: f32,
    eof_overscroll: bool,
    interactive: bool,
}

fn scrollbar_visible(policy: ScrollbarPolicy, content: f32, viewport: f32) -> bool {
    match policy {
        ScrollbarPolicy::AlwaysVisible => true,
        ScrollbarPolicy::Hidden => false,
        ScrollbarPolicy::VisibleWhenNeeded => content > viewport + 0.5,
    }
}

fn scroll_area_outer_rect(ui: &Ui, max_size: Option<Vec2>) -> Rect {
    max_size
        .map(|size| Rect::from_min_size(ui.available_rect_before_wrap().min, size))
        .unwrap_or_else(|| ui.available_rect_before_wrap())
}

fn visible_scrollbars(
    scrollbar_x: ScrollbarPolicy,
    scrollbar_y: ScrollbarPolicy,
    thickness: f32,
    state: &ScrollState,
    outer_rect: Rect,
) -> VisibleScrollbars {
    let x = scrollbar_visible(
        scrollbar_x,
        state.content_size.x,
        state.viewport_size.x.max(outer_rect.width()),
    );
    let y = scrollbar_visible(
        scrollbar_y,
        state.content_size.y,
        state.viewport_size.y.max(outer_rect.height()),
    );
    VisibleScrollbars {
        x,
        y,
        x_extent: if x { thickness } else { 0.0 },
        y_extent: if y { thickness } else { 0.0 },
    }
}

fn inner_rect_for_bars(outer_rect: Rect, bars: VisibleScrollbars) -> Rect {
    Rect::from_min_max(
        outer_rect.min,
        pos2(
            outer_rect.max.x - bars.y_extent,
            outer_rect.max.y - bars.x_extent,
        ),
    )
}

fn apply_pending_target(state: &mut ScrollState, source: ScrollSource, inner_rect: Rect) {
    if !source.programmatic {
        return;
    }
    let Some(target) = state.pending_target.take() else {
        return;
    };
    if let Some(align) = target.align_y {
        state.offset.y = align.resolve(
            egui::Rangef::new(target.rect.min.y, target.rect.max.y),
            inner_rect.height(),
            state.content_size.y,
            state.offset.y,
        );
    }
    if let Some(align) = target.align_x {
        state.offset.x = align.resolve(
            egui::Rangef::new(target.rect.min.x, target.rect.max.x),
            inner_rect.width(),
            state.content_size.x,
            state.offset.x,
        );
    }
    state.user_scrolled = false;
}

fn apply_mouse_wheel(ui: &Ui, hovered: bool, source: ScrollSource, state: &mut ScrollState) {
    if !hovered || !source.mouse_wheel {
        return;
    }
    let scroll = ui.input(|i| i.smooth_scroll_delta);
    if scroll == Vec2::ZERO {
        return;
    }
    state.offset.x -= scroll.x;
    state.offset.y -= scroll.y;
    state.user_scrolled = true;
}

fn clipped_content_ui(ui: &mut Ui, inner_rect: Rect, offset: Vec2, child_size: Vec2) -> Ui {
    let mut content_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_size(inner_rect.min - offset, child_size))
            .layout(*ui.layout()),
    );
    content_ui.set_clip_rect(inner_rect);
    content_ui
}

fn measured_content_size(
    content_ui: &Ui,
    inner_rect: Rect,
    offset: Vec2,
    min_content_size: Vec2,
) -> Vec2 {
    content_size_with_minimum(
        (content_ui.min_rect().max - (inner_rect.min - offset)).max(Vec2::ZERO),
        min_content_size,
    )
}

fn vertical_bar_rect(outer_rect: Rect, thickness: f32, bottom_gap: f32) -> Rect {
    Rect::from_min_max(
        pos2(outer_rect.max.x - thickness, outer_rect.min.y),
        pos2(outer_rect.max.x, outer_rect.max.y - bottom_gap),
    )
}

fn horizontal_bar_rect(outer_rect: Rect, thickness: f32, right_gap: f32) -> Rect {
    Rect::from_min_max(
        pos2(outer_rect.min.x, outer_rect.max.y - thickness),
        pos2(outer_rect.max.x - right_gap, outer_rect.max.y),
    )
}

fn scrollbar_bar_rect(axis: Axis, outer_rect: Rect, thickness: f32, cross_gap: f32) -> Rect {
    match axis {
        Axis::X => horizontal_bar_rect(outer_rect, thickness, cross_gap),
        Axis::Y => vertical_bar_rect(outer_rect, thickness, cross_gap),
    }
}

fn content_size_with_minimum(content_size: Vec2, min_content_size: Vec2) -> Vec2 {
    finite_vec2(content_size).max(finite_vec2(min_content_size))
}

fn child_content_rect_size(content_size: Vec2, viewport_size: Vec2) -> Vec2 {
    finite_vec2(content_size).max(finite_vec2(viewport_size).max(Vec2::splat(1.0)))
}

fn paint_and_handle_scrollbar(
    ui: &mut Ui,
    id: Id,
    bar_rect: Rect,
    axis: Axis,
    state: &mut ScrollState,
    eof_overscroll: bool,
    interactive: bool,
) {
    let Some(geometry) = scrollbar_geometry(bar_rect, axis, state, eof_overscroll) else {
        return;
    };

    let sense = if interactive {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let response = ui.interact(bar_rect, id, sense);

    if interactive {
        handle_scrollbar_drag(ui, &response, bar_rect, axis, state, &geometry);
    }

    let visuals = ui.visuals();
    let track_color = visuals.extreme_bg_color.linear_multiply(0.5);
    let thumb_color = if response.hovered() || response.dragged() {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.widgets.inactive.bg_fill
    };
    ui.painter()
        .add(Shape::rect_filled(bar_rect, 0.0, track_color));
    ui.painter()
        .add(Shape::rect_filled(geometry.thumb_rect, 2.0, thumb_color));
}

fn scrollbar_geometry(
    bar_rect: Rect,
    axis: Axis,
    state: &ScrollState,
    eof_overscroll: bool,
) -> Option<ScrollbarGeometry> {
    let axis_index = axis.index();
    let bar_extent = axis.extent(bar_rect);
    let content = state.content_size[axis_index];
    let viewport = state.viewport_size[axis_index];
    if bar_extent <= 0.0 || content <= 0.0 {
        return None;
    }

    let max_offset =
        ScrollState::max_offset(state.content_size, state.viewport_size, eof_overscroll)
            [axis_index];
    let virtual_content = content + scrollbar_extra_extent(axis, viewport, eof_overscroll);
    let thumb_frac = (viewport / virtual_content).clamp(0.05, 1.0);
    let thumb_extent = (bar_extent * thumb_frac).max(16.0).min(bar_extent);
    let track_extent = (bar_extent - thumb_extent).max(0.0);
    let pos_frac = if max_offset > 0.0 {
        state.offset[axis_index] / max_offset
    } else {
        0.0
    };
    let thumb_rect = axis.thumb_rect(bar_rect, pos_frac * track_extent, thumb_extent);

    Some(ScrollbarGeometry {
        axis_index,
        max_offset,
        track_extent,
        thumb_extent,
        thumb_rect,
    })
}

fn scrollbar_extra_extent(axis: Axis, viewport: f32, eof_overscroll: bool) -> f32 {
    if eof_overscroll && matches!(axis, Axis::Y) {
        viewport
    } else {
        0.0
    }
}

fn handle_scrollbar_drag(
    ui: &Ui,
    response: &egui::Response,
    bar_rect: Rect,
    axis: Axis,
    state: &mut ScrollState,
    geometry: &ScrollbarGeometry,
) {
    let pointer = ui.input(|i| i.pointer.interact_pos());
    if response.drag_started() {
        start_scrollbar_drag(pointer, bar_rect, axis, state, geometry);
    } else if response.dragged() {
        continue_scrollbar_drag(pointer, axis, state, geometry);
    } else {
        state.scrollbar_drag[geometry.axis_index] = None;
    }
}

fn start_scrollbar_drag(
    pointer: Option<egui::Pos2>,
    bar_rect: Rect,
    axis: Axis,
    state: &mut ScrollState,
    geometry: &ScrollbarGeometry,
) {
    let Some(pointer) = pointer else {
        return;
    };
    let initial_offset = if geometry.thumb_rect.contains(pointer) {
        state.offset[geometry.axis_index]
    } else {
        track_offset_for_pointer(pointer, bar_rect, axis, geometry)
    };
    state.scrollbar_drag[geometry.axis_index] = Some(ScrollbarDragState {
        origin_pointer: pointer,
        origin_offset: initial_offset,
    });
    state.offset[geometry.axis_index] = initial_offset;
    state.user_scrolled = true;
}

fn paint_visible_scrollbar(ui: &mut Ui, state: &mut ScrollState, request: ScrollbarPaintRequest) {
    paint_and_handle_scrollbar(
        ui,
        request.id,
        scrollbar_bar_rect(
            request.axis,
            request.outer_rect,
            request.thickness,
            request.cross_gap,
        ),
        request.axis,
        state,
        request.eof_overscroll,
        request.interactive,
    );
}

fn track_offset_for_pointer(
    pointer: egui::Pos2,
    bar_rect: Rect,
    axis: Axis,
    geometry: &ScrollbarGeometry,
) -> f32 {
    let thumb_start = (axis.pos_in(pointer, bar_rect) - geometry.thumb_extent * 0.5)
        .clamp(0.0, geometry.track_extent);
    if geometry.track_extent > 0.0 {
        thumb_start / geometry.track_extent * geometry.max_offset
    } else {
        0.0
    }
}

fn continue_scrollbar_drag(
    pointer: Option<egui::Pos2>,
    axis: Axis,
    state: &mut ScrollState,
    geometry: &ScrollbarGeometry,
) {
    let Some(pointer) = pointer else {
        return;
    };
    let Some(drag) = state.scrollbar_drag[geometry.axis_index] else {
        return;
    };

    let delta_offset = if geometry.track_extent > 0.0 {
        axis.delta(pointer, drag.origin_pointer) / geometry.track_extent * geometry.max_offset
    } else {
        0.0
    };
    state.offset[geometry.axis_index] =
        (drag.origin_offset + delta_offset).clamp(0.0, geometry.max_offset);
    state.user_scrolled = true;
}

#[cfg(test)]
mod tests {
    use super::{child_content_rect_size, content_size_with_minimum};
    use eframe::egui::Vec2;

    #[test]
    fn content_size_respects_virtual_minimum_height() {
        assert_eq!(
            content_size_with_minimum(Vec2::new(200.0, 120.0), Vec2::new(0.0, 800.0)),
            Vec2::new(200.0, 800.0)
        );
    }

    #[test]
    fn content_size_keeps_measured_extent_when_larger_than_minimum() {
        assert_eq!(
            content_size_with_minimum(Vec2::new(640.0, 900.0), Vec2::new(0.0, 800.0)),
            Vec2::new(640.0, 900.0)
        );
    }

    #[test]
    fn content_size_discards_non_finite_values() {
        assert_eq!(
            content_size_with_minimum(Vec2::new(f32::INFINITY, f32::NAN), Vec2::new(0.0, 800.0)),
            Vec2::new(0.0, 800.0)
        );
    }

    #[test]
    fn child_content_rect_size_is_finite_and_at_least_viewport() {
        assert_eq!(
            child_content_rect_size(Vec2::new(f32::INFINITY, f32::NAN), Vec2::new(400.0, 300.0)),
            Vec2::new(400.0, 300.0)
        );
    }
}
