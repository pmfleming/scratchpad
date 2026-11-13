use super::callout;
mod controls;
mod footer;
mod header;
mod result_model;
mod results;
mod state;
mod summary;

use crate::app::app_state::{ScratchpadApp, SearchFocusTarget, search_runtime, workspace};
use crate::app::commands::{AppCommand, EditCommand, SearchCommand};
use crate::app::ui::{panel_style, settings, widget_ids};
use eframe::egui;
use state::{SearchStripActions, SearchStripState};

/// Width shared by other dialogs that line up with Search.
pub(crate) const SEARCH_DIALOG_WIDTH: f32 = 620.0;
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
    let screen = ctx.content_rect();
    let dismissible = panel_style::dismiss_on_outside_click(app);
    let popup_open = egui::Popup::is_any_open(&ctx);
    let layer = widget_ids::area_layer_id(egui::Order::Foreground, "search_window");
    let was_visible = ctx.memory(|m| m.areas().visible_last_frame(&layer));
    let frame = panel_style::window_frame(&ctx, app.state.app_settings.editor_background_color());
    // Window sizes include the frame, so add its margin to the header content width.
    let min_size = egui::vec2(
        header::min_width(&ctx) + frame.total_margin().sum().x,
        SEARCH_MIN_HEIGHT,
    );

    let response = egui::Window::new("Search")
        .id(layer.id)
        .order(layer.order)
        .title_bar(false)
        .resizable(true)
        .default_size(SEARCH_DEFAULT_SIZE.max(min_size))
        .min_size(min_size)
        .max_size((screen.size() - egui::vec2(24.0, 24.0)).max(min_size))
        .frame(frame)
        .default_pos(egui::pos2(
            (screen.right() - SEARCH_DEFAULT_SIZE.x - 24.0).max(screen.left()),
            screen.top() + 72.0,
        ))
        .show(&ctx, |ui| {
            widget_ids::feature_scope(ui, "search_dialog", |ui| {
                settings::apply_dialog_typography(ui);
                callout::apply_spacing(ui);
                panel_style::apply(ui);
                ui.visuals_mut().panel_fill = frame.fill;
                ui.visuals_mut().window_fill = frame.fill;

                actions.close_requested |= header::show(ui, &mut state, &mut actions, !dismissible);
                controls::show_search_controls(
                    ui,
                    &mut state,
                    &mut actions,
                    find_input_id,
                    replace_input_id,
                );
                summary::show(ui, app, &state, &mut actions);
                footer::panel(ui, app, &state, &mut actions);
                results::show_search_results(ui, app, &state, &mut actions);
            });
        });
    if let Some(response) = response {
        callout::mark_scroll_blocker_if_hovered(&ctx, &response.response);
        // Ignore the opening click and interactions with the panel's menus.
        actions.close_requested |= dismissible
            && was_visible
            && !popup_open
            && !egui::Popup::is_any_open(&ctx)
            && !app.state.dialogs.any_modal_open()
            && workspace::accessors::pending_action(app).is_none()
            && response.response.clicked_elsewhere();
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
    if actions.recapture_selection {
        crate::app::app_state::search_state::api::set_search_scope(
            app,
            crate::app::app_state::SearchScope::SelectionOnly,
        );
    }
    if actions.cancel_review {
        app.state
            .search_state
            .replace
            .pending_replace_all_confirmation = None;
    }
    if let Some((id, files)) = actions.undo_all {
        crate::app::app_state::search_state::replace::transaction::undo(app, id, files);
    }
    if let Some(index) = actions.recent_index
        && let Some(recent) = state.recent.get(index)
    {
        crate::app::app_state::search_state::api::set_search_query(app, recent.query.clone());
        crate::app::app_state::search_state::api::set_search_mode(
            app,
            if recent.regex {
                crate::app::services::search::SearchMode::Regex
            } else {
                crate::app::services::search::SearchMode::PlainText
            },
        );
        crate::app::app_state::search_state::api::set_search_match_case(app, recent.match_case);
        crate::app::app_state::search_state::api::set_search_whole_word(app, recent.whole_word);
    }
    if actions.next_requested || actions.previous_requested {
        crate::app::app_state::search_state::recent::commit(app);
    }
    if actions.close_requested {
        crate::app::commands::handle_command(app, AppCommand::Search(SearchCommand::Close));
        return;
    }

    dispatch_search_actions(app, state.target_focus(), actions);
}

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
    if state.query != app.state.search_state.query() {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::SetSearchQuery {
                query: state.query.clone(),
            }),
        );
    }
    if state.replacement != app.state.search_state.replacement() {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::SetSearchReplacement {
                replacement: state.replacement.clone(),
            }),
        );
    }
    if state.replace_open != app.state.search_state.replace_open() {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::SetSearchReplaceOpen {
                open: state.replace_open,
            }),
        );
    }
    if state.scope != app.state.search_state.scope() {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::SetSearchScope { scope: state.scope }),
        );
    }
    if state.mode != app.state.search_state.mode() {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::SetSearchMode { mode: state.mode }),
        );
    }
    if state.match_case != app.state.search_state.match_case() {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::SetSearchMatchCase {
                enabled: state.match_case,
            }),
        );
    }
    if state.whole_word != app.state.search_state.whole_word() {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::SetSearchWholeWord {
                enabled: state.whole_word,
            }),
        );
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
