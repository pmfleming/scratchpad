use crate::app::ui::autoscroll::{AutoScrollAxis, AutoScrollConfig, edge_auto_scroll_velocity};
use eframe::egui;

pub(super) const TAB_DRAG_AUTOSCROLL_MAX_STEP: f32 = 18.0;
const TAB_DRAG_AUTOSCROLL_CONFIG: AutoScrollConfig = AutoScrollConfig {
    edge_extent: 36.0,
    outside_extent: 0.0,
    min_velocity: 0.0,
    max_velocity: TAB_DRAG_AUTOSCROLL_MAX_STEP,
    cross_axis_margin: 12.0,
};

pub(crate) fn auto_scroll_delta(
    viewport_rect: egui::Rect,
    pointer_pos: egui::Pos2,
    axis: super::TabDropAxis,
) -> f32 {
    edge_auto_scroll_velocity(
        viewport_rect,
        pointer_pos,
        match axis {
            super::TabDropAxis::Horizontal => AutoScrollAxis::Horizontal,
            super::TabDropAxis::Vertical => AutoScrollAxis::Vertical,
        },
        TAB_DRAG_AUTOSCROLL_CONFIG,
    )
}
