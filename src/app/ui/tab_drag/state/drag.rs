use super::{TAB_DRAG_THRESHOLD, TabDragState};
use crate::app::ui::widget_ids;
use eframe::egui;

pub(crate) fn begin_tab_drag_if_needed(
    ui: &egui::Ui,
    index: usize,
    dragged_indices: &[usize],
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
        let dragged_indices = collected_dragged_indices(dragged_indices, index);
        data.insert_temp(
            tab_drag_state_id(),
            TabDragState {
                source_index: index,
                dragged_indices: dragged_indices.clone(),
                start_pos: pointer_pos,
                current_pos: pointer_pos,
            },
        );
    });
}

pub(crate) fn active_drag_sources_for_context(ctx: &egui::Context) -> Vec<usize> {
    let Some(drag_state) = current_tab_drag_state_for_context(ctx) else {
        return Vec::new();
    };
    if !drag_is_active(&drag_state) {
        return Vec::new();
    }
    drag_state.dragged_indices
}

pub(crate) fn is_drag_active_for_context(ctx: &egui::Context) -> bool {
    current_tab_drag_state_for_context(ctx)
        .as_ref()
        .is_some_and(drag_is_active)
}

pub(crate) fn update_current_tab_drag(ui: &egui::Ui) -> Option<TabDragState> {
    let mut drag_state = current_tab_drag_state(ui)?;

    if let Some(pointer_pos) = ui.input(|input| input.pointer.latest_pos()) {
        drag_state.current_pos = pointer_pos;
        ui.ctx().data_mut(|data| {
            data.insert_temp(tab_drag_state_id(), drag_state.clone());
        });
    }

    Some(drag_state)
}

pub(crate) fn current_tab_drag_state_for_context(ctx: &egui::Context) -> Option<TabDragState> {
    ctx.data(|data| data.get_temp::<TabDragState>(tab_drag_state_id()))
}

pub(crate) fn drag_is_active(drag_state: &TabDragState) -> bool {
    drag_state.start_pos.distance(drag_state.current_pos) >= TAB_DRAG_THRESHOLD
}

pub(crate) fn clear_tab_drag_state(ui: &egui::Ui) {
    ui.ctx().data_mut(|data| {
        data.remove::<TabDragState>(tab_drag_state_id());
    });
}

pub(super) fn collected_dragged_indices(dragged_indices: &[usize], index: usize) -> Vec<usize> {
    if dragged_indices.is_empty() {
        vec![index]
    } else {
        dragged_indices.to_vec()
    }
}

fn tab_drag_state_id() -> egui::Id {
    widget_ids::global("tab_strip_drag_state")
}

fn current_tab_drag_state(ui: &egui::Ui) -> Option<TabDragState> {
    ui.ctx()
        .data(|data| data.get_temp::<TabDragState>(tab_drag_state_id()))
}

#[cfg(test)]
mod tests {
    use super::collected_dragged_indices;

    #[test]
    fn collected_dragged_indices_preserves_multi_item_selection() {
        let dragged_indices = (0..24).collect::<Vec<_>>();

        assert_eq!(
            collected_dragged_indices(&dragged_indices, 5),
            dragged_indices
        );
    }

    #[test]
    fn collected_dragged_indices_falls_back_to_source_index() {
        assert_eq!(collected_dragged_indices(&[], 7), vec![7]);
    }
}
