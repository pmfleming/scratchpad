use super::callout;
mod controls;
mod results;
mod state;

use crate::app::app_state::{ScratchpadApp, SearchFocusTarget};
use crate::app::commands::AppCommand;
use crate::app::ui::settings;
use crate::app::ui::widget_ids;
use eframe::egui;
use state::{SearchStripActions, SearchStripState};

pub(crate) const SEARCH_DIALOG_WIDTH: f32 = 620.0;
const SEARCH_DIALOG_HEIGHT: f32 = 520.0;
const SEARCH_TITLE_SIZE: f32 = 24.0;

pub(crate) fn show_search_strip(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if !app.state.search_state.open() {
        return;
    }

    let mut state = SearchStripState::from_app(app);
    let mut actions = SearchStripActions::default();
    let find_input_id = widget_ids::local(ui, "search_find_input");
    let replace_input_id = widget_ids::local(ui, "search_replace_input");

    let default_pos = callout::centered_position(
        ui.ctx(),
        egui::vec2(SEARCH_DIALOG_WIDTH, SEARCH_DIALOG_HEIGHT),
    );

    widget_ids::area("search_dialog_overlay")
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(true)
        .default_pos(default_pos)
        .show(ui.ctx(), |ui| {
            widget_ids::feature_scope(ui, "search_dialog", |ui| {
                ui.set_width(SEARCH_DIALOG_WIDTH);
                ui.set_min_width(SEARCH_DIALOG_WIDTH);
                ui.set_max_width(SEARCH_DIALOG_WIDTH);
                let inner = callout::frame(ui).show(ui, |ui| {
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
                    results::show_search_results(ui, app, &state, &mut actions);
                });
                callout::mark_scroll_blocker_if_hovered(ui.ctx(), &inner.response);
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
    if state.query != app.state.search_state.query() {
        app.handle_command(AppCommand::SetSearchQuery {
            query: state.query.clone(),
        });
    }
    if state.replacement != app.state.search_state.replacement() {
        app.handle_command(AppCommand::SetSearchReplacement {
            replacement: state.replacement.clone(),
        });
    }
    if state.replace_open != app.state.search_state.replace_open() {
        app.handle_command(AppCommand::SetSearchReplaceOpen {
            open: state.replace_open,
        });
    }
    if state.scope != app.state.search_state.scope() {
        app.handle_command(AppCommand::SetSearchScope { scope: state.scope });
    }
    if state.mode != app.state.search_state.mode() {
        app.handle_command(AppCommand::SetSearchMode { mode: state.mode });
    }
    if state.match_case != app.state.search_state.match_case() {
        app.handle_command(AppCommand::SetSearchMatchCase {
            enabled: state.match_case,
        });
    }
    if state.whole_word != app.state.search_state.whole_word() {
        app.handle_command(AppCommand::SetSearchWholeWord {
            enabled: state.whole_word,
        });
    }
}

fn dispatch_search_actions(
    app: &mut ScratchpadApp,
    target_focus: SearchFocusTarget,
    actions: SearchStripActions,
) {
    for (requested, command) in [
        (actions.previous_requested, AppCommand::PreviousSearchMatch),
        (actions.next_requested, AppCommand::NextSearchMatch),
        (
            actions.undo_requested,
            AppCommand::UndoActiveBufferTextOperation,
        ),
        (
            actions.redo_requested,
            AppCommand::RedoActiveBufferTextOperation,
        ),
        (
            actions.replace_current_requested,
            AppCommand::ReplaceCurrentMatch,
        ),
        (actions.replace_all_requested, AppCommand::ReplaceAllMatches),
    ] {
        dispatch_requested_command(app, target_focus, requested, command);
    }

    if let Some(match_index) = actions.focused_file_match_index {
        app.handle_command(AppCommand::FocusSearchResultFile { match_index });
    }
    if let Some(match_index) = actions.selected_match_index {
        app.handle_command(AppCommand::ActivateSearchMatch { match_index });
    }
}

fn dispatch_requested_command(
    app: &mut ScratchpadApp,
    target_focus: SearchFocusTarget,
    requested: bool,
    command: AppCommand,
) {
    if !requested {
        return;
    }

    app.request_search_focus(target_focus);
    app.handle_command(command);
}

fn render_search_header(ui: &mut egui::Ui) -> bool {
    callout::header_row(ui, "search_replace.header", "Close search", |ui| {
        ui.label(
            egui::RichText::new("Search")
                .size(SEARCH_TITLE_SIZE)
                .color(callout::text(ui)),
        );
    })
}
