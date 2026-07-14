use super::consume_app_shortcut;
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::domain::ViewId;
use crate::app::shortcut_keymap::ShortcutAction;
use eframe::egui;

pub(super) fn handle_region_traversal_shortcut(
    app: &mut ScratchpadApp,
    ctx: &egui::Context,
) -> bool {
    let direction = if consume_app_shortcut(app, ctx, ShortcutAction::TraverseRegionForward) {
        Some(1)
    } else if consume_app_shortcut(app, ctx, ShortcutAction::TraverseRegionBackward) {
        Some(-1)
    } else {
        None
    };
    let Some(direction) = direction else {
        return false;
    };
    let Some(next_view_id) = next_view_for_region_traversal(app, direction) else {
        return true;
    };

    crate::app::commands::handle_command(
        app,
        AppCommand::Workspace(WorkspaceCommand::ActivateView {
            view_id: next_view_id,
        }),
    );
    true
}

fn next_view_for_region_traversal(app: &ScratchpadApp, direction: i32) -> Option<ViewId> {
    let tab = app.tab_manager.active_tab()?;
    let ordered = tab.ordered_view_ids_in_layout_order();
    if ordered.len() <= 1 {
        return None;
    }

    let current = ordered
        .iter()
        .position(|view_id| *view_id == tab.layout.active_view_id)
        .unwrap_or(0);
    let next = next_region_index(current, ordered.len(), direction)?;
    ordered.get(next).copied()
}

fn next_region_index(current: usize, len: usize, direction: i32) -> Option<usize> {
    if len <= 1 {
        return None;
    }
    Some(if direction < 0 {
        current.checked_sub(1).unwrap_or(len - 1)
    } else {
        (current + 1) % len
    })
}

#[cfg(test)]
mod tests {
    use super::next_region_index;

    #[test]
    fn f6_traversal_wraps_forward() {
        assert_eq!(next_region_index(2, 3, 1), Some(0));
    }

    #[test]
    fn shift_f6_traversal_wraps_backward() {
        assert_eq!(next_region_index(0, 3, -1), Some(2));
    }
}
