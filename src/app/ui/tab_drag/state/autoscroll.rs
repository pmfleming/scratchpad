use eframe::egui;

const TAB_DRAG_AUTOSCROLL_EDGE: f32 = 36.0;
pub(super) const TAB_DRAG_AUTOSCROLL_MAX_STEP: f32 = 18.0;
const TAB_DRAG_VERTICAL_MARGIN: f32 = 12.0;

pub(crate) fn auto_scroll_delta(viewport_rect: egui::Rect, pointer_pos: egui::Pos2) -> f32 {
    if !auto_scroll_vertical_bounds(viewport_rect).contains(&pointer_pos.y) {
        return 0.0;
    }

    if let Some(delta) = left_auto_scroll_delta(viewport_rect, pointer_pos) {
        return delta;
    }

    if let Some(delta) = right_auto_scroll_delta(viewport_rect, pointer_pos) {
        return delta;
    }

    0.0
}

pub(crate) fn vertical_auto_scroll_delta(
    viewport_rect: egui::Rect,
    pointer_pos: egui::Pos2,
) -> f32 {
    if !auto_scroll_horizontal_bounds(viewport_rect).contains(&pointer_pos.x) {
        return 0.0;
    }

    if let Some(delta) = top_auto_scroll_delta(viewport_rect, pointer_pos) {
        return delta;
    }

    if let Some(delta) = bottom_auto_scroll_delta(viewport_rect, pointer_pos) {
        return delta;
    }

    0.0
}

fn auto_scroll_vertical_bounds(viewport_rect: egui::Rect) -> std::ops::RangeInclusive<f32> {
    (viewport_rect.top() - TAB_DRAG_VERTICAL_MARGIN)
        ..=(viewport_rect.bottom() + TAB_DRAG_VERTICAL_MARGIN)
}

fn auto_scroll_horizontal_bounds(viewport_rect: egui::Rect) -> std::ops::RangeInclusive<f32> {
    (viewport_rect.left() - TAB_DRAG_VERTICAL_MARGIN)
        ..=(viewport_rect.right() + TAB_DRAG_VERTICAL_MARGIN)
}

fn left_auto_scroll_delta(viewport_rect: egui::Rect, pointer_pos: egui::Pos2) -> Option<f32> {
    let left_distance = pointer_pos.x - viewport_rect.left();
    (left_distance <= TAB_DRAG_AUTOSCROLL_EDGE).then(|| -scaled_auto_scroll_delta(left_distance))
}

fn right_auto_scroll_delta(viewport_rect: egui::Rect, pointer_pos: egui::Pos2) -> Option<f32> {
    let right_distance = viewport_rect.right() - pointer_pos.x;
    (right_distance <= TAB_DRAG_AUTOSCROLL_EDGE).then(|| scaled_auto_scroll_delta(right_distance))
}

fn top_auto_scroll_delta(viewport_rect: egui::Rect, pointer_pos: egui::Pos2) -> Option<f32> {
    let top_distance = pointer_pos.y - viewport_rect.top();
    (top_distance <= TAB_DRAG_AUTOSCROLL_EDGE).then(|| -scaled_auto_scroll_delta(top_distance))
}

fn bottom_auto_scroll_delta(viewport_rect: egui::Rect, pointer_pos: egui::Pos2) -> Option<f32> {
    let bottom_distance = viewport_rect.bottom() - pointer_pos.y;
    (bottom_distance <= TAB_DRAG_AUTOSCROLL_EDGE).then(|| scaled_auto_scroll_delta(bottom_distance))
}

fn scaled_auto_scroll_delta(distance: f32) -> f32 {
    let intensity = (1.0 - distance / TAB_DRAG_AUTOSCROLL_EDGE).clamp(0.0, 1.0);
    TAB_DRAG_AUTOSCROLL_MAX_STEP * intensity
}
