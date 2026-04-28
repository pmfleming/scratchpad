use eframe::egui;

use crate::app::ui::scrolling::{Axis, ScrollIntent};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AutoScrollConfig {
    pub(crate) edge_extent: f32,
    pub(crate) max_step: f32,
    pub(crate) cross_axis_margin: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoScrollAxis {
    Horizontal,
    Vertical,
}

pub(crate) fn edge_auto_scroll_delta(
    viewport_rect: egui::Rect,
    pointer_pos: egui::Pos2,
    axis: AutoScrollAxis,
    config: AutoScrollConfig,
) -> f32 {
    if !cross_axis_bounds(viewport_rect, axis, config.cross_axis_margin)
        .contains(&cross_axis_coordinate(pointer_pos, axis))
    {
        return 0.0;
    }

    let leading_distance = distance_to_leading_edge(viewport_rect, pointer_pos, axis);
    if leading_distance <= config.edge_extent {
        return -scaled_auto_scroll_delta(leading_distance, config);
    }

    let trailing_distance = distance_to_trailing_edge(viewport_rect, pointer_pos, axis);
    if trailing_distance <= config.edge_extent {
        return scaled_auto_scroll_delta(trailing_distance, config);
    }

    0.0
}

fn cross_axis_bounds(
    viewport_rect: egui::Rect,
    axis: AutoScrollAxis,
    margin: f32,
) -> std::ops::RangeInclusive<f32> {
    match axis {
        AutoScrollAxis::Horizontal => {
            (viewport_rect.top() - margin)..=(viewport_rect.bottom() + margin)
        }
        AutoScrollAxis::Vertical => {
            (viewport_rect.left() - margin)..=(viewport_rect.right() + margin)
        }
    }
}

fn cross_axis_coordinate(pointer_pos: egui::Pos2, axis: AutoScrollAxis) -> f32 {
    match axis {
        AutoScrollAxis::Horizontal => pointer_pos.y,
        AutoScrollAxis::Vertical => pointer_pos.x,
    }
}

fn distance_to_leading_edge(
    viewport_rect: egui::Rect,
    pointer_pos: egui::Pos2,
    axis: AutoScrollAxis,
) -> f32 {
    match axis {
        AutoScrollAxis::Horizontal => pointer_pos.x - viewport_rect.left(),
        AutoScrollAxis::Vertical => pointer_pos.y - viewport_rect.top(),
    }
}

fn distance_to_trailing_edge(
    viewport_rect: egui::Rect,
    pointer_pos: egui::Pos2,
    axis: AutoScrollAxis,
) -> f32 {
    match axis {
        AutoScrollAxis::Horizontal => viewport_rect.right() - pointer_pos.x,
        AutoScrollAxis::Vertical => viewport_rect.bottom() - pointer_pos.y,
    }
}

fn scaled_auto_scroll_delta(distance: f32, config: AutoScrollConfig) -> f32 {
    let intensity = (1.0 - distance / config.edge_extent).clamp(0.0, 1.0);
    config.max_step * intensity
}

/// Convert a per-frame edge-autoscroll pixel delta into the `ScrollIntent`s
/// that the unified `ScrollManager` consumes.
///
/// `delta` is the output of two `edge_auto_scroll_delta` calls (one per axis)
/// — a *pixel-per-frame* nudge. `frame_dt` is the elapsed time for the frame
/// being rendered. The intent's `velocity` is therefore `delta / frame_dt`,
/// which `ScrollManager::tick_edge_autoscroll(dt, ...)` integrates back into a
/// pixel offset on subsequent frames.
///
/// Emits one `EdgeAutoscroll` per axis whose component is non-zero. Returns
/// an empty slice (via the array length) when the pointer is away from any
/// edge.
#[allow(dead_code)] // Bridge for future ScrollManager plumbing; covered by unit tests.
pub(crate) fn drag_delta_to_intents(delta: egui::Vec2, frame_dt: f32) -> Vec<ScrollIntent> {
    let mut out = Vec::with_capacity(2);
    if frame_dt <= 0.0 {
        return out;
    }
    if delta.x != 0.0 {
        out.push(ScrollIntent::EdgeAutoscroll {
            axis: Axis::X,
            velocity: delta.x / frame_dt,
        });
    }
    if delta.y != 0.0 {
        out.push(ScrollIntent::EdgeAutoscroll {
            axis: Axis::Y,
            velocity: delta.y / frame_dt,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{AutoScrollAxis, AutoScrollConfig, drag_delta_to_intents, edge_auto_scroll_delta};
    use crate::app::ui::scrolling::{Axis, ScrollIntent};
    use eframe::egui::{Rect, pos2, vec2};

    const CONFIG: AutoScrollConfig = AutoScrollConfig {
        edge_extent: 36.0,
        max_step: 18.0,
        cross_axis_margin: 12.0,
    };

    #[test]
    fn edge_auto_scroll_delta_pushes_toward_leading_edge() {
        let viewport = Rect::from_min_size(pos2(40.0, 10.0), vec2(240.0, 30.0));

        assert!(
            edge_auto_scroll_delta(
                viewport,
                pos2(42.0, 24.0),
                AutoScrollAxis::Horizontal,
                CONFIG,
            ) < 0.0
        );
    }

    #[test]
    fn edge_auto_scroll_delta_pushes_toward_trailing_edge() {
        let viewport = Rect::from_min_size(pos2(40.0, 10.0), vec2(140.0, 240.0));

        assert!(
            edge_auto_scroll_delta(
                viewport,
                pos2(70.0, 248.0),
                AutoScrollAxis::Vertical,
                CONFIG,
            ) > 0.0
        );
    }

    #[test]
    fn edge_auto_scroll_delta_is_zero_outside_cross_axis_margin() {
        let viewport = Rect::from_min_size(pos2(40.0, 10.0), vec2(240.0, 30.0));

        assert_eq!(
            edge_auto_scroll_delta(
                viewport,
                pos2(42.0, 80.0),
                AutoScrollAxis::Horizontal,
                CONFIG,
            ),
            0.0
        );
    }

    // ---- drag_delta_to_intents ----

    #[test]
    fn drag_delta_zero_emits_no_intents() {
        let intents = drag_delta_to_intents(vec2(0.0, 0.0), 1.0 / 60.0);
        assert!(intents.is_empty());
    }

    #[test]
    fn drag_delta_y_only_emits_one_y_intent() {
        let intents = drag_delta_to_intents(vec2(0.0, 4.0), 1.0 / 60.0);
        assert_eq!(intents.len(), 1);
        match intents[0] {
            ScrollIntent::EdgeAutoscroll { axis, velocity } => {
                assert_eq!(axis, Axis::Y);
                assert!((velocity - 4.0 * 60.0).abs() < 1e-3);
            }
            _ => panic!("expected EdgeAutoscroll, got {:?}", intents[0]),
        }
    }

    #[test]
    fn drag_delta_both_axes_emits_two_intents_in_x_then_y_order() {
        let intents = drag_delta_to_intents(vec2(-2.0, 5.0), 1.0 / 60.0);
        assert_eq!(intents.len(), 2);
        assert!(matches!(
            intents[0],
            ScrollIntent::EdgeAutoscroll { axis: Axis::X, .. }
        ));
        assert!(matches!(
            intents[1],
            ScrollIntent::EdgeAutoscroll { axis: Axis::Y, .. }
        ));
    }

    #[test]
    fn drag_delta_zero_or_negative_dt_emits_no_intents() {
        assert!(drag_delta_to_intents(vec2(3.0, 4.0), 0.0).is_empty());
        assert!(drag_delta_to_intents(vec2(3.0, 4.0), -1.0 / 60.0).is_empty());
    }
}
