use eframe::egui::{self, Id, Rect, Sense, Ui, Vec2, pos2};

use crate::app::ui::{callout, widget_ids};

use super::acceleration::{ScrollAccelerationConfig, accelerated_scroll_delta};
use super::scrollbar::{
    ScrollbarAxis, ScrollbarPaintRequest, inner_rect_for_bars, paint_visible_scrollbar,
    visible_scrollbars,
};
use super::source::ScrollSource;
use super::state::{ScrollState, finite_vec2};
use super::target::ScrollbarPolicy;

const WHEEL_ACCELERATION_CONFIG: ScrollAccelerationConfig = ScrollAccelerationConfig {
    reset_after_seconds: 0.22,
    ramp_per_second: 2.7,
    ramp_per_pixel: 0.004,
    max_multiplier: 3.1,
};

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
    interaction_id: Id,
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
        let id = id.into();
        Self {
            id,
            interaction_id: id,
            source: ScrollSource::EDITOR,
            scrollbar_x: ScrollbarPolicy::VisibleWhenNeeded,
            scrollbar_y: ScrollbarPolicy::VisibleWhenNeeded,
            eof_overscroll: true,
            scrollbar_thickness: 7.0,
            min_content_size: Vec2::ZERO,
            max_size: None,
        }
    }

    pub fn source(mut self, source: ScrollSource) -> Self {
        self.source = source;
        self
    }

    pub fn interaction_id(mut self, id: impl Into<Id>) -> Self {
        self.interaction_id = id.into();
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
        let layout_state = pass_stable_layout_state(ui, self.id, state);
        let bars = visible_scrollbars(
            self.scrollbar_x,
            self.scrollbar_y,
            self.scrollbar_thickness,
            &layout_state,
            outer_rect,
        );
        let inner_rect = inner_rect_for_bars(outer_rect, bars);

        state.viewport_size = inner_rect.size();
        apply_pending_target(&mut state, self.source, inner_rect);

        // Hover gates wheel/scrollbar input.
        let outer_response = widget_ids::interact(
            ui,
            outer_rect,
            self.interaction_id.with("__outer"),
            Sense::hover(),
            "scroll_area_outer",
        );

        // Mouse wheel.
        let prev_offset = state.offset;
        apply_mouse_wheel(
            ui,
            scroll_area_contains_pointer(ui, outer_rect)
                || outer_response.hovered()
                || outer_response.contains_pointer(),
            self.source,
            self.eof_overscroll,
            &mut state,
        );

        state.clamp_offset(self.eof_overscroll);

        // Build a child Ui clipped to the inner rect.
        let visible_rect =
            Rect::from_min_size(pos2(state.offset.x, state.offset.y), inner_rect.size());
        let child_size = child_content_rect_size(state.content_size, inner_rect.size());
        let mut content_ui = clipped_content_ui(
            ui,
            self.id.with("__content_ui"),
            inner_rect,
            state.offset,
            child_size,
        );

        let inner_value = add_contents(&mut content_ui, state.offset, visible_rect);
        // Content size derived from the inner Ui's min_rect, translated back
        // out of the offset space so it represents the absolute extent.
        state.content_size =
            measured_content_size(&content_ui, inner_rect, state.offset, self.min_content_size);

        // Re-clamp after we know the latest content size.
        state.clamp_offset(self.eof_overscroll);

        // Paint scrollbars and handle drag.
        let mut paint_scrollbar =
            |axis: ScrollbarAxis, _id_suffix: &str, cross_gap: f32, state: &mut ScrollState| {
                paint_visible_scrollbar(
                    ui,
                    state,
                    ScrollbarPaintRequest {
                        id: self.id.with(_id_suffix),
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
            paint_scrollbar(ScrollbarAxis::Y, "__bar_y", bars.x_extent, &mut state);
        }
        if bars.x {
            paint_scrollbar(ScrollbarAxis::X, "__bar_x", bars.y_extent, &mut state);
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
struct PassStableScrollLayout {
    frame: u64,
    content_size: Vec2,
    viewport_size: Vec2,
}

fn scroll_area_outer_rect(ui: &Ui, max_size: Option<Vec2>) -> Rect {
    max_size
        .map(|size| Rect::from_min_size(ui.available_rect_before_wrap().min, size))
        .unwrap_or_else(|| ui.available_rect_before_wrap())
}

fn pass_stable_layout_state(ui: &Ui, id: Id, state: ScrollState) -> ScrollState {
    let storage_id = id.with("__pass_stable_layout_state");
    let frame = ui.ctx().cumulative_frame_nr();
    if ui.ctx().current_pass_index() == 0 {
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                storage_id,
                PassStableScrollLayout {
                    frame,
                    content_size: state.content_size,
                    viewport_size: state.viewport_size,
                },
            );
        });
        return state;
    }

    ui.ctx()
        .data(|data| data.get_temp::<PassStableScrollLayout>(storage_id))
        .filter(|layout| layout.frame == frame)
        .map(|layout| ScrollState {
            content_size: layout.content_size,
            viewport_size: layout.viewport_size,
            ..state
        })
        .unwrap_or(state)
}

fn apply_pending_target(state: &mut ScrollState, source: ScrollSource, inner_rect: Rect) {
    if !source.programmatic {
        return;
    }
    let Some(target) = state.pending_target.take() else {
        return;
    };
    state.offset = resolved_target_offset(state, target, inner_rect);
    state.user_scrolled = false;
}

fn resolved_target_offset(
    state: &ScrollState,
    target: super::target::ScrollTarget,
    inner_rect: Rect,
) -> Vec2 {
    Vec2::new(
        resolve_target_axis(
            target.align_x,
            target.rect.min.x..=target.rect.max.x,
            inner_rect.width(),
            state.content_size.x,
            state.offset.x,
        ),
        resolve_target_axis(
            target.align_y,
            target.rect.min.y..=target.rect.max.y,
            inner_rect.height(),
            state.content_size.y,
            state.offset.y,
        ),
    )
}

fn resolve_target_axis(
    align: Option<super::target::ScrollAlign>,
    range: std::ops::RangeInclusive<f32>,
    viewport_size: f32,
    content_size: f32,
    current_offset: f32,
) -> f32 {
    align.map_or(current_offset, |align| {
        align.resolve(
            egui::Rangef::new(*range.start(), *range.end()),
            viewport_size,
            content_size,
            current_offset,
        )
    })
}

fn apply_mouse_wheel(
    ui: &Ui,
    hovered: bool,
    source: ScrollSource,
    eof_overscroll: bool,
    state: &mut ScrollState,
) {
    if !hovered || !source.mouse_wheel || callout::scroll_blocker_active(ui.ctx()) {
        return;
    }
    if ui.input(|input| input.modifiers.shift) {
        return;
    }
    let (scroll, now) = ui.input(|i| (i.smooth_scroll_delta, i.time));
    if scroll == Vec2::ZERO {
        return;
    }
    let scroll = accelerated_wheel_delta(state, now, scroll);
    let previous = state.offset;
    state.offset = wheel_offset(state, scroll, eof_overscroll);
    if previous == state.offset {
        return;
    }
    consume_wheel_axes(ui, previous, state.offset);
    state.user_scrolled = true;
}

fn accelerated_wheel_delta(state: &mut ScrollState, now: f64, scroll: Vec2) -> Vec2 {
    Vec2::new(
        accelerated_scroll_delta(
            &mut state.wheel_acceleration[0],
            now,
            scroll.x,
            WHEEL_ACCELERATION_CONFIG,
        ),
        accelerated_scroll_delta(
            &mut state.wheel_acceleration[1],
            now,
            scroll.y,
            WHEEL_ACCELERATION_CONFIG,
        ),
    )
}

fn wheel_offset(state: &ScrollState, scroll: Vec2, eof_overscroll: bool) -> Vec2 {
    let max_offset =
        ScrollState::max_offset(state.content_size, state.viewport_size, eof_overscroll);
    Vec2::new(
        (state.offset.x - scroll.x).clamp(0.0, max_offset.x),
        (state.offset.y - scroll.y).clamp(0.0, max_offset.y),
    )
}

fn consume_wheel_axes(ui: &Ui, previous: Vec2, current: Vec2) {
    ui.input_mut(|input| {
        if axis_changed(previous.x, current.x) {
            input.smooth_scroll_delta.x = 0.0;
        }
        if axis_changed(previous.y, current.y) {
            input.smooth_scroll_delta.y = 0.0;
        }
    });
}

fn axis_changed(previous: f32, current: f32) -> bool {
    (previous - current).abs() > f32::EPSILON
}

fn scroll_area_contains_pointer(ui: &Ui, rect: Rect) -> bool {
    ui.input(|input| {
        input
            .pointer
            .latest_pos()
            .is_some_and(|pos| rect.contains(pos))
    })
}

fn clipped_content_ui(ui: &mut Ui, id: Id, inner_rect: Rect, offset: Vec2, child_size: Vec2) -> Ui {
    let content_rect = Rect::from_min_size(inner_rect.min - offset, child_size);
    let mut content_ui =
        widget_ids::rect_child_ui(ui, content_rect, ("scroll_content", id), *ui.layout());
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

fn content_size_with_minimum(content_size: Vec2, min_content_size: Vec2) -> Vec2 {
    finite_vec2(content_size).max(finite_vec2(min_content_size))
}

fn child_content_rect_size(content_size: Vec2, viewport_size: Vec2) -> Vec2 {
    finite_vec2(content_size).max(finite_vec2(viewport_size).max(Vec2::splat(1.0)))
}
