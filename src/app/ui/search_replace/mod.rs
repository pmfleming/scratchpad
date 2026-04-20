use super::callout;
mod controls;
mod results;
mod state;

use crate::app::app_state::{ScratchpadApp, SearchFocusTarget};
use crate::app::commands::AppCommand;
use crate::app::ui::settings;
use eframe::egui;
use state::{SearchStripActions, SearchStripState};

pub(crate) const SEARCH_DIALOG_WIDTH: f32 = 620.0;
const SEARCH_TITLE_SIZE: f32 = 24.0;

pub(crate) fn show_search_strip(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if !app.search_open() {
        return;
    }

    let mut state = SearchStripState::from_app(app);
    let mut actions = SearchStripActions::default();
    let find_input_id = ui.make_persistent_id("search_find_input");
    let replace_input_id = ui.make_persistent_id("search_replace_input");

    let default_pos = callout::centered_position(ui.ctx(), egui::vec2(SEARCH_DIALOG_WIDTH, 460.0));

    egui::Area::new(egui::Id::new("search_dialog_overlay"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(true)
        .default_pos(default_pos)
        .show(ui.ctx(), |ui| {
            ui.set_width(SEARCH_DIALOG_WIDTH);
            ui.set_min_width(SEARCH_DIALOG_WIDTH);
            callout::frame(ui).show(ui, |ui| {
                settings::apply_dialog_typography(ui);
                callout::apply_spacing(ui);
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);

                if render_search_header(ui) {
                    actions.close_requested = true;
                }
                ui.add_space(4.0);

                controls::show_search_controls(
                    ui,
                    &mut state,
                    &mut actions,
                    find_input_id,
                    replace_input_id,
                );
                results::show_search_results(ui, &state, &mut actions);
            });
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
    if state.replace_open != app.search_replace_open() {
        app.set_search_replace_open(state.replace_open);
    }
    if state.scope != app.search_scope() {
        app.set_search_scope(state.scope);
    }
    if state.mode != app.search_mode() {
        app.set_search_mode(state.mode);
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
    if actions.undo_requested {
        app.request_search_focus(target_focus);
        app.handle_command(AppCommand::UndoActiveBufferTextOperation);
    }
    if actions.redo_requested {
        app.request_search_focus(target_focus);
        app.handle_command(AppCommand::RedoActiveBufferTextOperation);
    }
    if actions.replace_current_requested {
        app.request_search_focus(target_focus);
        app.handle_command(AppCommand::ReplaceCurrentMatch);
    }
    if actions.replace_all_requested {
        app.request_search_focus(target_focus);
        app.handle_command(AppCommand::ReplaceAllMatches);
    }
    if let Some(match_index) = actions.selected_match_index
        && app.activate_search_match_at(match_index)
    {
        app.request_focus_for_active_view();
    }
}

fn render_search_header(ui: &mut egui::Ui) -> bool {
    callout::header_row(ui, "Close search", |ui| {
        ui.label(
            egui::RichText::new("Search")
                .size(SEARCH_TITLE_SIZE)
                .color(callout::text(ui)),
        );
    })
}
