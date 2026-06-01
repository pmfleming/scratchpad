use super::state::{SearchStripActions, SearchStripState};
mod buttons;
mod input;
mod shortcuts;

use crate::app::app_state::{SearchFocusTarget, SearchReplaceAvailability, SearchScope};
use crate::app::shortcut_tooltips;
use crate::app::theme::{action_hover_bg, text_muted, text_primary};
use crate::app::ui::{callout, settings, widget_ids};
use buttons::{
    ICON_BUTTON_SIZE, icon_toggle_chip, replace_tooltip, scope_icon, scope_tooltip, toggle_flag,
    toggle_mode, trigger_action,
};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_CLOCKWISE, ARROW_COUNTER_CLOCKWISE, ARROWS_COUNTER_CLOCKWISE, CARET_DOWN, CARET_UP,
    MAGNIFYING_GLASS, SWAP, TEXTBOX,
};
use input::icon_text_input;
use shortcuts::{TextInputKind, consume_search_strip_shortcuts, consume_text_input_keys};

const CASE_SENSITIVE_ICON: &str = "Aa";
const CONTROL_BUTTON_HEIGHT: f32 = 34.0;
const SEARCH_CARD_CORNER_RADIUS: u8 = 12;

pub(super) fn show_search_controls(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    find_input_id: egui::Id,
    replace_input_id: egui::Id,
) {
    let (find_response, replace_response) = ui
        .vertical(|ui| {
            let find_response = render_search_pill(ui, state, find_input_id);
            let replace_response = render_replace_pill(ui, state, actions, replace_input_id);
            (find_response, replace_response)
        })
        .inner;

    if find_response.has_focus() {
        consume_text_input_keys(ui, actions, TextInputKind::Find);
    }
    if let Some(replace_response) = replace_response.as_ref()
        && replace_response.has_focus()
    {
        consume_text_input_keys(ui, actions, TextInputKind::Replace);
    }
    consume_search_strip_shortcuts(
        ui,
        state,
        actions,
        find_response.has_focus()
            || replace_response
                .as_ref()
                .is_some_and(|response| response.has_focus()),
    );
}

fn render_search_pill(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    find_input_id: egui::Id,
) -> egui::Response {
    search_card(ui, |ui| {
        ui.vertical(|ui| {
            let find_response = icon_text_input(
                ui,
                MAGNIFYING_GLASS,
                &mut state.query,
                find_input_id,
                "Search",
            );
            state.sync_focus(&find_response, SearchFocusTarget::FindInput);

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let summary = state.results_summary();
                if !summary.is_empty() {
                    ui.label(
                        egui::RichText::new(summary)
                            .size(12.5)
                            .color(text_muted(ui)),
                    );
                    ui.add_space(10.0);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    toggle_mode(ui, &mut state.mode);
                    toggle_flag(
                        ui,
                        &mut state.whole_word,
                        TEXTBOX,
                        shortcut_tooltips::SEARCH_WHOLE_WORD,
                    );
                    toggle_flag(
                        ui,
                        &mut state.match_case,
                        CASE_SENSITIVE_ICON,
                        shortcut_tooltips::SEARCH_MATCH_CASE,
                    );
                    ui.add_space(6.0);
                    for scope in [
                        SearchScope::SelectionOnly,
                        SearchScope::ActiveBuffer,
                        SearchScope::ActiveWorkspaceTab,
                        SearchScope::AllOpenTabs,
                    ] {
                        if icon_toggle_chip(
                            ui,
                            state.scope == scope,
                            scope_icon(scope),
                            scope_tooltip(scope, state.scope_origin),
                        )
                        .clicked()
                        {
                            state.scope = scope;
                        }
                    }
                });
            });

            find_response
        })
        .inner
    })
}

fn render_replace_pill(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    replace_input_id: egui::Id,
) -> Option<egui::Response> {
    search_card(ui, |ui| {
        render_replace_heading(ui, &mut state.replace_open);

        if !state.replace_open {
            return None;
        }

        ui.add_space(4.0);

        let replace_response = icon_text_input(
            ui,
            ARROWS_COUNTER_CLOCKWISE,
            &mut state.replacement,
            replace_input_id,
            "Replace",
        );
        state.sync_focus(&replace_response, SearchFocusTarget::ReplaceInput);

        ui.add_space(4.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), CONTROL_BUTTON_HEIGHT),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                let replace_enabled = matches!(
                    state.replace_availability,
                    SearchReplaceAvailability::Allowed
                );
                let replace_all_tooltip = replace_tooltip(
                    &state.replace_availability,
                    shortcut_tooltips::REPLACE_ALL_MATCHES,
                );
                trigger_action(
                    ui,
                    replace_enabled,
                    SWAP,
                    replace_all_tooltip,
                    &mut actions.replace_all_requested,
                );
                let replace_current_tooltip = replace_tooltip(
                    &state.replace_availability,
                    shortcut_tooltips::REPLACE_CURRENT_MATCH,
                );
                trigger_action(
                    ui,
                    replace_enabled,
                    ARROWS_COUNTER_CLOCKWISE,
                    replace_current_tooltip,
                    &mut actions.replace_current_requested,
                );
                trigger_action(
                    ui,
                    state.can_redo_text_operation,
                    ARROW_CLOCKWISE,
                    shortcut_tooltips::REDO,
                    &mut actions.redo_requested,
                );
                trigger_action(
                    ui,
                    state.can_undo_text_operation,
                    ARROW_COUNTER_CLOCKWISE,
                    shortcut_tooltips::UNDO,
                    &mut actions.undo_requested,
                );
                trigger_action(
                    ui,
                    state.match_count > 0,
                    CARET_DOWN,
                    shortcut_tooltips::SEARCH_NEXT_MATCH,
                    &mut actions.next_requested,
                );
                trigger_action(
                    ui,
                    state.match_count > 0,
                    CARET_UP,
                    shortcut_tooltips::SEARCH_PREVIOUS_MATCH,
                    &mut actions.previous_requested,
                );
            },
        );

        Some(replace_response)
    })
}

fn render_replace_heading(ui: &mut egui::Ui, replace_open: &mut bool) {
    ui.horizontal(|ui| {
        if pill_heading_button(ui, "Replace").clicked() {
            *replace_open = !*replace_open;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tooltip = if *replace_open {
                "Collapse replace controls"
            } else {
                "Expand replace controls"
            };
            if callout::icon_button(
                ui,
                "search_replace.replace_heading",
                if *replace_open { CARET_UP } else { CARET_DOWN },
                callout::IconButtonStyle {
                    icon_size: 16.0,
                    size: ICON_BUTTON_SIZE,
                    fill: action_hover_bg(ui),
                },
                tooltip,
                true,
            )
            .clicked()
            {
                *replace_open = !*replace_open;
            }
        });
    });
}

fn pill_heading_button(ui: &mut egui::Ui, title: &str) -> egui::Response {
    widget_ids::surface_response(
        ui,
        ("search_replace.heading", title),
        widget_ids::WidgetRole::ActionButton,
        |ui| {
            ui.add(
                egui::Button::new(
                    egui::RichText::new(title)
                        .size(15.0)
                        .color(text_primary(ui)),
                )
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
            )
        },
    )
}

fn search_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    settings::dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(SEARCH_CARD_CORNER_RADIUS))
        .show(ui, add_contents)
        .inner
}
