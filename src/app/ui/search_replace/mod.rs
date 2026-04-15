mod controls;
mod results;
mod state;

use crate::app::app_state::{ScratchpadApp, SearchFocusTarget};
use crate::app::commands::AppCommand;
use crate::app::theme::{action_bg, border};
use eframe::egui;
use state::{SearchStripActions, SearchStripState};

pub(crate) fn show_search_strip(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if !app.search_open() {
        return;
    }

    let mut state = SearchStripState::from_app(app);
    let mut actions = SearchStripActions::default();
    let find_input_id = ui.make_persistent_id("search_find_input");
    let replace_input_id = ui.make_persistent_id("search_replace_input");

    egui::Frame::NONE
        .fill(action_bg(ui))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            controls::show_search_controls(
                ui,
                &mut state,
                &mut actions,
                find_input_id,
                replace_input_id,
            );
            results::show_search_results(ui, &state, &mut actions);
        });

    apply_search_inputs(app, &state);
    if actions.close_requested {
        app.handle_command(AppCommand::CloseSearch);
        return;
    }

    dispatch_search_actions(app, state.target_focus(), actions);
}

fn apply_search_inputs(app: &mut ScratchpadApp, state: &SearchStripState) {
    if state.query != app.search_query() {
        app.set_search_query(state.query.clone());
    }
    if state.replacement != app.search_replacement() {
        app.set_search_replacement(state.replacement.clone());
    }
    if state.scope != app.search_scope() {
        app.set_search_scope(state.scope);
    }
    if state.match_case != app.search_match_case() {
        app.set_search_match_case(state.match_case);
    }
    if state.whole_word != app.search_whole_word() {
        app.set_search_whole_word(state.whole_word);
    }
}

fn dispatch_search_actions(
    app: &mut ScratchpadApp,
    target_focus: SearchFocusTarget,
    actions: SearchStripActions,
) {
    if actions.previous_requested {
        app.request_search_focus(target_focus);
        app.handle_command(AppCommand::PreviousSearchMatch);
    }
    if actions.next_requested {
        app.request_search_focus(target_focus);
        app.handle_command(AppCommand::NextSearchMatch);
    }
    if actions.replace_current_requested {
        app.request_search_focus(target_focus);
        app.handle_command(AppCommand::ReplaceCurrentMatch);
    }
    if actions.replace_all_requested {
        app.request_search_focus(target_focus);
        app.handle_command(AppCommand::ReplaceAllMatches);
    }
    if let Some(match_index) = actions.selected_match_index {
        app.request_search_focus(target_focus);
        app.activate_search_match_at(match_index);
    }
}