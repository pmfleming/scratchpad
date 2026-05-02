use crate::app::app_state::ScratchpadApp;
use eframe::egui;

mod paint;
mod state;

pub(crate) use state::{
    TabDropAxis, TabDropZone, TabRectEntry, active_drag_sources_for_context,
    begin_tab_drag_if_needed, is_drag_active_for_context,
};

pub(crate) enum TabDragCommit {
    Reorder {
        from_index: usize,
        to_index: usize,
    },
    ReorderGroup {
        from_indices: Vec<usize>,
        to_index: usize,
    },
    Combine {
        source_index: usize,
        target_index: usize,
    },
    CombineGroup {
        source_indices: Vec<usize>,
        target_index: usize,
    },
}

pub(crate) fn sync_drag_state(ui: &egui::Ui) {
    let _ = state::update_current_tab_drag(ui);
}

pub(crate) fn update_tab_drag(
    ui: &egui::Ui,
    zones: &[TabDropZone],
    total_tab_count: usize,
) -> Option<TabDragCommit> {
    let drag_state = state::update_current_tab_drag(ui)?;
    let drag_active = state::drag_is_active(&drag_state);
    let dragged_indices = drag_state.dragged_indices.clone();
    let allow_combine =
        drag_sources_allow_combine(zones, &dragged_indices, drag_state.source_index);
    let drop_intent = drag_active
        .then(|| state::locate_drop_intent(zones, drag_state.current_pos, allow_combine))
        .flatten();

    if let Some(drop_intent) = &drop_intent {
        match drop_intent {
            state::TabDropIntent::Reorder {
                zone_index,
                drop_slot,
            } => {
                paint::paint_tab_reorder_marker(ui.ctx(), &zones[*zone_index], *drop_slot);
            }
            state::TabDropIntent::Combine {
                zone_index,
                target_index,
            } => {
                paint::paint_tab_combine_target(ui.ctx(), &zones[*zone_index], *target_index);
            }
        }
    }

    if ui.input(|input| input.pointer.primary_down()) {
        return None;
    }

    state::clear_tab_drag_state(ui);

    match drop_intent? {
        state::TabDropIntent::Reorder { drop_slot, .. } => {
            if dragged_indices.len() > 1 {
                Some(TabDragCommit::ReorderGroup {
                    from_indices: dragged_indices,
                    to_index: drop_slot.min(total_tab_count),
                })
            } else {
                let to_index =
                    state::resolve_drop_slot(drag_state.source_index, drop_slot, total_tab_count);
                (to_index != drag_state.source_index).then_some(TabDragCommit::Reorder {
                    from_index: drag_state.source_index,
                    to_index,
                })
            }
        }
        state::TabDropIntent::Combine { target_index, .. } => {
            if dragged_indices.len() > 1 {
                (!dragged_indices.contains(&target_index)).then_some(TabDragCommit::CombineGroup {
                    source_indices: dragged_indices,
                    target_index,
                })
            } else {
                (target_index != drag_state.source_index).then_some(TabDragCommit::Combine {
                    source_index: drag_state.source_index,
                    target_index,
                })
            }
        }
    }
}

fn drag_sources_allow_combine(
    zones: &[TabDropZone],
    dragged_indices: &[usize],
    source_index: usize,
) -> bool {
    let mut source_can_combine = false;
    let mut selected_workspace_can_combine = false;

    for entry in zones.iter().flat_map(|zone| zone.entries.iter()) {
        if entry.index == source_index && entry.combine_enabled {
            source_can_combine = true;
        }
        if dragged_indices.len() > 1
            && entry.combine_enabled
            && dragged_indices.contains(&entry.index)
        {
            selected_workspace_can_combine = true;
        }
    }

    source_can_combine || selected_workspace_can_combine
}

pub(crate) fn paint_dragged_tab_ghost(ctx: &egui::Context, app: &ScratchpadApp) {
    let Some(drag_state) = state::current_tab_drag_state_for_context(ctx) else {
        return;
    };
    if !state::drag_is_active(&drag_state) {
        return;
    }
    paint::paint_dragged_tab_ghost(ctx, app, drag_state);
}

pub(crate) fn auto_scroll_tab_list(
    ctx: &egui::Context,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
    content_extent: f32,
    scroll_state: &egui::scroll_area::State,
    axis: TabDropAxis,
) {
    let Some(drag_state) = state::current_tab_drag_state_for_context(ctx) else {
        return;
    };
    if !state::drag_is_active(&drag_state) {
        return;
    }

    let (delta, current_offset, viewport_extent) = match axis {
        TabDropAxis::Horizontal => (
            state::auto_scroll_delta(viewport_rect, drag_state.current_pos, axis),
            scroll_state.offset.x,
            viewport_rect.width(),
        ),
        TabDropAxis::Vertical => (
            state::auto_scroll_delta(viewport_rect, drag_state.current_pos, axis),
            scroll_state.offset.y,
            viewport_rect.height(),
        ),
    };
    if delta.abs() <= f32::EPSILON {
        return;
    }

    let max_offset = (content_extent - viewport_extent).max(0.0);
    let next_offset = (current_offset + delta).clamp(0.0, max_offset);
    if (next_offset - current_offset).abs() <= f32::EPSILON {
        return;
    }

    let mut next_state = *scroll_state;
    match axis {
        TabDropAxis::Horizontal => next_state.offset.x = next_offset,
        TabDropAxis::Vertical => next_state.offset.y = next_offset,
    }
    next_state.store(ctx, scroll_area_id);
    ctx.request_repaint();
}
