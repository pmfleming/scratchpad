use super::TabStripOutcome;
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;

pub(crate) fn apply_tab_outcome(app: &mut ScratchpadApp, outcome: TabStripOutcome) {
    apply_tab_activation(app, &outcome);
    apply_tab_closing(app, &outcome);
    apply_tab_promotions(app, &outcome);
    apply_tab_reordering(app, &outcome);
    apply_tab_combining(app, &outcome);
    clear_consumed_scroll_request(app, &outcome);
}

fn apply_tab_activation(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if let Some(index) = outcome.activated_tab {
        if app.showing_settings() {
            app.handle_command(AppCommand::CloseSettings);
        }
        app.handle_command(AppCommand::ActivateTab { index });
    }

    if outcome.activate_settings {
        app.handle_command(AppCommand::OpenSettings);
    }
}

fn apply_tab_closing(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if let Some(index) = outcome.close_requested_tab {
        app.handle_command(AppCommand::RequestCloseTab { index });
    }

    if outcome.close_settings {
        app.handle_command(AppCommand::CloseSettings);
    }
}

fn apply_tab_promotions(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if let Some(index) = outcome.promote_all_files_tab {
        app.handle_command(AppCommand::PromoteTabFilesToTabs { index });
    }
}

fn apply_tab_reordering(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if let Some((from_index, to_index)) = outcome.reordered_tabs {
        app.handle_command(AppCommand::ReorderDisplayTab {
            from_index,
            to_index,
        });
    }
}

fn apply_tab_combining(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if let Some((source_index, target_index)) = outcome.combined_tabs {
        app.handle_command(AppCommand::CombineTabIntoTab {
            source_index,
            target_index,
        });
    }
}

fn clear_consumed_scroll_request(app: &mut ScratchpadApp, outcome: &TabStripOutcome) {
    if outcome.consumed_scroll_request {
        app.tab_manager_mut().pending_scroll_to_active = false;
    }
}
