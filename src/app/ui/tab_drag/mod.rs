use crate::app::app_state::ScratchpadApp;
use eframe::egui;
use std::collections::HashMap;

mod editor;
mod paint;
mod state;

pub(crate) use editor::handle_editor_drop;

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

/// Clear unhandled releases only after both the tab strip and editor had a chance to accept them.
pub(crate) fn finish_drag_frame(ui: &egui::Ui) {
    if !ui.input(|input| input.pointer.primary_down()) {
        state::clear_tab_drag_state(ui);
    }
}

pub(crate) fn update_tab_drag(
    ui: &egui::Ui,
    zones: &[TabDropZone],
    total_tab_count: usize,
) -> Option<TabDragCommit> {
    let drag_state = state::update_current_tab_drag(ui)?;
    let allow_combine =
        drag_sources_allow_combine(zones, &drag_state.dragged_indices, drag_state.source_index);
    let drop_intent = state::drag_is_active(&drag_state)
        .then(|| state::locate_drop_intent(zones, drag_state.current_pos, allow_combine))
        .flatten();
    if let Some(drop_intent) = &drop_intent {
        paint_drop_intent(ui.ctx(), zones, drop_intent);
    }

    if ui.input(|input| input.pointer.primary_down()) {
        return None;
    }

    let drop_intent = drop_intent?;
    state::clear_tab_drag_state(ui);
    commit_for_drop(
        drop_intent,
        drag_state.source_index,
        drag_state.dragged_indices,
        total_tab_count,
    )
}

fn paint_drop_intent(ctx: &egui::Context, zones: &[TabDropZone], intent: &state::TabDropIntent) {
    match *intent {
        state::TabDropIntent::Reorder {
            zone_index,
            drop_slot,
        } => paint::paint_tab_reorder_marker(ctx, &zones[zone_index], drop_slot),
        state::TabDropIntent::Combine {
            zone_index,
            target_index,
        } => paint::paint_tab_combine_target(ctx, &zones[zone_index], target_index),
    }
}

/// The change a released drag makes; `None` when it would leave tabs unchanged.
fn commit_for_drop(
    intent: state::TabDropIntent,
    source_index: usize,
    dragged_indices: Vec<usize>,
    total_tab_count: usize,
) -> Option<TabDragCommit> {
    let group = dragged_indices.len() > 1;
    match intent {
        state::TabDropIntent::Reorder { drop_slot, .. } if group => {
            Some(TabDragCommit::ReorderGroup {
                from_indices: dragged_indices,
                to_index: drop_slot.min(total_tab_count),
            })
        }
        state::TabDropIntent::Reorder { drop_slot, .. } => {
            let to_index = state::resolve_drop_slot(source_index, drop_slot, total_tab_count);
            (to_index != source_index).then_some(TabDragCommit::Reorder {
                from_index: source_index,
                to_index,
            })
        }
        state::TabDropIntent::Combine { target_index, .. } if group => {
            (!dragged_indices.contains(&target_index)).then_some(TabDragCommit::CombineGroup {
                source_indices: dragged_indices,
                target_index,
            })
        }
        state::TabDropIntent::Combine { target_index, .. } => (target_index != source_index)
            .then_some(TabDragCommit::Combine {
                source_index,
                target_index,
            }),
    }
}

/// Combining is offered when the dragged tab, or any tab of a dragged group,
/// is a workspace tab (Settings cannot be combined).
fn drag_sources_allow_combine(
    zones: &[TabDropZone],
    dragged_indices: &[usize],
    source_index: usize,
) -> bool {
    let group = dragged_indices.len() > 1;
    zones
        .iter()
        .flat_map(|zone| zone.entries.iter())
        .filter(|entry| entry.combine_enabled)
        .any(|entry| {
            entry.index == source_index || (group && dragged_indices.contains(&entry.index))
        })
}

pub(crate) fn paint_dragged_tab_ghost(
    ctx: &egui::Context,
    app: &ScratchpadApp,
    duplicate_name_counts: &HashMap<String, usize>,
) {
    let Some(drag_state) = state::current_tab_drag_state_for_context(ctx) else {
        return;
    };
    if !state::drag_is_active(&drag_state) {
        return;
    }
    paint::paint_dragged_tab_ghost(ctx, app, drag_state, duplicate_name_counts);
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
            scroll_state.clamped_offset().x,
            viewport_rect.width(),
        ),
        TabDropAxis::Vertical => (
            state::auto_scroll_delta(viewport_rect, drag_state.current_pos, axis),
            scroll_state.clamped_offset().y,
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

    let mut offset = scroll_state.clamped_offset();
    match axis {
        TabDropAxis::Horizontal => offset.x = next_offset,
        TabDropAxis::Vertical => offset.y = next_offset,
    }
    crate::app::ui::widget_ids::request_scroll_offset(ctx, scroll_area_id, offset);
}

#[cfg(test)]
mod tests;
