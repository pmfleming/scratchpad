use eframe::egui::{self, Id, Rect, Sense, Stroke, Ui, epaint::Shape, pos2, vec2};

use super::acceleration::{ScrollAccelerationConfig, accelerated_scroll_delta};
use super::state::{ScrollState, ScrollbarDragState};
use super::target::ScrollbarPolicy;
use crate::app::ui::widget_ids;

const SCROLLBAR_BUTTON_EXTENT: f32 = 18.0;
const SCROLLBAR_BUTTON_MIN_TRACK_EXTENT: f32 = 48.0;
const SCROLLBAR_BUTTON_BASE_VIEWPORTS_PER_SECOND: f32 = 2.4;
const SCROLLBAR_BUTTON_MIN_PIXELS_PER_SECOND: f32 = 520.0;
const SCROLLBAR_BUTTON_MAX_PIXELS_PER_SECOND: f32 = 2200.0;
const SCROLLBAR_BUTTON_ACCELERATION_CONFIG: ScrollAccelerationConfig = ScrollAccelerationConfig {
    reset_after_seconds: 0.24,
    ramp_per_second: 2.4,
    ramp_per_pixel: 0.001,
    max_multiplier: 4.0,
};

#[derive(Clone, Copy)]
pub(super) struct VisibleScrollbars {
    pub(super) x: bool,
    pub(super) y: bool,
    pub(super) x_extent: f32,
    pub(super) y_extent: f32,
}

#[derive(Clone, Copy)]
pub(super) enum ScrollbarAxis {
    X,
    Y,
}

impl ScrollbarAxis {
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

    fn button_rect(self, bar_rect: Rect, direction: f32, extent: f32) -> Rect {
        match (self, direction.is_sign_negative()) {
            (Self::X, true) => Rect::from_min_size(bar_rect.min, vec2(extent, bar_rect.height())),
            (Self::X, false) => Rect::from_min_size(
                pos2(bar_rect.max.x - extent, bar_rect.min.y),
                vec2(extent, bar_rect.height()),
            ),
            (Self::Y, true) => Rect::from_min_size(bar_rect.min, vec2(bar_rect.width(), extent)),
            (Self::Y, false) => Rect::from_min_size(
                pos2(bar_rect.min.x, bar_rect.max.y - extent),
                vec2(bar_rect.width(), extent),
            ),
        }
    }

    fn track_rect_between_buttons(self, bar_rect: Rect, button_extent: f32) -> Rect {
        match self {
            Self::X => Rect::from_min_max(
                pos2(bar_rect.min.x + button_extent, bar_rect.min.y),
                pos2(bar_rect.max.x - button_extent, bar_rect.max.y),
            ),
            Self::Y => Rect::from_min_max(
                pos2(bar_rect.min.x, bar_rect.min.y + button_extent),
                pos2(bar_rect.max.x, bar_rect.max.y - button_extent),
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

#[derive(Clone, Copy)]
struct ScrollbarTrackRects {
    leading_button: Option<Rect>,
    trailing_button: Option<Rect>,
    track: Rect,
}

pub(super) struct ScrollbarPaintRequest {
    pub(super) id: Id,
    pub(super) outer_rect: Rect,
    pub(super) axis: ScrollbarAxis,
    pub(super) thickness: f32,
    pub(super) cross_gap: f32,
    pub(super) eof_overscroll: bool,
    pub(super) interactive: bool,
}

pub(super) fn visible_scrollbars(
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

pub(super) fn inner_rect_for_bars(outer_rect: Rect, bars: VisibleScrollbars) -> Rect {
    Rect::from_min_max(
        outer_rect.min,
        pos2(
            outer_rect.max.x - bars.y_extent,
            outer_rect.max.y - bars.x_extent,
        ),
    )
}

pub(super) fn paint_visible_scrollbar(
    ui: &mut Ui,
    state: &mut ScrollState,
    request: ScrollbarPaintRequest,
) {
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

fn scrollbar_visible(policy: ScrollbarPolicy, content: f32, viewport: f32) -> bool {
    match policy {
        ScrollbarPolicy::AlwaysVisible => true,
        ScrollbarPolicy::Hidden => false,
        ScrollbarPolicy::VisibleWhenNeeded => content > viewport + 0.5,
    }
}

fn scrollbar_bar_rect(
    axis: ScrollbarAxis,
    outer_rect: Rect,
    thickness: f32,
    cross_gap: f32,
) -> Rect {
    match axis {
        ScrollbarAxis::X => Rect::from_min_max(
            pos2(outer_rect.min.x, outer_rect.max.y - thickness),
            pos2(outer_rect.max.x - cross_gap, outer_rect.max.y),
        ),
        ScrollbarAxis::Y => Rect::from_min_max(
            pos2(outer_rect.max.x - thickness, outer_rect.min.y),
            pos2(outer_rect.max.x, outer_rect.max.y - cross_gap),
        ),
    }
}

fn scrollbar_track_rects(bar_rect: Rect, axis: ScrollbarAxis) -> ScrollbarTrackRects {
    let button_extent = scrollbar_button_extent(bar_rect, axis);
    if button_extent <= 0.0 {
        return ScrollbarTrackRects {
            leading_button: None,
            trailing_button: None,
            track: bar_rect,
        };
    }

    ScrollbarTrackRects {
        leading_button: Some(axis.button_rect(bar_rect, -1.0, button_extent)),
        trailing_button: Some(axis.button_rect(bar_rect, 1.0, button_extent)),
        track: axis.track_rect_between_buttons(bar_rect, button_extent),
    }
}

fn scrollbar_button_extent(bar_rect: Rect, axis: ScrollbarAxis) -> f32 {
    let required = SCROLLBAR_BUTTON_EXTENT * 2.0 + SCROLLBAR_BUTTON_MIN_TRACK_EXTENT;
    if axis.extent(bar_rect) >= required {
        SCROLLBAR_BUTTON_EXTENT
    } else {
        0.0
    }
}

fn paint_and_handle_scrollbar(
    ui: &mut Ui,
    id: Id,
    bar_rect: Rect,
    axis: ScrollbarAxis,
    state: &mut ScrollState,
    eof_overscroll: bool,
    interactive: bool,
) {
    let rects = scrollbar_track_rects(bar_rect, axis);
    let Some(geometry) = scrollbar_geometry(rects.track, axis, state, eof_overscroll) else {
        return;
    };

    let sense = if interactive {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let response = widget_ids::interact(ui, rects.track, id, sense, "scrollbar");

    let mut leading_response = None;
    let mut trailing_response = None;
    if interactive {
        leading_response = handle_scrollbar_step_button(
            ui,
            id.with("__leading_button"),
            rects.leading_button,
            -1.0,
            state,
            &geometry,
        );
        trailing_response = handle_scrollbar_step_button(
            ui,
            id.with("__trailing_button"),
            rects.trailing_button,
            1.0,
            state,
            &geometry,
        );
        handle_scrollbar_drag(ui, &response, rects.track, axis, state, &geometry);
    }

    paint_scrollbar(
        ui,
        axis,
        bar_rect,
        geometry.thumb_rect,
        &response,
        leading_response.as_ref(),
        trailing_response.as_ref(),
        rects,
    );
}

fn paint_scrollbar(
    ui: &Ui,
    axis: ScrollbarAxis,
    bar_rect: Rect,
    thumb_rect: Rect,
    response: &egui::Response,
    leading_response: Option<&egui::Response>,
    trailing_response: Option<&egui::Response>,
    rects: ScrollbarTrackRects,
) {
    let visuals = ui.visuals();
    let track_color = visuals.extreme_bg_color.linear_multiply(0.5);
    let thumb_color = if response.hovered() || response.dragged() {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.widgets.inactive.bg_fill
    };
    ui.painter()
        .add(Shape::rect_filled(bar_rect, 0.0, track_color));
    if let (Some(rect), Some(response)) = (rects.leading_button, leading_response) {
        paint_scrollbar_button(ui, axis, rect, -1.0, response);
    }
    if let (Some(rect), Some(response)) = (rects.trailing_button, trailing_response) {
        paint_scrollbar_button(ui, axis, rect, 1.0, response);
    }
    ui.painter()
        .add(Shape::rect_filled(thumb_rect, 2.0, thumb_color));
}

fn paint_scrollbar_button(
    ui: &Ui,
    axis: ScrollbarAxis,
    rect: Rect,
    direction: f32,
    response: &egui::Response,
) {
    let visuals = ui.visuals();
    let fill = if response.is_pointer_button_down_on() {
        visuals.widgets.active.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.extreme_bg_color.linear_multiply(0.55)
    };
    ui.painter().add(Shape::rect_filled(rect, 0.0, fill));

    let center = rect.center();
    let size = (axis.extent(rect) * 0.28).clamp(3.0, 5.0);
    let points = match (axis, direction.is_sign_negative()) {
        (ScrollbarAxis::Y, true) => [
            pos2(center.x, center.y - size),
            pos2(center.x - size, center.y + size * 0.6),
            pos2(center.x + size, center.y + size * 0.6),
        ],
        (ScrollbarAxis::Y, false) => [
            pos2(center.x, center.y + size),
            pos2(center.x - size, center.y - size * 0.6),
            pos2(center.x + size, center.y - size * 0.6),
        ],
        (ScrollbarAxis::X, true) => [
            pos2(center.x - size, center.y),
            pos2(center.x + size * 0.6, center.y - size),
            pos2(center.x + size * 0.6, center.y + size),
        ],
        (ScrollbarAxis::X, false) => [
            pos2(center.x + size, center.y),
            pos2(center.x - size * 0.6, center.y - size),
            pos2(center.x - size * 0.6, center.y + size),
        ],
    };
    ui.painter().add(Shape::convex_polygon(
        points.to_vec(),
        visuals.widgets.inactive.fg_stroke.color,
        Stroke::NONE,
    ));
}

fn scrollbar_geometry(
    bar_rect: Rect,
    axis: ScrollbarAxis,
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

fn scrollbar_extra_extent(axis: ScrollbarAxis, viewport: f32, eof_overscroll: bool) -> f32 {
    if eof_overscroll && matches!(axis, ScrollbarAxis::Y) {
        viewport
    } else {
        0.0
    }
}

fn handle_scrollbar_drag(
    ui: &Ui,
    response: &egui::Response,
    bar_rect: Rect,
    axis: ScrollbarAxis,
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

fn handle_scrollbar_step_button(
    ui: &Ui,
    id: Id,
    rect: Option<Rect>,
    direction: f32,
    state: &mut ScrollState,
    geometry: &ScrollbarGeometry,
) -> Option<egui::Response> {
    let rect = rect?;
    let response = widget_ids::interact(ui, rect, id, Sense::click(), "scrollbar_button");
    if !response.is_pointer_button_down_on() {
        return Some(response);
    }

    let (now, dt) = ui.input(|input| (input.time, input.stable_dt.clamp(1.0 / 120.0, 0.05)));
    let base_delta = scrollbar_button_base_delta(state, geometry.axis_index, dt) * direction;
    let delta = accelerated_scroll_delta(
        &mut state.scrollbar_button_acceleration[geometry.axis_index],
        now,
        base_delta,
        SCROLLBAR_BUTTON_ACCELERATION_CONFIG,
    );
    let previous = state.offset[geometry.axis_index];
    state.offset[geometry.axis_index] =
        (state.offset[geometry.axis_index] + delta).clamp(0.0, geometry.max_offset);
    if (state.offset[geometry.axis_index] - previous).abs() > f32::EPSILON {
        state.user_scrolled = true;
        ui.ctx().request_repaint();
    }

    Some(response)
}

fn scrollbar_button_base_delta(state: &ScrollState, axis_index: usize, dt: f32) -> f32 {
    (state.viewport_size[axis_index] * SCROLLBAR_BUTTON_BASE_VIEWPORTS_PER_SECOND).clamp(
        SCROLLBAR_BUTTON_MIN_PIXELS_PER_SECOND,
        SCROLLBAR_BUTTON_MAX_PIXELS_PER_SECOND,
    ) * dt
}

fn start_scrollbar_drag(
    pointer: Option<egui::Pos2>,
    bar_rect: Rect,
    axis: ScrollbarAxis,
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

fn track_offset_for_pointer(
    pointer: egui::Pos2,
    bar_rect: Rect,
    axis: ScrollbarAxis,
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
    axis: ScrollbarAxis,
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
