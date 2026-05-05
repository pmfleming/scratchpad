use super::{SplitHandleDragState, TileAction};
use crate::app::domain::{SplitPath, ViewId};
use crate::app::ui::widget_ids;
use eframe::egui;

pub fn split_drag_active_for_context(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(global_split_drag_state_id()))
        .unwrap_or(false)
}

pub fn split_drag_active(ui: &egui::Ui, id: egui::Id) -> bool {
    split_drag_state(ui, id).is_some()
}

pub fn split_drag_state_id(pane_path: &SplitPath) -> egui::Id {
    widget_ids::root_id(("split_handle_drag", pane_path))
}

pub fn handle_split_interaction(
    ui: &mut egui::Ui,
    response: &egui::Response,
    id: egui::Id,
    tile_rect: egui::Rect,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) -> Option<SplitHandleDragState> {
    begin_split_drag_if_needed(ui, response, id);
    update_split_drag_state(ui, id, tile_rect, view_id, actions)
}

fn split_drag_state(ui: &egui::Ui, split_drag_state_id: egui::Id) -> Option<SplitHandleDragState> {
    ui.ctx()
        .data(|data| data.get_temp::<SplitHandleDragState>(split_drag_state_id))
}

fn begin_split_drag_if_needed(
    ui: &egui::Ui,
    split_response: &egui::Response,
    split_drag_state_id: egui::Id,
) {
    if split_response.hovered()
        && ui.input(|input| input.pointer.primary_pressed())
        && let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos())
    {
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                split_drag_state_id,
                SplitHandleDragState {
                    start_pos: pointer_pos,
                    current_pos: pointer_pos,
                },
            );
        });
        mark_global_split_drag_active(ui);
    }
}

fn update_split_drag_state(
    ui: &egui::Ui,
    split_drag_state_id: egui::Id,
    tile_rect: egui::Rect,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) -> Option<SplitHandleDragState> {
    let state = split_drag_state(ui, split_drag_state_id)?;

    if ui.input(|input| input.pointer.primary_down()) {
        return refresh_split_drag_state(ui, split_drag_state_id, state);
    }

    clear_split_drag_state(ui, split_drag_state_id);
    commit_split_drag_action(tile_rect, state, view_id, actions);
    None
}

fn refresh_split_drag_state(
    ui: &egui::Ui,
    split_drag_state_id: egui::Id,
    mut state: SplitHandleDragState,
) -> Option<SplitHandleDragState> {
    let pointer_pos = ui.input(|input| input.pointer.latest_pos())?;
    state.current_pos = pointer_pos;
    ui.ctx().data_mut(|data| {
        data.insert_temp(split_drag_state_id, state);
    });
    mark_global_split_drag_active(ui);
    Some(state)
}

fn clear_split_drag_state(ui: &egui::Ui, split_drag_state_id: egui::Id) {
    ui.ctx().data_mut(|data| {
        data.remove::<SplitHandleDragState>(split_drag_state_id);
        data.remove::<bool>(global_split_drag_state_id());
    });
}

fn mark_global_split_drag_active(ui: &egui::Ui) {
    ui.ctx()
        .data_mut(|data| data.insert_temp(global_split_drag_state_id(), true));
}

fn global_split_drag_state_id() -> egui::Id {
    widget_ids::ctx_key("tile_split_drag_active")
}

fn commit_split_drag_action(
    tile_rect: egui::Rect,
    state: SplitHandleDragState,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) {
    if let Some((axis, new_view_first, ratio)) =
        super::split_preview_spec(tile_rect, state.start_pos, state.current_pos)
    {
        actions.push(TileAction::Activate(view_id));
        actions.push(TileAction::Split {
            axis,
            new_view_first,
            ratio,
        });
    }
}
