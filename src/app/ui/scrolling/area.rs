use eframe::egui::{self, Id, Rect, Sense, Ui, Vec2, pos2};
use std::time::Instant;

use crate::app::capacity_metrics::{FramePhase, record_frame_phase};
use crate::app::ui::{callout, widget_ids};

use super::acceleration::{ScrollAccelerationConfig, accelerated_scroll_delta};
use super::scrollbar::{
    ScrollbarAxis, ScrollbarPaintRequest, inner_rect_for_bars, paint_visible_scrollbar,
    visible_scrollbars,
};
use super::source::ScrollSource;
use super::state::{ScrollState, finite_vec2};
use super::target::ScrollbarPolicy;

const SCROLLBAR_THICKNESS: f32 = 7.0;

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
    reserve_vertical_gutter: bool,
    min_content_size: Vec2,
}

impl ScrollArea {
    pub fn new(id: impl Into<Id>) -> Self {
        let id = id.into();
        Self {
            id,
            interaction_id: id,
            source: ScrollSource::ALL,
            scrollbar_x: ScrollbarPolicy::VisibleWhenNeeded,
            scrollbar_y: ScrollbarPolicy::VisibleWhenNeeded,
            eof_overscroll: true,
            reserve_vertical_gutter: false,
            min_content_size: Vec2::ZERO,
        }
    }

    #[must_use]
    pub fn source(mut self, source: ScrollSource) -> Self {
        self.source = source;
        self
    }

    #[must_use]
    pub fn interaction_id(mut self, id: impl Into<Id>) -> Self {
        self.interaction_id = id.into();
        self
    }

    #[must_use]
    pub fn scrollbar_x(mut self, p: ScrollbarPolicy) -> Self {
        self.scrollbar_x = p;
        self
    }

    #[must_use]
    pub fn scrollbar_y(mut self, p: ScrollbarPolicy) -> Self {
        self.scrollbar_y = p;
        self
    }

    #[must_use]
    pub fn eof_overscroll(mut self, on: bool) -> Self {
        self.eof_overscroll = on;
        self
    }

    /// Keep text wrapping stable when the vertical scrollbar appears/disappears.
    /// Visibility and interaction still follow the scrollbar policy.
    #[must_use]
    pub fn reserve_vertical_gutter(mut self, reserve: bool) -> Self {
        self.reserve_vertical_gutter = reserve;
        self
    }

    #[must_use]
    pub fn min_content_size(mut self, size: Vec2) -> Self {
        self.min_content_size = size;
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
        let mut scroll_phase_started_at = Instant::now();
        let mut state = ScrollState::load(ui, self.id);
        state.sanitize();
        state.content_size = content_size_with_minimum(state.content_size, self.min_content_size);
        let outer_rect = ui.available_rect_before_wrap();

        // Determine which scrollbars will be visible from the previous frame's
        // content/viewport sizes. This causes a one-frame lag on first show
        // when scrollbar visibility flips, matching egui's behavior.
        let layout_state = pass_stable_layout_state(ui, self.id, state);
        let mut bars = visible_scrollbars(
            self.scrollbar_x,
            self.scrollbar_y,
            SCROLLBAR_THICKNESS,
            &layout_state,
            outer_rect,
        );
        if self.reserve_vertical_gutter {
            bars.y_extent = SCROLLBAR_THICKNESS;
        }
        let inner_rect = inner_rect_for_bars(outer_rect, bars);

        state.viewport_size = inner_rect.size();

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
        record_frame_phase(FramePhase::Scroll, scroll_phase_started_at.elapsed());

        let inner_value = add_contents(&mut content_ui, state.offset, visible_rect);
        scroll_phase_started_at = Instant::now();
        // Content size derived from the inner Ui's min_rect, translated back
        // out of the offset space so it represents the absolute extent.
        state.content_size =
            measured_content_size(&content_ui, inner_rect, state.offset, self.min_content_size);

        // Re-clamp after we know the latest content size.
        state.clamp_offset(self.eof_overscroll);

        // Paint scrollbars and handle drag.
        let mut paint_scrollbar =
            |axis: ScrollbarAxis, id_suffix: &str, cross_gap: f32, state: &mut ScrollState| {
                paint_visible_scrollbar(
                    ui,
                    state,
                    ScrollbarPaintRequest {
                        id: self.id.with(id_suffix),
                        outer_rect,
                        axis,
                        thickness: SCROLLBAR_THICKNESS,
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
        record_frame_phase(FramePhase::Scroll, scroll_phase_started_at.elapsed());

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
        .map_or(state, |layout| ScrollState {
            content_size: layout.content_size,
            viewport_size: layout.viewport_size,
            ..state
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
    let (scroll, now, wheel_source) =
        ui.input(|i| (i.smooth_scroll_delta, i.time, i.scroll_source()));
    if scroll == Vec2::ZERO {
        return;
    }
    let scroll = accelerated_wheel_delta(
        state,
        now,
        scroll,
        wheel_source.unwrap_or(egui::MouseWheelSource::Unknown),
    );
    let previous = state.offset;
    state.offset = wheel_offset(state, scroll, eof_overscroll);
    if previous == state.offset {
        return;
    }
    consume_wheel_axes(ui, previous, state.offset);
}

fn accelerated_wheel_delta(
    state: &mut ScrollState,
    now: f64,
    scroll: Vec2,
    source: egui::MouseWheelSource,
) -> Vec2 {
    if matches!(
        source,
        egui::MouseWheelSource::Trackpad | egui::MouseWheelSource::Momentum
    ) {
        // The OS already supplies smooth/accelerated deltas. Do not amplify
        // momentum or carry its acceleration into the next physical wheel use.
        state.wheel_acceleration = Default::default();
        return scroll;
    }
    // Windows, Wayland and X11 currently report Unknown; preserve their policy.
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

#[cfg(test)]
mod source_tests {
    use super::{
        Id, Rect, ScrollArea, ScrollState, ScrollbarPolicy, Vec2, accelerated_wheel_delta, egui,
    };

    #[test]
    fn reserved_gutter_keeps_wrapping_width_stable_when_content_grows_and_shrinks() {
        let ctx = egui::Context::default();
        let scroll_id = Id::unique("stable-wrap-test");
        let mut widths = Vec::new();
        for height in [20., 2000., 2000., 20., 20.] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 400.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let out = ScrollArea::new(scroll_id)
                        .reserve_vertical_gutter(true)
                        .scrollbar_x(ScrollbarPolicy::Hidden)
                        .show_viewport(ui, |ui, _, viewport| {
                            widths.push(viewport.width());
                            ui.allocate_space(egui::vec2(viewport.width(), height));
                        });
                    assert_eq!(out.inner_rect.width(), *widths.last().unwrap());
                },
            );
            output.textures_delta.clear();
        }
        assert!(widths.iter().all(|width| *width == widths[0]));
    }

    #[test]
    fn wheel_and_unknown_keep_acceleration_but_trackpad_and_momentum_do_not() {
        let delta = Vec2::new(0.0, 100.0);
        for source in [
            egui::MouseWheelSource::Wheel,
            egui::MouseWheelSource::Unknown,
        ] {
            let mut state = ScrollState::default();
            assert_eq!(
                accelerated_wheel_delta(&mut state, 1.0, delta, source),
                delta
            );
            assert!(accelerated_wheel_delta(&mut state, 1.05, delta, source).y > delta.y);
            for smooth in [
                egui::MouseWheelSource::Trackpad,
                egui::MouseWheelSource::Momentum,
            ] {
                assert_eq!(
                    accelerated_wheel_delta(&mut state, 1.06, delta, smooth),
                    delta
                );
            }
            assert_eq!(
                accelerated_wheel_delta(&mut state, 1.07, delta, source),
                delta
            );
        }
    }
}
