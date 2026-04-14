use super::TabStripOutcome;
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;

pub(crate) fn apply_tab_outcome(app: &mut ScratchpadApp, outcome: TabStripOutcome) {
    apply_workspace_slot_command(app, outcome.activated_tab, |index| {
        AppCommand::ActivateTab { index }
    });
    if outcome.activate_settings {
        app.handle_command(AppCommand::OpenSettings);
    }

    apply_workspace_slot_command(app, outcome.close_requested_tab, |index| {
        AppCommand::RequestCloseTab { index }
    });
    if outcome.close_settings {
        app.handle_command(AppCommand::CloseSettings);
    }

    apply_workspace_slot_command(app, outcome.promote_all_files_tab, |index| {
        AppCommand::PromoteTabFilesToTabs { index }
    });
    apply_tab_reordering(app, &outcome);
    apply_tab_combining(app, &outcome);
    clear_consumed_scroll_request(app, &outcome);
}

fn apply_tab_reordering(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if let Some((from_indices, to_index)) = &outcome.reordered_tab_group {
        let _ = app.reorder_display_tab_group(from_indices.clone(), *to_index);
        app.clear_tab_selection();
        return;
    }

    if let Some((from_index, to_index)) = outcome.reordered_tabs {
        app.handle_command(AppCommand::ReorderDisplayTab {
            from_index,
            to_index,
        });
        app.clear_tab_selection();
    }
}

fn apply_tab_combining(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if let Some((source_indices, target_index)) = &outcome.combined_tab_group {
        let workspace_sources = source_indices
            .iter()
            .filter_map(|slot_index| app.workspace_index_for_slot(*slot_index))
            .collect::<Vec<_>>();
        if let Some(workspace_target) = app.workspace_index_for_slot(*target_index) {
            app.handle_command(AppCommand::CombineTabsIntoTab {
                source_indices: workspace_sources,
                target_index: workspace_target,
            });
        }
        app.clear_tab_selection();
        return;
    }

    if let Some((source_index, target_index)) = outcome.combined_tabs
        && let (Some(source_index), Some(target_index)) = (
            app.workspace_index_for_slot(source_index),
            app.workspace_index_for_slot(target_index),
        )
    {
        app.handle_command(AppCommand::CombineTabIntoTab {
            source_index,
            target_index,
        });
        app.clear_tab_selection();
    }
}

fn apply_workspace_slot_command(
    app: &mut ScratchpadApp,
    slot_index: Option<usize>,
    command: impl FnOnce(usize) -> AppCommand,
) {
    if let Some(index) = slot_index.and_then(|slot_index| app.workspace_index_for_slot(slot_index))
    {
        app.handle_command(command(index));
    }
}

fn clear_consumed_scroll_request(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if outcome.consumed_scroll_request {
        app.tab_manager_mut().pending_scroll_to_active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{TabStripOutcome, apply_tab_outcome};
    use crate::app::app_state::ScratchpadApp;
    use crate::app::commands::AppCommand;
    use crate::app::domain::{PendingAction, WorkspaceTab};
    use crate::app::services::session_store::SessionStore;

    fn test_app() -> ScratchpadApp {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());
        ScratchpadApp::with_session_store(session_store)
    }

    fn app_with_named_tabs(names: &[&str]) -> ScratchpadApp {
        let mut app = test_app();
        for (index, name) in names.iter().enumerate() {
            if index > 0 {
                app.append_tab(WorkspaceTab::untitled());
            }
            app.tabs_mut()[index].buffer.name = (*name).to_owned();
        }
        app
    }

    fn app_with_settings_between_tabs() -> ScratchpadApp {
        let mut app = app_with_named_tabs(&["one.txt", "two.txt", "three.txt"]);

        app.handle_command(AppCommand::OpenSettings);
        app.handle_command(AppCommand::ReorderDisplayTab {
            from_index: 3,
            to_index: 1,
        });

        app
    }

    #[test]
    fn activating_last_display_slot_targets_last_workspace_tab() {
        let mut app = app_with_settings_between_tabs();

        apply_tab_outcome(
            &mut app,
            TabStripOutcome {
                activated_tab: Some(3),
                ..Default::default()
            },
        );

        assert_eq!(app.active_tab_index(), 2);
        assert_eq!(
            app.tabs()[app.active_tab_index()].active_buffer().name,
            "three.txt"
        );
        assert!(!app.showing_settings());
    }

    #[test]
    fn closing_last_display_slot_targets_last_workspace_tab() {
        let mut app = app_with_settings_between_tabs();

        apply_tab_outcome(
            &mut app,
            TabStripOutcome {
                close_requested_tab: Some(3),
                ..Default::default()
            },
        );

        assert!(matches!(
            app.pending_action(),
            Some(PendingAction::CloseTab(2))
        ));
    }

    #[test]
    fn combining_last_display_slot_uses_workspace_indexes() {
        let mut app = app_with_settings_between_tabs();

        apply_tab_outcome(
            &mut app,
            TabStripOutcome {
                combined_tabs: Some((3, 0)),
                ..Default::default()
            },
        );

        assert_eq!(app.tabs().len(), 2);
        assert_eq!(app.active_tab_index(), 0);
        assert_eq!(app.tabs()[0].views.len(), 2);
    }

    #[test]
    fn reordering_a_group_of_display_slots_moves_them_together() {
        let mut app = app_with_named_tabs(&["one.txt", "two.txt", "three.txt", "four.txt"]);

        apply_tab_outcome(
            &mut app,
            TabStripOutcome {
                reordered_tab_group: Some((vec![1, 2], 4)),
                ..Default::default()
            },
        );

        let names = app
            .tabs()
            .iter()
            .map(|tab| tab.active_buffer().name.clone())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["one.txt", "four.txt", "two.txt", "three.txt"]);
    }

    #[test]
    fn combining_a_group_of_display_slots_merges_them_into_target_tab() {
        let mut app = app_with_named_tabs(&["one.txt", "two.txt", "three.txt", "four.txt"]);

        apply_tab_outcome(
            &mut app,
            TabStripOutcome {
                combined_tab_group: Some((vec![1, 2], 0)),
                ..Default::default()
            },
        );

        assert_eq!(app.tabs().len(), 2);
        assert_eq!(app.active_tab_index(), 0);
        assert_eq!(app.tabs()[0].views.len(), 3);
    }
}
