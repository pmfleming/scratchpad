use super::{SPLIT_DRAG_THRESHOLD, SplitAxis};
use eframe::egui;

const SPLIT_RATIO_MIN: f32 = 0.2;
const SPLIT_RATIO_MAX: f32 = 0.8;
const SPLIT_RATIO_CENTER_SNAP_BAND: f32 = 0.05;

pub fn split_preview_spec(
    tile_rect: egui::Rect,
    start_pos: egui::Pos2,
    current_pos: egui::Pos2,
) -> Option<(SplitAxis, bool, f32)> {
    let drag_delta = current_pos - start_pos;
    if drag_delta.length() < SPLIT_DRAG_THRESHOLD {
        return None;
    }

    let horizontal_fraction = (drag_delta.x.abs() / tile_rect.width().max(1.0)).clamp(0.0, 1.0);
    let vertical_fraction = (drag_delta.y.abs() / tile_rect.height().max(1.0)).clamp(0.0, 1.0);

    if horizontal_fraction == 0.0 && vertical_fraction == 0.0 {
        return None;
    }

    let axis = if horizontal_fraction >= vertical_fraction {
        SplitAxis::Vertical
    } else {
        SplitAxis::Horizontal
    };

    let (new_view_first, ratio) = calculate_split_ratio(tile_rect, current_pos, drag_delta, axis);
    Some((axis, new_view_first, ratio))
}

pub fn split_rect(rect: egui::Rect, axis: SplitAxis, ratio: f32) -> (egui::Rect, egui::Rect) {
    match axis {
        SplitAxis::Horizontal => {
            let split_y = rect.top() + rect.height() * ratio;
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), split_y)),
                egui::Rect::from_min_max(egui::pos2(rect.left(), split_y), rect.max),
            )
        }
        SplitAxis::Vertical => {
            let split_x = rect.left() + rect.width() * ratio;
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(split_x, rect.bottom())),
                egui::Rect::from_min_max(egui::pos2(split_x, rect.top()), rect.max),
            )
        }
    }
}

fn calculate_split_ratio(
    tile_rect: egui::Rect,
    current_pos: egui::Pos2,
    drag_delta: egui::Vec2,
    axis: SplitAxis,
) -> (bool, f32) {
    let (fraction, new_view_first) = match axis {
        SplitAxis::Vertical => (
            (current_pos.x - tile_rect.left()) / tile_rect.width().max(1.0),
            drag_delta.x < 0.0,
        ),
        SplitAxis::Horizontal => (
            (current_pos.y - tile_rect.top()) / tile_rect.height().max(1.0),
            drag_delta.y < 0.0,
        ),
    };
    let ratio = snap_and_clamp_ratio(fraction);
    (new_view_first, ratio)
}

fn snap_and_clamp_ratio(fraction: f32) -> f32 {
    let clamped = fraction.clamp(SPLIT_RATIO_MIN, SPLIT_RATIO_MAX);
    if (clamped - 0.5).abs() <= SPLIT_RATIO_CENTER_SNAP_BAND {
        0.5
    } else {
        clamped
    }
}
