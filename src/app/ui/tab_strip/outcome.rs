use super::TabStripOutcome;
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;

pub(crate) fn apply_tab_outcome(app: &mut ScratchpadApp, outcome: TabStripOutcome) {
    apply_workspace_slot_command(app, outcome.activated_tab, |index| {
        AppCommand::ActivateTab { index }
    });
    if let Some(index) = outcome
        .rename_requested_tab
        .and_then(|slot_index| app.workspace_index_for_slot(slot_index))
    {
        app.handle_command(AppCommand::ActivateTab { index });
        app.begin_tab_rename(index);
    }
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
        if let Some((workspace_sources, workspace_target)) =
            resolve_group_combine_targets(app, source_indices, *target_index)
        {
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

fn resolve_group_combine_targets(
    app: &ScratchpadApp,
    source_indices: &[usize],
    target_index: usize,
) -> Option<(Vec<usize>, usize)> {
    let workspace_sources = source_indices
        .iter()
        .filter_map(|slot_index| app.workspace_index_for_slot(*slot_index))
        .collect::<Vec<_>>();

    if let Some(workspace_target) = app.workspace_index_for_slot(target_index) {
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
