use super::{paint, state};
use crate::app::app_state::{ScratchpadApp, workspace::display_tabs};
use crate::app::commands::{AppCommand, WorkspaceCommand};
use eframe::egui;

/// Use the same workspace-combine operation as dropping onto another tab.
pub(crate) fn handle_editor_drop(ui: &egui::Ui, app: &mut ScratchpadApp, editor_rect: egui::Rect) {
    let Some(drag) = state::update_current_tab_drag(ui) else {
        return;
    };
    if !state::drag_is_active(&drag) || !ui.rect_contains_pointer(editor_rect) {
        return;
    }

    let target_index = app.tab_manager.active_tab_index;
    let target_slot = display_tabs::slot_for_workspace_index(app, target_index);
    let source_indices = editor_combine_sources(app, &drag.dragged_indices, target_slot);
    if source_indices.is_empty() {
        return;
    }

    paint::paint_combine_target(ui.ctx(), editor_rect);
    if ui.input(|input| input.pointer.primary_down()) {
        return;
    }

    state::clear_tab_drag_state(ui);
    let command = if source_indices.len() == 1 {
        WorkspaceCommand::CombineTabIntoTab {
            source_index: source_indices[0],
            target_index,
        }
    } else {
        WorkspaceCommand::CombineTabsIntoTab {
            source_indices,
            target_index,
        }
    };
    crate::app::commands::handle_command(app, AppCommand::Workspace(command));
    display_tabs::clear_tab_selection(app);
}

fn editor_combine_sources(
    app: &ScratchpadApp,
    dragged_slots: &[usize],
    target_slot: usize,
) -> Vec<usize> {
    // Match tab-strip behavior: a selection containing the target cannot be combined with itself.
    if dragged_slots.contains(&target_slot) {
        return Vec::new();
    }
    dragged_slots
        .iter()
        .filter_map(|slot| display_tabs::workspace_index_for_slot(app, *slot))
        .collect()
}
