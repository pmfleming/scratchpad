mod controls;
mod footer;
mod header;
mod result_model;
mod results;
mod state;
mod summary;

use crate::app::app_state::{
    ScratchpadApp, SearchFocusTarget, search_runtime, search_state::SearchState,
};
use crate::app::commands::{AppCommand, EditCommand, SearchCommand};
use crate::app::ui::{panel_style, panel_style::window, widget_ids};
use eframe::egui;
use state::{SearchStripActions, SearchStripState};

const SEARCH_DEFAULT_SIZE: egui::Vec2 = egui::vec2(480.0, 600.0);
const SEARCH_MIN_HEIGHT: f32 = 320.0;

pub(crate) fn show_search_strip(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if !app.state.search_state.open() {
        return;
    }

    let mut state = SearchStripState::from_app(app);
    let mut actions = SearchStripActions::default();
    let find_input_id = widget_ids::local(ui, "search_find_input");
    let replace_input_id = widget_ids::local(ui, "search_replace_input");
    let ctx = ui.ctx().clone();
    let layer = widget_ids::area_layer_id(egui::Order::Foreground, "search_window");
    let outside = window::OutsideClick::before_show(&ctx, app, layer);
    let frame = panel_style::window_frame(&ctx, app.state.app_settings.editor_background_color());
    // Window sizes include the frame, so add its margin to the header content width.
    let min_size = egui::vec2(
        header::WIDTH.get(&ctx) + frame.total_margin().sum().x,
        SEARCH_MIN_HEIGHT,
    );

    let response = window::resizable(&ctx, "Search", layer, frame, SEARCH_DEFAULT_SIZE, min_size)
        .show(&ctx, |ui| {
            window::contents(ui, "search_dialog", frame.fill, |ui| {
                let show_close = outside.needs_close_button();
                actions.close_requested |= header::show(ui, &mut state, &mut actions, show_close);
                controls::show_search_controls(
                    ui,
                    &mut state,
                    &mut actions,
                    find_input_id,
                    replace_input_id,
                );
                summary::show(ui, app, &state, &mut actions);
                footer::panel(ui, app, &state, &mut actions);
                results::show_search_results(ui, app, &mut state, &mut actions);
            });
        });
    if let Some(response) = response {
        // Ignore the opening click and interactions with the panel's menus.
        actions.close_requested |= outside.after_show(&ctx, app, &response.response)
            && !app.state.dialogs.any_modal_open();
    }

    apply_search_inputs(app, &state);
    if actions.settings_requested {
        crate::app::app_state::search_controller::close_search(app);
        crate::app::app_state::settings_controller::open_settings(app);
        ui.data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("settings.search"),
                "search result limit".to_owned(),
            )
        });
        return;
    }
    apply_panel_actions(app, &state, &actions);
    apply_list_navigation(app, &actions);
    if actions.close_requested {
        crate::app::commands::handle_command(app, AppCommand::Search(SearchCommand::Close));
        return;
    }

    dispatch_search_actions(app, state.target_focus(), actions);
}

fn apply_panel_actions(
    app: &mut ScratchpadApp,
    state: &SearchStripState,
    actions: &SearchStripActions,
) {
    use crate::app::app_state::search_state::{api, recent, replace};
    if actions.recapture_selection {
        api::set_search_scope(app, crate::app::app_state::SearchScope::SelectionOnly);
    }
    if actions.cancel_review {
        app.state
            .search_state
            .replace
            .pending_replace_all_confirmation = None;
    }
    if let Some((id, files)) = actions.undo_all {
        replace::transaction::undo(app, id, files);
    }
    if let Some(index) = actions.recent_index
        && let Some(recent) = state.recent.get(index)
    {
        api::set_search_query(app, recent.query.clone());
        api::set_search_mode(
            app,
            if recent.regex {
                crate::app::services::search::SearchMode::Regex
            } else {
                crate::app::services::search::SearchMode::PlainText
            },
        );
        api::set_search_match_case(app, recent.match_case);
        api::set_search_whole_word(app, recent.whole_word);
    }
    if actions.next_requested || actions.previous_requested {
        recent::commit(app);
    }
}

/// Keyboard moves through results keep focus in the list; only Enter goes to the editor.
fn apply_list_navigation(app: &mut ScratchpadApp, actions: &SearchStripActions) {
    use crate::app::app_state::search_controller::activate_search_match_at;
    if let Some(index) = actions.preview_match_index {
        activate_search_match_at(app, index);
        search_runtime::request_search_focus(app, SearchFocusTarget::Results);
    }
    if actions.focus_results {
        if app.state.search_state.active_match_index().is_none() {
            activate_search_match_at(app, 0);
        }
        search_runtime::request_search_focus(app, SearchFocusTarget::Results);
    }
    if actions.focus_find {
        search_runtime::request_search_focus(app, SearchFocusTarget::FindInput);
    }
}
/// A panel input that differs from the search state, as the command that applies it.
type InputChange = fn(&SearchStripState, &SearchState) -> Option<SearchCommand>;

/// Checked in order, each against the state the previous commands left behind.
const INPUT_CHANGES: [InputChange; 7] = [
    |strip, search| {
        (strip.query != search.query()).then(|| SearchCommand::SetSearchQuery {
            query: strip.query.clone(),
        })
    },
    |strip, search| {
        (strip.replacement != search.replacement()).then(|| SearchCommand::SetSearchReplacement {
            replacement: strip.replacement.clone(),
        })
    },
    |strip, search| {
        (strip.replace_open != search.replace_open()).then_some(
            SearchCommand::SetSearchReplaceOpen {
                open: strip.replace_open,
            },
        )
    },
    |strip, search| {
        (strip.scope != search.scope())
            .then_some(SearchCommand::SetSearchScope { scope: strip.scope })
    },
    |strip, search| {
        (strip.mode != search.mode()).then_some(SearchCommand::SetSearchMode { mode: strip.mode })
    },
    |strip, search| {
        (strip.match_case != search.match_case()).then_some(SearchCommand::SetSearchMatchCase {
            enabled: strip.match_case,
        })
    },
    |strip, search| {
        (strip.whole_word != search.whole_word()).then_some(SearchCommand::SetSearchWholeWord {
            enabled: strip.whole_word,
        })
    },
];

fn apply_search_inputs(app: &mut ScratchpadApp, state: &SearchStripState) {
    if state.preserve_case != app.state.search_state.query.preserve_case {
        app.state.search_state.query.preserve_case = state.preserve_case;
        app.state
            .search_state
            .replace
            .pending_replace_all_confirmation = None;
        app.state.search_state.replace.planning = None;
        crate::app::app_state::search_state::visual::refresh_search_visual_state(app);
    }
    for change in INPUT_CHANGES {
        if let Some(command) = change(state, &app.state.search_state) {
            crate::app::commands::handle_command(app, AppCommand::Search(command));
        }
    }
}

fn dispatch_search_actions(
    app: &mut ScratchpadApp,
    target_focus: SearchFocusTarget,
    actions: SearchStripActions,
) {
    for (requested, command) in [
        (
            actions.previous_requested,
            AppCommand::Search(SearchCommand::PreviousSearchMatch),
        ),
        (
            actions.next_requested,
            AppCommand::Search(SearchCommand::NextSearchMatch),
        ),
        (
            actions.undo_requested,
            AppCommand::Edit(EditCommand::UndoActiveBufferTextOperation),
        ),
        (
            actions.redo_requested,
            AppCommand::Edit(EditCommand::RedoActiveBufferTextOperation),
        ),
        (
            actions.replace_current_requested,
            AppCommand::Search(SearchCommand::ReplaceCurrentMatch),
        ),
        (
            actions.replace_all_requested,
            AppCommand::Search(SearchCommand::ReplaceAllMatches),
        ),
    ] {
        dispatch_requested_command(app, target_focus, requested, command);
    }

    if let Some(match_index) = actions.selected_match_index {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::ActivateSearchMatch { match_index }),
        );
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

    search_runtime::request_search_focus(app, target_focus);
    crate::app::commands::handle_command(app, command);
}

#[cfg(test)]
mod accessibility_tests;
