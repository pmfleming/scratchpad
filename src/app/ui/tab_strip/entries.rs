mod horizontal;
mod shared;
mod vertical;

use super::{HeaderLayout, TabStripOutcome};
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::WorkspaceTab;
use crate::app::ui::tab_drag::{self, TabDropZone};
use eframe::egui;
use std::collections::HashMap;

type DuplicateNameCounts = HashMap<String, usize>;

pub(crate) fn duplicate_name_counts(tabs: &[WorkspaceTab]) -> DuplicateNameCounts {
    let mut counts = HashMap::with_capacity(tabs.len());
    for tab in tabs {
        *counts.entry(tab.buffer.name.clone()).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn show_vertical_tab_region(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
) -> TabStripOutcome {
    let duplicate_name_counts = duplicate_name_counts(app.tabs());
    vertical::show_vertical_tab_region(ui, app, &duplicate_name_counts)
}

pub(crate) fn show_tab_region(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
) -> TabStripOutcome {
    let duplicate_name_counts = duplicate_name_counts(app.tabs());
    horizontal::show_tab_region(ctx, ui, app, layout, &duplicate_name_counts)
}

fn apply_tab_drag_feedback(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    drop_zones: &[TabDropZone],
    outcome: &mut TabStripOutcome,
) {
    update_reordered_tabs(ui, app.total_tab_slots(), drop_zones, outcome);
    tab_drag::paint_dragged_tab_ghost(ui.ctx(), app);
}

fn update_reordered_tabs(
    ui: &mut egui::Ui,
    tab_count: usize,
    drop_zones: &[TabDropZone],
    outcome: &mut TabStripOutcome,
) {
    if let Some(commit) = tab_drag::update_tab_drag(ui, drop_zones, tab_count) {
        match commit {
            tab_drag::TabDragCommit::Reorder {
                from_index,
                to_index,
            } => outcome.reordered_tabs = Some((from_index, to_index)),
            tab_drag::TabDragCommit::ReorderGroup {
                from_indices,
                to_index,
            } => outcome.reordered_tab_group = Some((from_indices, to_index)),
            tab_drag::TabDragCommit::Combine {
                source_index,
                target_index,
            } => outcome.combined_tabs = Some((source_index, target_index)),
            tab_drag::TabDragCommit::CombineGroup {
                source_indices,
                target_index,
            } => outcome.combined_tab_group = Some((source_indices, target_index)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::shared::apply_settings_tab_interaction;
    use super::shared::handle_settings_tab_click;
    use crate::app::app_state::ScratchpadApp;
    use crate::app::commands::AppCommand;
    use crate::app::domain::WorkspaceTab;
    use crate::app::services::session_store::SessionStore;
    use crate::app::ui::tab_strip::TabStripOutcome;
    use eframe::egui::Modifiers;

    fn test_app() -> ScratchpadApp {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());
        ScratchpadApp::with_session_store(session_store)
    }

    fn app_with_settings_between_tabs() -> ScratchpadApp {
        let mut app = test_app();
        app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
        app.append_tab(WorkspaceTab::untitled());
        app.tabs_mut()[1].buffer.name = "two.txt".to_owned();
        app.handle_command(AppCommand::OpenSettings);
        app.handle_command(AppCommand::ReorderDisplayTab {
            from_index: 2,
            to_index: 1,
        });
        app
    }

    #[test]
    fn settings_tab_close_gesture_closes_settings_surface() {
        let mut outcome = TabStripOutcome::default();

        apply_settings_tab_interaction(&mut outcome, true, true, false);

        assert!(outcome.close_settings);
        assert!(!outcome.activate_settings);
        assert!(outcome.close_requested_tab.is_none());
    }

    #[test]
    fn settings_tab_close_gesture_closes_unfocused_settings_surface() {
        let mut outcome = TabStripOutcome::default();

        apply_settings_tab_interaction(&mut outcome, false, true, false);

        assert!(outcome.close_settings);
        assert!(!outcome.activate_settings);
        assert!(outcome.close_requested_tab.is_none());
    }

    #[test]
    fn clicking_settings_tab_activates_settings_surface() {
        let mut outcome = TabStripOutcome::default();

        apply_settings_tab_interaction(&mut outcome, false, false, true);

        assert!(outcome.activate_settings);
        assert!(!outcome.close_settings);
    }

    #[test]
    fn ctrl_clicking_settings_tab_toggles_selection_without_activation() {
        let mut app = app_with_settings_between_tabs();

        let activate = handle_settings_tab_click(
            &mut app,
            1,
            Modifiers {
                ctrl: true,
                ..Default::default()
            },
        );

        assert!(app.tab_slot_selected(1));
        assert!(!activate);
    }

    #[test]
    fn plain_clicking_settings_tab_selects_and_activates() {
        let mut app = app_with_settings_between_tabs();

        let activate = handle_settings_tab_click(&mut app, 1, Modifiers::default());

        assert!(app.tab_slot_selected(1));
        assert!(activate);
    }
}
