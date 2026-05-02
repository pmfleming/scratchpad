use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AutoScrollConfig {
    pub(crate) edge_extent: f32,
    pub(crate) outside_extent: f32,
    pub(crate) min_velocity: f32,
    pub(crate) max_velocity: f32,
    pub(crate) cross_axis_margin: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoScrollAxis {
    Horizontal,
    Vertical,
}

pub(crate) fn edge_auto_scroll_velocity(
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
    if leading_distance < config.edge_extent {
        return -scaled_auto_scroll_velocity(leading_distance, config);
    }

    let trailing_distance = distance_to_trailing_edge(viewport_rect, pointer_pos, axis);
    if trailing_distance < config.edge_extent {
        return scaled_auto_scroll_velocity(trailing_distance, config);
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

fn scaled_auto_scroll_velocity(distance_to_edge: f32, config: AutoScrollConfig) -> f32 {
    if config.edge_extent <= 0.0 || config.max_velocity <= 0.0 {
        return 0.0;
    }
    let inside_edge = (config.edge_extent - distance_to_edge).clamp(0.0, config.edge_extent);
    let outside_edge = (-distance_to_edge).clamp(0.0, config.outside_extent.max(0.0));
    let max_penetration = (config.edge_extent + config.outside_extent.max(0.0)).max(1.0);
    let intensity = ((inside_edge + outside_edge) / max_penetration).clamp(0.0, 1.0);
    let curved = intensity * intensity;
    config.min_velocity.max(0.0) + (config.max_velocity - config.min_velocity).max(0.0) * curved
}
