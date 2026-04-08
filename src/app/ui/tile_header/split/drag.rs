use super::{SplitHandleDragState, TileAction, ViewId};
use eframe::egui;

pub fn split_drag_active(ui: &egui::Ui, id: egui::Id) -> bool {
    split_drag_state(ui, id).is_some()
}

pub fn split_drag_state_id(ui: &egui::Ui, tab_index: usize, view_id: ViewId) -> egui::Id {
    ui.make_persistent_id(("split_handle_drag", tab_index, view_id))
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
    Some(state)
}

fn clear_split_drag_state(ui: &egui::Ui, split_drag_state_id: egui::Id) {
    ui.ctx().data_mut(|data| {
        data.remove::<SplitHandleDragState>(split_drag_state_id);
    });
}

fn commit_split_drag_action(
    tile_rect: egui::Rect,
    state: SplitHandleDragState,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) {
    if let Some((axis, new_view_first, ratio)) =
        super::split_preview_spec(tile_rect, state.current_pos - state.start_pos)
    {
        actions.push(TileAction::Activate(view_id));
        actions.push(TileAction::Split {
            axis,
            new_view_first,
            ratio,
        });
    }
}
