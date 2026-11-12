use super::callout;
mod controls;
mod result_model;
mod results;
mod state;
mod summary;

use crate::app::app_state::{ScratchpadApp, SearchFocusTarget, search_runtime};
use crate::app::commands::{AppCommand, EditCommand, SearchCommand};
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
                    summary::show(ui, app, &state, &mut actions);
                    results::show_search_results(ui, app, &state, &mut actions);
                    summary::footer(ui, app, &state, &mut actions);
                });
                callout::mark_scroll_blocker_if_hovered(ui.ctx(), &inner.response);
            });
        });

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

    if let Some(match_index) = actions.focused_file_match_index {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::FocusSearchResultFile { match_index }),
        );
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

fn render_search_header(ui: &mut egui::Ui) -> bool {
    callout::header_row(ui, "search_replace.header", "Close search", |ui| {
        ui.label(
            egui::RichText::new("Search")
                .size(SEARCH_TITLE_SIZE)
                .color(callout::text(ui)),
        );
    })
}

#[cfg(test)]
mod accessibility_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn redesigned_fields_and_actions_are_named_and_replace_can_collapse() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::app::app_state::test_support::test_app(root.path());
        app.open_search_and_replace();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1100.0, 1000.0))
            .with_context_setup(|ctx| {
                crate::app::fonts::apply_editor_fonts(
                    ctx,
                    &crate::app::fonts::EditorFontSelection::scratchpad(
                        crate::app::fonts::EditorFontPreset::Standard,
                    ),
                    4.0,
                )
                .unwrap();
                ctx.options_mut(|o| o.warn_on_id_clash = true);
            })
            .build_ui_state(
                |ui, app| {
                    app.poll_search();
                    show_search_strip(ui, app);
                },
                app,
            );
        harness.run_steps(3);
        harness.get_by_role_and_label(egui::Role::TextInput, "Find");
        harness.get_by_role_and_label(egui::Role::TextInput, "Replace with");
        harness.get_by_role_and_label(egui::Role::Button, "Replace all included matches");
        harness
            .get_by_role_and_label(egui::Role::Button, "▾ Replace")
            .click();
        harness.run_steps(3);
        assert!(
            harness
                .query_by_role_and_label(egui::Role::TextInput, "Replace with")
                .is_none()
        );
    }
}
