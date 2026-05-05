use super::{editor_pixel_offset_resolved, selection_drag_active, sync_local_scroll_state};
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
    if let Some(view) = tab.view_mut(view_id) {
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
    let snapshot = view.latest_display_snapshot.clone();
    let resolve = |id| buffer.document().piece_tree().anchor_position(id);
    let anchor_to_row = scrolling::display_aware_anchor_to_row(snapshot.as_ref(), resolve);
    apply_edge_autoscroll_axis(view, scrolling::Axis::X, velocity.x, &anchor_to_row);
    apply_edge_autoscroll_axis(view, scrolling::Axis::Y, velocity.y, &anchor_to_row);
    view.scroll
        .tick_edge_autoscroll(dt, &anchor_to_row, scrolling::naive_row_to_anchor);
    view.scroll.clear_edge_autoscroll();
    let offset = editor_pixel_offset_resolved(view, buffer, snapshot.as_ref());
    sync_local_scroll_state(ui, scroll_id, offset);
}

fn apply_edge_autoscroll_axis(
    view: &mut crate::app::domain::EditorViewState,
    axis: scrolling::Axis,
    velocity: f32,
    anchor_to_row: &impl Fn(scrolling::ScrollAnchor) -> f32,
) {
    view.scroll.apply_intent(
        scrolling::ScrollIntent::EdgeAutoscroll { axis, velocity },
        anchor_to_row,
        scrolling::naive_row_to_anchor,
    );
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
