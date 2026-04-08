use super::{TAB_DRAG_THRESHOLD, TabDragState};
use eframe::egui;

pub(crate) fn begin_tab_drag_if_needed(
    ui: &egui::Ui,
    index: usize,
    tab_response: &egui::Response,
    close_response: &egui::Response,
) {
    if current_tab_drag_state(ui).is_some() || close_response.hovered() {
        return;
    }

    if !tab_response.hovered() || !ui.input(|input| input.pointer.primary_pressed()) {
        return;
    }

    let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };

    ui.ctx().data_mut(|data| {
        data.insert_temp(
            tab_drag_state_id(),
            TabDragState {
                source_index: index,
                start_pos: pointer_pos,
                current_pos: pointer_pos,
            },
        );
    });
}

pub(crate) fn active_drag_source_for_context(ctx: &egui::Context) -> Option<usize> {
    let drag_state = current_tab_drag_state_for_context(ctx)?;
    drag_is_active(drag_state).then_some(drag_state.source_index)
}

pub(crate) fn has_tab_drag_for_context(ctx: &egui::Context) -> bool {
    current_tab_drag_state_for_context(ctx).is_some()
}

pub(crate) fn is_drag_active_for_context(ctx: &egui::Context) -> bool {
    current_tab_drag_state_for_context(ctx).is_some_and(drag_is_active)
}

pub(crate) fn update_current_tab_drag(ui: &egui::Ui) -> Option<TabDragState> {
    let mut drag_state = current_tab_drag_state(ui)?;

    if let Some(pointer_pos) = ui.input(|input| input.pointer.latest_pos()) {
        drag_state.current_pos = pointer_pos;
        ui.ctx().data_mut(|data| {
            data.insert_temp(tab_drag_state_id(), drag_state);
        });
    }

    Some(drag_state)
}

pub(crate) fn current_tab_drag_state_for_context(ctx: &egui::Context) -> Option<TabDragState> {
    ctx.data(|data| data.get_temp::<TabDragState>(tab_drag_state_id()))
}

pub(crate) fn drag_is_active(drag_state: TabDragState) -> bool {
    drag_state.start_pos.distance(drag_state.current_pos) >= TAB_DRAG_THRESHOLD
}

pub(crate) fn clear_tab_drag_state(ui: &egui::Ui) {
    ui.ctx().data_mut(|data| {
        data.remove::<TabDragState>(tab_drag_state_id());
    });
}

fn tab_drag_state_id() -> egui::Id {
    egui::Id::new("tab_strip_drag_state")
}

fn current_tab_drag_state(ui: &egui::Ui) -> Option<TabDragState> {
    ui.ctx()
        .data(|data| data.get_temp::<TabDragState>(tab_drag_state_id()))
}
