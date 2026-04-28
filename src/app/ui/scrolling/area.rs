use eframe::egui::{self, Id, Rect, Sense, Ui, UiBuilder, Vec2, epaint::Shape, pos2, vec2};

use super::source::ScrollSource;
use super::state::{ScrollState, ScrollbarDragState};
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

        // Outer rect.
        let outer_rect = match self.max_size {
            Some(size) => {
                let min = ui.available_rect_before_wrap().min;
                Rect::from_min_size(min, size)
            }
            None => ui.available_rect_before_wrap(),
        };

        // Determine which scrollbars will be visible from the previous frame's
        // content/viewport sizes. This causes a one-frame lag on first show
        // when scrollbar visibility flips, matching egui's behavior.
        let show_x = scrollbar_visible(
            self.scrollbar_x,
            state.content_size.x,
            state.viewport_size.x.max(outer_rect.width()),
        );
        let show_y = scrollbar_visible(
            self.scrollbar_y,
            state.content_size.y,
            state.viewport_size.y.max(outer_rect.height()),
        );
        let bar_x = if show_x {
            self.scrollbar_thickness
        } else {
            0.0
        };
        let bar_y = if show_y {
            self.scrollbar_thickness
        } else {
            0.0
        };

        let inner_rect = Rect::from_min_max(
            outer_rect.min,
            pos2(outer_rect.max.x - bar_y, outer_rect.max.y - bar_x),
        );

        state.viewport_size = inner_rect.size();

        // Resolve any pending programmatic target before clamping.
        if self.source.programmatic
            && let Some(target) = state.pending_target.take()
        {
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

        // Hover gates wheel/scrollbar input.
        let outer_response = ui.interact(outer_rect, self.id.with("__outer"), Sense::hover());
        let hovered = outer_response.hovered();

        // Mouse wheel.
        let prev_offset = state.offset;
        if hovered && self.source.mouse_wheel {
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            if scroll != Vec2::ZERO {
                state.offset.x -= scroll.x;
                state.offset.y -= scroll.y;
                state.user_scrolled = true;
            }
        }

        state.clamp_offset(self.eof_overscroll);

        // Build a child Ui clipped to the inner rect.
        let visible_rect =
            Rect::from_min_size(pos2(state.offset.x, state.offset.y), inner_rect.size());

        let mut content_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    inner_rect.min - state.offset,
                    Vec2::splat(f32::INFINITY),
                ))
                .layout(*ui.layout()),
        );
        content_ui.set_clip_rect(inner_rect);

        let inner_value = add_contents(&mut content_ui, state.offset, visible_rect);
        // Content size derived from the inner Ui's min_rect, translated back
        // out of the offset space so it represents the absolute extent.
        let content_min_rect = content_ui.min_rect();
        let content_size = (content_min_rect.max - (inner_rect.min - state.offset)).max(Vec2::ZERO);
        state.content_size = content_size;

        // Re-clamp after we know the latest content size.
        state.clamp_offset(self.eof_overscroll);

        // Paint scrollbars and handle drag.
        if show_y {
            let bar_rect = Rect::from_min_max(
                pos2(
                    outer_rect.max.x - self.scrollbar_thickness,
                    outer_rect.min.y,
                ),
                pos2(outer_rect.max.x, outer_rect.max.y - bar_x),
            );
            paint_and_handle_scrollbar(
                ui,
                self.id.with("__bar_y"),
                bar_rect,
                Axis::Y,
                &mut state,
                self.eof_overscroll,
                self.source.scroll_bar,
            );
        }
        if show_x {
            let bar_rect = Rect::from_min_max(
                pos2(
                    outer_rect.min.x,
                    outer_rect.max.y - self.scrollbar_thickness,
                ),
                pos2(outer_rect.max.x - bar_y, outer_rect.max.y),
            );
            paint_and_handle_scrollbar(
                ui,
                self.id.with("__bar_x"),
                bar_rect,
                Axis::X,
                &mut state,
                self.eof_overscroll,
                self.source.scroll_bar,
            );
        }

        // Reserve outer rect in parent layout.
        ui.advance_cursor_after_rect(outer_rect);

        let did_scroll = prev_offset != state.offset;
        state.store(ui, self.id);

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
enum Axis {
    X,
    Y,
}

fn scrollbar_visible(policy: ScrollbarPolicy, content: f32, viewport: f32) -> bool {
    match policy {
        ScrollbarPolicy::AlwaysVisible => true,
        ScrollbarPolicy::Hidden => false,
        ScrollbarPolicy::VisibleWhenNeeded => content > viewport + 0.5,
    }
}

/// Layout of a scrollbar thumb along a single axis. Pure math, exposed for
/// testing the thumb-position mapping in isolation from egui painting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ThumbLayout {
    pub thumb_start: f32,
    pub thumb_extent: f32,
    pub track_extent: f32,
}

/// Compute the thumb's position and extent along the bar.
///
/// * `content` is the total content size on this axis.
/// * `viewport` is the visible viewport size on this axis.
/// * `bar_extent` is the bar's pixel length on this axis.
/// * `offset` is the current scroll offset (0..=max_off).
/// * `max_off` is the maximum scroll offset for this axis (already accounts
///   for EOF overscroll if applicable).
/// * `extra` is `viewport` for the Y axis when EOF overscroll is enabled,
///   else 0.0.
pub(crate) fn thumb_layout(
    content: f32,
    viewport: f32,
    bar_extent: f32,
    offset: f32,
    max_off: f32,
    extra: f32,
) -> ThumbLayout {
    if bar_extent <= 0.0 || content <= 0.0 {
        return ThumbLayout {
            thumb_start: 0.0,
            thumb_extent: 0.0,
            track_extent: 0.0,
        };
    }
    let virtual_content = content + extra;
    let thumb_frac = (viewport / virtual_content).clamp(0.05, 1.0);
    let thumb_extent = (bar_extent * thumb_frac).max(16.0).min(bar_extent);
    let track_extent = (bar_extent - thumb_extent).max(0.0);
    let pos_frac = if max_off > 0.0 { offset / max_off } else { 0.0 };
    let thumb_start = pos_frac * track_extent;
    ThumbLayout {
        thumb_start,
        thumb_extent,
        track_extent,
    }
}

/// Map a pointer coordinate along the bar to a scroll offset, centering the
/// thumb on the click. Used for click-on-track jumps.
pub(crate) fn track_click_offset(
    pos_along_bar: f32,
    thumb_extent: f32,
    track_extent: f32,
    max_off: f32,
) -> f32 {
    let new_thumb_start = (pos_along_bar - thumb_extent * 0.5).clamp(0.0, track_extent);
    if track_extent > 0.0 {
        new_thumb_start / track_extent * max_off
    } else {
        0.0
    }
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
    let (axis_idx, bar_extent) = match axis {
        Axis::X => (0usize, bar_rect.width()),
        Axis::Y => (1usize, bar_rect.height()),
    };
    let content = state.content_size[axis_idx];
    let viewport = state.viewport_size[axis_idx];
    let max_off =
        ScrollState::max_offset(state.content_size, state.viewport_size, eof_overscroll)[axis_idx];
    if bar_extent <= 0.0 || content <= 0.0 {
        return;
    }
    let extra = if eof_overscroll && matches!(axis, Axis::Y) {
        viewport
    } else {
        0.0
    };
    let layout = thumb_layout(
        content,
        viewport,
        bar_extent,
        state.offset[axis_idx],
        max_off,
        extra,
    );
    let ThumbLayout {
        thumb_start,
        thumb_extent,
        track_extent,
    } = layout;

    let thumb_rect = match axis {
        Axis::X => Rect::from_min_size(
            pos2(bar_rect.min.x + thumb_start, bar_rect.min.y),
            vec2(thumb_extent, bar_rect.height()),
        ),
        Axis::Y => Rect::from_min_size(
            pos2(bar_rect.min.x, bar_rect.min.y + thumb_start),
            vec2(bar_rect.width(), thumb_extent),
        ),
    };

    let sense = if interactive {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let response = ui.interact(bar_rect, id, sense);

    if interactive {
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if response.drag_started() {
            if let Some(p) = pointer {
                let in_thumb = thumb_rect.contains(p);
                let initial_offset = if in_thumb {
                    state.offset[axis_idx]
                } else {
                    let pos_along = match axis {
                        Axis::X => p.x - bar_rect.min.x,
                        Axis::Y => p.y - bar_rect.min.y,
                    };
                    track_click_offset(pos_along, thumb_extent, track_extent, max_off)
                };
                state.scrollbar_drag[axis_idx] = Some(ScrollbarDragState {
                    origin_pointer: p,
                    origin_offset: initial_offset,
                });
                state.offset[axis_idx] = initial_offset;
                state.user_scrolled = true;
            }
        } else if response.dragged() {
            if let (Some(drag), Some(p)) = (state.scrollbar_drag[axis_idx], pointer) {
                let delta_pixels = match axis {
                    Axis::X => p.x - drag.origin_pointer.x,
                    Axis::Y => p.y - drag.origin_pointer.y,
                };
                let delta_offset = if track_extent > 0.0 {
                    delta_pixels / track_extent * max_off
                } else {
                    0.0
                };
                state.offset[axis_idx] = (drag.origin_offset + delta_offset).clamp(0.0, max_off);
                state.user_scrolled = true;
            }
        } else {
            state.scrollbar_drag[axis_idx] = None;
        }
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
        .add(Shape::rect_filled(thumb_rect, 2.0, thumb_color));
}

#[cfg(test)]
mod tests {
    use super::{ThumbLayout, thumb_layout, track_click_offset};

    #[test]
    fn thumb_extent_scales_with_viewport_to_content_ratio() {
        // viewport = 50% of content → thumb covers 50% of bar.
        let lay = thumb_layout(1_000.0, 500.0, 200.0, 0.0, 500.0, 0.0);
        assert_eq!(lay.thumb_extent, 100.0);
        assert_eq!(lay.track_extent, 100.0);
        assert_eq!(lay.thumb_start, 0.0);
    }

    #[test]
    fn thumb_extent_floors_at_sixteen_pixels_for_huge_documents() {
        // viewport tiny relative to content → thumb_frac would be < 16/200,
        // so the floor at 16 px engages.
        let lay = thumb_layout(100_000.0, 200.0, 200.0, 0.0, 99_800.0, 0.0);
        assert_eq!(lay.thumb_extent, 16.0);
        assert_eq!(lay.track_extent, 184.0);
    }

    #[test]
    fn thumb_start_tracks_offset_at_zero_and_max() {
        let lay_top = thumb_layout(2_000.0, 400.0, 200.0, 0.0, 1_600.0, 0.0);
        assert_eq!(lay_top.thumb_start, 0.0);

        let lay_bot = thumb_layout(2_000.0, 400.0, 200.0, 1_600.0, 1_600.0, 0.0);
        assert_eq!(lay_bot.thumb_start, lay_bot.track_extent);
    }

    #[test]
    fn thumb_start_is_proportional_at_midway() {
        let lay = thumb_layout(2_000.0, 400.0, 200.0, 800.0, 1_600.0, 0.0);
        // pos_frac = 0.5 → thumb_start = 0.5 * track_extent.
        assert_eq!(lay.thumb_start, lay.track_extent * 0.5);
    }

    #[test]
    fn thumb_layout_handles_eof_overscroll_extra() {
        // With overscroll, virtual_content = content + viewport, so thumb_frac
        // shrinks: viewport / (content + viewport).
        let lay_no = thumb_layout(2_000.0, 400.0, 200.0, 0.0, 1_600.0, 0.0);
        let lay_yes = thumb_layout(2_000.0, 400.0, 200.0, 0.0, 2_000.0, 400.0);
        assert!(lay_yes.thumb_extent < lay_no.thumb_extent);
    }

    #[test]
    fn thumb_layout_returns_zero_when_bar_or_content_empty() {
        assert_eq!(
            thumb_layout(1_000.0, 500.0, 0.0, 0.0, 500.0, 0.0),
            ThumbLayout {
                thumb_start: 0.0,
                thumb_extent: 0.0,
                track_extent: 0.0,
            }
        );
        assert_eq!(
            thumb_layout(0.0, 500.0, 200.0, 0.0, 500.0, 0.0),
            ThumbLayout {
                thumb_start: 0.0,
                thumb_extent: 0.0,
                track_extent: 0.0,
            }
        );
    }

    #[test]
    fn thumb_start_is_zero_when_max_offset_is_zero() {
        // Content fits in viewport → max_off = 0 → thumb pinned at start.
        let lay = thumb_layout(300.0, 400.0, 200.0, 0.0, 0.0, 0.0);
        assert_eq!(lay.thumb_start, 0.0);
    }

    #[test]
    fn track_click_centers_thumb_on_cursor() {
        // Bar 200, thumb 50, track 150. Click at pos_along = 100 → desired
        // thumb_start = 100 - 25 = 75 → offset = 75/150 * max = 0.5 * 1000 = 500.
        let off = track_click_offset(100.0, 50.0, 150.0, 1_000.0);
        assert_eq!(off, 500.0);
    }

    #[test]
    fn track_click_clamps_to_zero_at_top() {
        let off = track_click_offset(0.0, 50.0, 150.0, 1_000.0);
        assert_eq!(off, 0.0);
    }

    #[test]
    fn track_click_clamps_to_max_off_at_bottom() {
        let off = track_click_offset(10_000.0, 50.0, 150.0, 1_000.0);
        assert_eq!(off, 1_000.0);
    }

    #[test]
    fn track_click_returns_zero_when_track_collapsed() {
        // Thumb fills the bar → no track room → any click maps to 0.
        let off = track_click_offset(50.0, 200.0, 0.0, 1_000.0);
        assert_eq!(off, 0.0);
    }
}
