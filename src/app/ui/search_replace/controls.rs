use super::results::results_summary;
use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{
    SearchFocusTarget, SearchReplaceAvailability, SearchScope, SearchScopeOrigin,
};
use crate::app::services::search::SearchMode;
use crate::app::theme::{
    action_hover_bg, border, tab_selected_accent, tab_selected_bg, text_muted, text_primary,
};
use crate::app::ui::{callout, settings};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_CLOCKWISE, ARROW_COUNTER_CLOCKWISE, ARROWS_COUNTER_CLOCKWISE, CARDS, CARET_DOWN,
    CARET_UP, MAGNIFYING_GLASS, RECTANGLE, SWAP, TABS, TEXT_ALIGN_JUSTIFY, TEXTBOX,
};

const CASE_SENSITIVE_ICON: &str = "Aa";
const REGEX_ICON: &str = ".*";
const INPUT_HEIGHT: f32 = 36.0;
const ICON_SIZE: f32 = 20.0;
const CONTROL_BUTTON_HEIGHT: f32 = 34.0;
const ICON_BUTTON_SIZE: egui::Vec2 = egui::vec2(36.0, CONTROL_BUTTON_HEIGHT);
const SEARCH_CARD_CORNER_RADIUS: u8 = 12;
const SEARCH_INPUT_CORNER_RADIUS: u8 = 8;

pub(super) fn show_search_controls(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    find_input_id: egui::Id,
    replace_input_id: egui::Id,
) {
    let (find_response, replace_response) = ui
        .vertical(|ui| {
            let find_response = render_search_pill(ui, state, actions, find_input_id);
            let replace_response = render_replace_pill(ui, state, actions, replace_input_id);
            (find_response, replace_response)
        })
        .inner;

    if find_response.has_focus() {
        consume_find_input_keys(ui, actions);
    }
    if let Some(replace_response) = replace_response
        && replace_response.has_focus()
    {
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            actions.replace_current_requested = true;
        }
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            actions.close_requested = true;
        }
    }
}

fn render_search_pill(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    _actions: &mut SearchStripActions,
    find_input_id: egui::Id,
) -> egui::Response {
    search_card(ui, |ui| {
        ui.vertical(|ui| {
            // Icon + text field on the same row
            let find_response = ui
                .horizontal(|ui| {
                    ui.allocate_ui(egui::vec2(28.0, INPUT_HEIGHT), |ui| {
                        ui.with_layout(
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(MAGNIFYING_GLASS)
                                        .font(egui::FontId::proportional(ICON_SIZE))
                                        .color(text_muted(ui)),
                                );
                            },
                        );
                    });
                    compact_text_field(
                        ui,
                        &mut state.query,
                        find_input_id,
                        "Search",
                        ui.available_width(),
                    )
                })
                .inner;
            state.sync_focus(&find_response, SearchFocusTarget::FindInput);

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let summary = results_summary(state);
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
                    toggle_flag(ui, &mut state.whole_word, TEXTBOX, "Whole word");
                    toggle_flag(
                        ui,
                        &mut state.match_case,
                        CASE_SENSITIVE_ICON,
                        "Case sensitive",
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

        // Icon + text field on the same row
        let replace_response = ui
            .horizontal(|ui| {
                ui.allocate_ui(egui::vec2(28.0, INPUT_HEIGHT), |ui| {
                    ui.with_layout(
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.label(
                                egui::RichText::new(ARROWS_COUNTER_CLOCKWISE)
                                    .font(egui::FontId::proportional(ICON_SIZE))
                                    .color(text_muted(ui)),
                            );
                        },
                    );
                });
                compact_text_field(
                    ui,
                    &mut state.replacement,
                    replace_input_id,
                    "Replace",
                    ui.available_width(),
                )
            })
            .inner;
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
                let replace_all_tooltip =
                    replace_tooltip(&state.replace_availability, "Replace all matches");
                trigger_action(
                    ui,
                    replace_enabled,
                    SWAP,
                    replace_all_tooltip,
                    &mut actions.replace_all_requested,
                );
                let replace_current_tooltip =
                    replace_tooltip(&state.replace_availability, "Replace current match");
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
                    "Redo the last operation-based text edit in the active buffer",
                    &mut actions.redo_requested,
                );
                trigger_action(
                    ui,
                    state.can_undo_text_operation,
                    ARROW_COUNTER_CLOCKWISE,
                    "Undo the last operation-based text edit in the active buffer",
                    &mut actions.undo_requested,
                );
                trigger_action(
                    ui,
                    state.match_count > 0,
                    CARET_DOWN,
                    "Next match",
                    &mut actions.next_requested,
                );
                trigger_action(
                    ui,
                    state.match_count > 0,
                    CARET_UP,
                    "Previous match",
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
                if *replace_open { CARET_UP } else { CARET_DOWN },
                16.0,
                ICON_BUTTON_SIZE,
                action_hover_bg(ui),
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
    ui.add(
        egui::Button::new(
            egui::RichText::new(title)
                .size(15.0)
                .color(text_primary(ui)),
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE),
    )
}

fn search_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    settings::dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(SEARCH_CARD_CORNER_RADIUS))
        .show(ui, add_contents)
        .inner
}

fn compact_text_field(
    ui: &mut egui::Ui,
    text: &mut String,
    id: egui::Id,
    hint: &str,
    width: f32,
) -> egui::Response {
    let inner = egui::Frame::NONE
        .fill(ui.visuals().widgets.inactive.weak_bg_fill)
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(SEARCH_INPUT_CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(2, 0))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(width, INPUT_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add_sized(
                        [width, INPUT_HEIGHT],
                        search_text_edit(text, id, hint).frame(egui::Frame::NONE),
                    )
                },
            )
            .inner
        });

    let stroke = if inner.inner.has_focus() {
        ui.visuals().widgets.active.bg_stroke
    } else if inner.inner.hovered() {
        ui.visuals().widgets.hovered.bg_stroke
    } else {
        ui.visuals().widgets.inactive.bg_stroke
    };
    ui.painter().rect_stroke(
        inner.response.rect,
        egui::CornerRadius::same(SEARCH_INPUT_CORNER_RADIUS),
        stroke,
        egui::StrokeKind::Inside,
    );

    inner.inner
}

fn icon_toggle_chip(
    ui: &mut egui::Ui,
    selected: bool,
    icon: &str,
    tooltip: &str,
) -> egui::Response {
    chip_button(
        ui,
        egui::RichText::new(icon)
            .font(egui::FontId::proportional(16.0))
            .color(if selected {
                text_primary(ui)
            } else {
                text_primary(ui).gamma_multiply(0.9)
            }),
        selected,
        ICON_BUTTON_SIZE,
        egui::vec2(0.0, 0.0),
        tooltip,
    )
}

fn icon_action_button(
    ui: &mut egui::Ui,
    icon: &str,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    callout::icon_button(
        ui,
        icon,
        16.0,
        ICON_BUTTON_SIZE,
        action_hover_bg(ui),
        tooltip,
        enabled,
    )
}

fn search_text_edit<'a>(text: &'a mut String, id: egui::Id, hint: &str) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .id(id)
        .hint_text(hint)
        .margin(egui::Margin::symmetric(10, 6))
}

fn toggle_flag(ui: &mut egui::Ui, value: &mut bool, icon: &str, tooltip: &str) {
    if icon_toggle_chip(ui, *value, icon, tooltip).clicked() {
        *value = !*value;
    }
}

fn trigger_action(ui: &mut egui::Ui, enabled: bool, icon: &str, tooltip: &str, flag: &mut bool) {
    if icon_action_button(ui, icon, tooltip, enabled).clicked() {
        *flag = true;
    }
}

fn chip_button(
    ui: &mut egui::Ui,
    text: egui::RichText,
    selected: bool,
    min_size: egui::Vec2,
    padding: egui::Vec2,
    tooltip: &str,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = padding;
        ui.add(
            egui::Button::new(text)
                .min_size(min_size)
                .fill(if selected {
                    tab_selected_bg(ui)
                } else {
                    action_hover_bg(ui)
                })
                .stroke(egui::Stroke::new(
                    1.0,
                    if selected {
                        tab_selected_accent(ui)
                    } else {
                        border(ui)
                    },
                ))
                .corner_radius(egui::CornerRadius::same(8)),
        )
        .on_hover_text(tooltip)
    })
    .inner
}

fn scope_tooltip(scope: SearchScope, origin: SearchScopeOrigin) -> &'static str {
    match scope {
        SearchScope::ActiveBuffer => "Search the Current Focused File",
        SearchScope::SelectionOnly if origin == SearchScopeOrigin::SelectionDefault => {
            "Search Selected Text (auto-selected)"
        }
        SearchScope::SelectionOnly => "Search Selected Text",
        SearchScope::ActiveWorkspaceTab => "Search All Files on This Tab",
        SearchScope::AllOpenTabs => "Search All Open Files",
    }
}

fn scope_icon(scope: SearchScope) -> &'static str {
    match scope {
        SearchScope::SelectionOnly => TEXT_ALIGN_JUSTIFY,
        SearchScope::ActiveBuffer => RECTANGLE,
        SearchScope::ActiveWorkspaceTab => CARDS,
        SearchScope::AllOpenTabs => TABS,
    }
}

fn toggle_mode(ui: &mut egui::Ui, mode: &mut SearchMode) {
    let regex_enabled = *mode == SearchMode::Regex;
    if icon_toggle_chip(ui, regex_enabled, REGEX_ICON, "Regex").clicked() {
        *mode = if regex_enabled {
            SearchMode::PlainText
        } else {
            SearchMode::Regex
        };
    }
}

fn replace_tooltip<'a>(
    availability: &'a SearchReplaceAvailability,
    allowed_tooltip: &'a str,
) -> &'a str {
    match availability {
        SearchReplaceAvailability::Allowed => allowed_tooltip,
        SearchReplaceAvailability::Disabled => "Replace is unavailable until results are ready.",
        SearchReplaceAvailability::Blocked(message) => message.as_str(),
    }
}

fn consume_find_input_keys(ui: &mut egui::Ui, actions: &mut SearchStripActions) {
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)) {
        actions.previous_requested = true;
    } else if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        actions.next_requested = true;
    }

    if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        actions.close_requested = true;
    }
}
