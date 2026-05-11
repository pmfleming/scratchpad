use eframe::egui::{self, Id, Rect, Sense, Ui, epaint::Shape, pos2, vec2};

use super::state::{ScrollState, ScrollbarDragState};
use super::target::ScrollbarPolicy;
use crate::app::ui::widget_ids;

const SCROLLBAR_DORMANT_THICKNESS: f32 = 2.0;
const SCROLLBAR_ACTIVE_BACKGROUND_OPACITY: f32 = 0.7;
const SCROLLBAR_DORMANT_HANDLE_OPACITY: f32 = 0.6;

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

    fn thin_thumb_rect(self, thumb_rect: Rect, thickness: f32) -> Rect {
        match self {
            Self::X => Rect::from_center_size(
                thumb_rect.center(),
                vec2(thumb_rect.width(), thickness.min(thumb_rect.height())),
            ),
            Self::Y => Rect::from_center_size(
                thumb_rect.center(),
                vec2(thickness.min(thumb_rect.width()), thumb_rect.height()),
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

fn paint_and_handle_scrollbar(
    ui: &mut Ui,
    id: Id,
    bar_rect: Rect,
    axis: ScrollbarAxis,
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
    let response = widget_ids::interact(ui, bar_rect, id, sense, "scrollbar");

    if interactive {
        handle_scrollbar_drag(ui, &response, bar_rect, axis, state, &geometry);
    }

    paint_scrollbar(ui, axis, bar_rect, geometry.thumb_rect, &response);
}

fn paint_scrollbar(
    ui: &Ui,
    axis: ScrollbarAxis,
    bar_rect: Rect,
    thumb_rect: Rect,
    response: &egui::Response,
) {
    let visuals = ui.visuals();
    let interactive = response.hovered() || response.dragged();
    let widget_visuals = if response.is_pointer_button_down_on() {
        &visuals.widgets.active
    } else if interactive {
        &visuals.widgets.hovered
    } else {
        &visuals.widgets.inactive
    };
    let track_color = visuals.extreme_bg_color.gamma_multiply(if interactive {
        SCROLLBAR_ACTIVE_BACKGROUND_OPACITY
    } else {
        0.0
    });
    let thumb_color = widget_visuals
        .fg_stroke
        .color
        .gamma_multiply(if interactive {
            1.0
        } else {
            SCROLLBAR_DORMANT_HANDLE_OPACITY
        });
    let visual_thumb_rect = if interactive {
        thumb_rect
    } else {
        axis.thin_thumb_rect(thumb_rect, SCROLLBAR_DORMANT_THICKNESS)
    };

    ui.painter().add(Shape::rect_filled(
        bar_rect,
        widget_visuals.corner_radius,
        track_color,
    ));
    ui.painter().add(Shape::rect_filled(
        visual_thumb_rect,
        widget_visuals.corner_radius,
        thumb_color,
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
