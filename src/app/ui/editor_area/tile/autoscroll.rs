use super::{selection_drag_active, sync_local_scroll_state};
use crate::app::domain::{ViewId, WorkspaceTab};
use crate::app::ui::autoscroll::{AutoScrollAxis, AutoScrollConfig, edge_auto_scroll_velocity};
use crate::app::ui::scrolling;
use eframe::egui;

const EDITOR_SELECTION_AUTOSCROLL_EDGE_ROWS: f32 = 1.5;
const EDITOR_SELECTION_AUTOSCROLL_MIN_EDGE_PX: f32 = 24.0;
const EDITOR_SELECTION_AUTOSCROLL_OUTSIDE_ROWS: f32 = 8.0;
const EDITOR_SELECTION_AUTOSCROLL_MIN_ROWS_PER_SEC: f32 = 8.0;
const EDITOR_SELECTION_AUTOSCROLL_MAX_ROWS_PER_SEC: f32 = 120.0;
const EDITOR_SELECTION_AUTOSCROLL_CROSS_AXIS_MARGIN: f32 = 24.0;

pub(super) fn apply_selection_edge_autoscroll_intent(
    ui: &egui::Ui,
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    scroll_id: egui::Id,
    interaction_response: Option<&egui::Response>,
    inner_rect: egui::Rect,
    row_height: f32,
) {
    if !selection_drag_active(ui, interaction_response, inner_rect) {
        return;
    }
    let Some(velocity) = selection_edge_autoscroll_velocity(ui, inner_rect, row_height) else {
        return;
    };
    if velocity == egui::Vec2::ZERO {
        clear_edge_autoscroll(tab, view_id);
        return;
    }
    ui.ctx().request_repaint();
    let dt = ui.input(|input| input.stable_dt).min(0.1);
    apply_edge_autoscroll_velocity(ui, tab, view_id, scroll_id, velocity, dt);
}

fn editor_selection_autoscroll_config(row_height: f32) -> AutoScrollConfig {
    let row_height = row_height.max(1.0);
    AutoScrollConfig {
        edge_extent: (EDITOR_SELECTION_AUTOSCROLL_EDGE_ROWS * row_height)
            .max(EDITOR_SELECTION_AUTOSCROLL_MIN_EDGE_PX),
        outside_extent: EDITOR_SELECTION_AUTOSCROLL_OUTSIDE_ROWS * row_height,
        min_velocity: EDITOR_SELECTION_AUTOSCROLL_MIN_ROWS_PER_SEC * row_height,
        max_velocity: EDITOR_SELECTION_AUTOSCROLL_MAX_ROWS_PER_SEC * row_height,
        cross_axis_margin: EDITOR_SELECTION_AUTOSCROLL_CROSS_AXIS_MARGIN,
    }
}

fn selection_edge_autoscroll_velocity(
    ui: &egui::Ui,
    inner_rect: egui::Rect,
    row_height: f32,
) -> Option<egui::Vec2> {
    let pointer_pos = ui.input(|input| input.pointer.latest_pos())?;
    Some(selection_edge_drag_velocity(
        inner_rect,
        pointer_pos,
        row_height,
    ))
}

fn clear_edge_autoscroll(tab: &mut WorkspaceTab, view_id: ViewId) {
    if let Some(view) = tab.layout.view_mut(view_id) {
        view.scroll.clear_edge_autoscroll();
    }
}

fn apply_edge_autoscroll_velocity(
    ui: &egui::Ui,
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    scroll_id: egui::Id,
    velocity: egui::Vec2,
    dt: f32,
) {
    let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) else {
        return;
    };
    let state = scrolling::ScrollState::load(ui, scroll_id);
    let offset = edge_autoscroll_offset(
        state.offset,
        velocity,
        dt,
        state.content_size,
        state.viewport_size,
    );
    view.set_editor_pixel_offset_resolved(buffer, offset);
    sync_local_scroll_state(ui, scroll_id, offset);
}

fn edge_autoscroll_offset(
    current: egui::Vec2,
    velocity: egui::Vec2,
    dt: f32,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
) -> egui::Vec2 {
    let max_offset = scrolling::ScrollState::max_offset(content_size, viewport_size, false);
    egui::vec2(
        (current.x + velocity.x * dt).clamp(0.0, max_offset.x),
        (current.y + velocity.y * dt).clamp(0.0, max_offset.y),
    )
}

fn selection_edge_drag_velocity(
    viewport_rect: egui::Rect,
    pointer_pos: egui::Pos2,
    row_height: f32,
) -> egui::Vec2 {
    let config = editor_selection_autoscroll_config(row_height);
    egui::vec2(
        edge_auto_scroll_velocity(
            viewport_rect,
            pointer_pos,
            AutoScrollAxis::Horizontal,
            config,
        ),
        edge_auto_scroll_velocity(viewport_rect, pointer_pos, AutoScrollAxis::Vertical, config),
    )
}

#[cfg(test)]
mod tests {
    use super::{edge_autoscroll_offset, egui, selection_edge_drag_velocity};

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 100.0))
    }

    #[test]
    fn selection_autoscroll_moves_down_below_viewport() {
        let velocity = selection_edge_drag_velocity(viewport(), egui::pos2(100.0, 120.0), 16.0);

        assert!(velocity.y > 0.0);
        assert_eq!(velocity.x, 0.0);
    }

    #[test]
    fn selection_autoscroll_is_idle_near_center() {
        let velocity = selection_edge_drag_velocity(viewport(), egui::pos2(100.0, 50.0), 16.0);

        assert_eq!(velocity, egui::Vec2::ZERO);
    }

    #[test]
    fn edge_autoscroll_offset_advances_without_snap_back() {
        let offset = edge_autoscroll_offset(
            egui::vec2(0.0, 40.0),
            egui::vec2(0.0, 100.0),
            0.5,
            egui::vec2(200.0, 400.0),
            egui::vec2(200.0, 100.0),
        );

        assert_eq!(offset, egui::vec2(0.0, 90.0));
    }

    #[test]
    fn edge_autoscroll_offset_clamps_to_scroll_extent() {
        let offset = edge_autoscroll_offset(
            egui::vec2(0.0, 290.0),
            egui::vec2(0.0, 100.0),
            0.5,
            egui::vec2(200.0, 400.0),
            egui::vec2(200.0, 100.0),
        );

        assert_eq!(offset, egui::vec2(0.0, 300.0));
    }
}
