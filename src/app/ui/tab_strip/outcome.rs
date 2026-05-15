use super::TabStripOutcome;
use crate::app::app_state::{
    ScratchpadApp, settings_controller,
    workspace::{accessors as workspace_accessors, display_tabs},
};
use crate::app::commands::{AppCommand, SettingsCommand, WorkspaceCommand};

pub(crate) fn apply_tab_outcome(app: &mut ScratchpadApp, outcome: TabStripOutcome) {
    apply_workspace_slot_command(app, outcome.activated_tab, |index| {
        AppCommand::Workspace(WorkspaceCommand::ActivateTab { index })
    });
    if let Some(index) = outcome
        .rename_requested_tab
        .and_then(|slot_index| display_tabs::workspace_index_for_slot(app, slot_index))
    {
        app.handle_command(AppCommand::Workspace(WorkspaceCommand::ActivateTab {
            index,
        }));
        workspace_accessors::begin_tab_rename(app, index);
    }
    if outcome.activate_settings {
        settings_controller::open_settings_preserving_tab_selection(app);
    }

    apply_workspace_slot_command(app, outcome.close_requested_tab, |index| {
        AppCommand::Workspace(WorkspaceCommand::RequestCloseTab { index })
    });
    if outcome.close_settings {
        app.handle_command(AppCommand::Settings(SettingsCommand::CloseSettings));
    }

    apply_workspace_slot_command(app, outcome.promote_all_files_tab, |index| {
        AppCommand::Workspace(WorkspaceCommand::PromoteTabFilesToTabs { index })
    });
    apply_tab_reordering(app, &outcome);
    apply_tab_combining(app, &outcome);
    clear_consumed_scroll_request(app, &outcome);
}

fn apply_tab_reordering(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if let Some((from_indices, to_index)) = &outcome.reordered_tab_group {
        let _ = display_tabs::reorder_display_tab_group(app, from_indices.clone(), *to_index);
        display_tabs::clear_tab_selection(app);
        return;
    }

    if let Some((from_index, to_index)) = outcome.reordered_tabs {
        app.handle_command(AppCommand::Workspace(WorkspaceCommand::ReorderDisplayTab {
            from_index,
            to_index,
        }));
        display_tabs::clear_tab_selection(app);
    }
}

fn apply_tab_combining(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if let Some((source_indices, target_index)) = &outcome.combined_tab_group {
        if let Some((workspace_sources, workspace_target)) =
            resolve_group_combine_targets(app, source_indices, *target_index)
        {
            app.handle_command(AppCommand::Workspace(
                WorkspaceCommand::CombineTabsIntoTab {
                    source_indices: workspace_sources,
                    target_index: workspace_target,
                },
            ));
        }
        crate::app::app_state::workspace::display_tabs::clear_tab_selection(app);
        return;
    }

    if let Some((source_index, target_index)) = outcome.combined_tabs
        && let (Some(source_index), Some(target_index)) = (
            crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(
                app,
                source_index,
            ),
            crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(
                app,
                target_index,
            ),
        )
    {
        app.handle_command(AppCommand::Workspace(WorkspaceCommand::CombineTabIntoTab {
            source_index,
            target_index,
        }));
        crate::app::app_state::workspace::display_tabs::clear_tab_selection(app);
    }
}

fn resolve_group_combine_targets(
    app: &ScratchpadApp,
    source_indices: &[usize],
    target_index: usize,
) -> Option<(Vec<usize>, usize)> {
    let workspace_sources = source_indices
        .iter()
        .filter_map(|slot_index| {
            crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(
                app,
                *slot_index,
            )
        })
        .collect::<Vec<_>>();

    if let Some(workspace_target) =
        crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(app, target_index)
    {
        return (!workspace_sources.is_empty()).then_some((workspace_sources, workspace_target));
    }

    let (&workspace_target, remaining_sources) = workspace_sources.split_first()?;
    (!remaining_sources.is_empty()).then_some((remaining_sources.to_vec(), workspace_target))
}

fn apply_workspace_slot_command(
    app: &mut ScratchpadApp,
    slot_index: Option<usize>,
    command: impl FnOnce(usize) -> AppCommand,
) {
    if let Some(index) = slot_index.and_then(|slot_index| {
        crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(app, slot_index)
    }) {
        app.handle_command(command(index));
    }
}

fn clear_consumed_scroll_request(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if outcome.consumed_scroll_request {
        app.tab_manager.pending_scroll_to_active = false;
    }
}
