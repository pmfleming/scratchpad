use super::{SPLIT_DRAG_THRESHOLD, SplitAxis};
use eframe::egui;

pub fn split_preview_spec(
    tile_rect: egui::Rect,
    drag_delta: egui::Vec2,
) -> Option<(SplitAxis, bool, f32)> {
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

    let (new_view_first, ratio) = calculate_split_ratio(tile_rect, drag_delta, axis);
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
    drag_delta: egui::Vec2,
    axis: SplitAxis,
) -> (bool, f32) {
    let (dominant_delta, extent, new_view_first) = match axis {
        SplitAxis::Vertical => (drag_delta.x.abs(), tile_rect.width(), drag_delta.x < 0.0),
        SplitAxis::Horizontal => (drag_delta.y.abs(), tile_rect.height(), drag_delta.y < 0.0),
    };

    let new_tile_fraction = (dominant_delta / extent.max(1.0)).clamp(0.3, 0.7);
    let ratio = if new_view_first {
        new_tile_fraction
    } else {
        1.0 - new_tile_fraction
    };

    (new_view_first, ratio.clamp(0.2, 0.8))
}
