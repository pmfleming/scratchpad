use crate::app::domain::WorkspaceTab;
use eframe::egui;

mod paint;
mod state;

pub(crate) use state::{
    TabDropAxis, TabDropZone, TabRectEntry, active_drag_source_for_context,
    begin_tab_drag_if_needed, has_tab_drag_for_context, is_drag_active_for_context,
};

pub(crate) fn sync_drag_state(ui: &egui::Ui) {
    let _ = state::update_current_tab_drag(ui);
}

pub(crate) fn update_tab_drag(
    ui: &egui::Ui,
    zones: &[TabDropZone],
    total_tab_count: usize,
) -> Option<(usize, usize)> {
    let drag_state = state::update_current_tab_drag(ui)?;
    let drag_active = state::drag_is_active(drag_state);
    let drop_target = drag_active
        .then(|| state::locate_drop_slot(zones, drag_state.current_pos))
        .flatten();

    if let Some((zone_index, drop_slot)) = drop_target {
        paint::paint_tab_reorder_marker(ui.ctx(), &zones[zone_index], drop_slot);
    }

    if ui.input(|input| input.pointer.primary_down()) {
        return None;
    }

    state::clear_tab_drag_state(ui);

    let (_, drop_slot) = drop_target?;
    let to_index = state::resolve_drop_slot(drag_state.source_index, drop_slot, total_tab_count);
    (to_index != drag_state.source_index).then_some((drag_state.source_index, to_index))
}

pub(crate) fn paint_dragged_tab_ghost(ctx: &egui::Context, tabs: &[WorkspaceTab]) {
    let Some(drag_state) = state::current_tab_drag_state_for_context(ctx) else {
        return;
    };
    if !state::drag_is_active(drag_state) {
        return;
    }

    paint::paint_dragged_tab_ghost(ctx, tabs, drag_state);
}

pub(crate) fn auto_scroll_tab_strip(
    ctx: &egui::Context,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
    content_width: f32,
    scroll_state: &egui::scroll_area::State,
) {
    let Some(drag_state) = state::current_tab_drag_state_for_context(ctx) else {
        return;
    };
    if !state::drag_is_active(drag_state) {
        return;
    }

    let delta_x = state::auto_scroll_delta(viewport_rect, drag_state.current_pos);
    if delta_x.abs() <= f32::EPSILON {
        return;
    }

    let max_offset_x = (content_width - viewport_rect.width()).max(0.0);
    let next_offset_x = (scroll_state.offset.x + delta_x).clamp(0.0, max_offset_x);
    if (next_offset_x - scroll_state.offset.x).abs() <= f32::EPSILON {
        return;
    }

    let mut next_state = *scroll_state;
    next_state.offset.x = next_offset_x;
    next_state.store(ctx, scroll_area_id);
    ctx.request_repaint();
}
