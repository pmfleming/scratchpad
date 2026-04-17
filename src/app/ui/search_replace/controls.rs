use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{SearchFocusTarget, SearchScope};
use crate::app::theme::{
    action_hover_bg, border, tab_selected_accent, tab_selected_bg, text_muted, text_primary,
};
use crate::app::ui::{callout, settings};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_CLOCKWISE, CARET_DOWN, CARET_UP, MAGNIFYING_GLASS, SWAP, TEXT_ALIGN_JUSTIFY,
};

const CASE_SENSITIVE_ICON: &str = "Aa";
const INPUT_HEIGHT: f32 = 36.0;
const CARD_ICON_SIZE: f32 = 18.0;
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
            if render_header(ui) {
                actions.close_requested = true;
            }

            let find_response = render_search_pill(ui, state, find_input_id);
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

fn render_header(ui: &mut egui::Ui) -> bool {
    callout::header_row(ui, "Close search", |_| {})
}

fn render_search_pill(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    find_input_id: egui::Id,
) -> egui::Response {
    search_card(ui, |ui| {
        ui.vertical(|ui| {
            render_pill_heading(ui, MAGNIFYING_GLASS, "Search");
            ui.add_space(8.0);

            let find_response = compact_text_field(
                ui,
                &mut state.query,
                find_input_id,
                "Search",
                ui.available_width(),
            );
            state.sync_focus(&find_response, SearchFocusTarget::FindInput);

            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), CONTROL_BUTTON_HEIGHT),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    for scope in [
                        SearchScope::ActiveBuffer,
                        SearchScope::ActiveWorkspaceTab,
                        SearchScope::AllOpenTabs,
                    ] {
                        if toggle_chip(
                            ui,
                            state.scope == scope,
                            scope.label(),
                            &scope_tooltip(scope),
                        )
                        .clicked()
                        {
                            state.scope = scope;
                        }
                    }
                },
            );

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
        ui.horizontal(|ui| {
            let toggle = pill_heading_button(ui, ARROW_CLOCKWISE, "Replace");
            if toggle.clicked() {
                state.replace_open = !state.replace_open;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if callout::icon_button(
                    ui,
                    if state.replace_open {
                        CARET_UP
                    } else {
                        CARET_DOWN
                    },
                    16.0,
                    ICON_BUTTON_SIZE,
                    action_hover_bg(ui),
                    if state.replace_open {
                        "Collapse replace controls"
                    } else {
                        "Expand replace controls"
                    },
                    true,
                )
                .clicked()
                {
                    state.replace_open = !state.replace_open;
                }
            });
        });

        if !state.replace_open {
            return None;
        }

        ui.add_space(8.0);
        let replace_response = compact_text_field(
            ui,
            &mut state.replacement,
            replace_input_id,
            "Replace",
            ui.available_width(),
        );
        state.sync_focus(&replace_response, SearchFocusTarget::ReplaceInput);

        ui.add_space(8.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), CONTROL_BUTTON_HEIGHT),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if icon_action_button(ui, SWAP, "Replace all matches", state.match_count > 0)
                    .clicked()
                {
                    actions.replace_all_requested = true;
                }
                if icon_action_button(
                    ui,
                    ARROW_CLOCKWISE,
                    "Replace current match",
                    state.match_count > 0,
                )
                .clicked()
                {
                    actions.replace_current_requested = true;
                }
                if icon_action_button(ui, CARET_DOWN, "Next match", state.match_count > 0).clicked()
                {
                    actions.next_requested = true;
                }
                if icon_action_button(ui, CARET_UP, "Previous match", state.match_count > 0)
                    .clicked()
                {
                    actions.previous_requested = true;
                }
                ui.add_space(6.0);
                if icon_toggle_chip(ui, state.whole_word, TEXT_ALIGN_JUSTIFY, "Whole word")
                    .clicked()
                {
                    state.whole_word = !state.whole_word;
                }
                if icon_toggle_chip(ui, state.match_case, CASE_SENSITIVE_ICON, "Case sensitive")
                    .clicked()
                {
                    state.match_case = !state.match_case;
                }
            },
        );

        Some(replace_response)
    })
}

fn render_pill_heading(ui: &mut egui::Ui, icon: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(icon)
                .size(CARD_ICON_SIZE)
                .color(text_muted(ui)),
        );
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(title)
                .size(15.0)
                .color(text_primary(ui)),
        );
    });
}

fn pill_heading_button(ui: &mut egui::Ui, icon: &str, title: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(format!("{icon}  {title}"))
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
            ui.add_sized(
                [width, INPUT_HEIGHT],
                search_text_edit(text, id, hint).frame(egui::Frame::NONE),
            )
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

fn toggle_chip(ui: &mut egui::Ui, selected: bool, label: &str, tooltip: &str) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(8.0, 0.0);
        ui.add(
            egui::Button::new(egui::RichText::new(label).size(12.5).color(if selected {
                text_primary(ui)
            } else {
                text_primary(ui).gamma_multiply(0.9)
            }))
            .min_size(egui::vec2(0.0, CONTROL_BUTTON_HEIGHT))
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

fn icon_toggle_chip(
    ui: &mut egui::Ui,
    selected: bool,
    icon: &str,
    tooltip: &str,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
        ui.add(
            egui::Button::new(
                egui::RichText::new(icon)
                    .font(egui::FontId::proportional(16.0))
                    .color(if selected {
                        text_primary(ui)
                    } else {
                        text_primary(ui).gamma_multiply(0.9)
                    }),
            )
            .min_size(ICON_BUTTON_SIZE)
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
        .margin(egui::Margin::symmetric(10, 8))
        .vertical_align(egui::Align::Center)
}

fn scope_tooltip(scope: SearchScope) -> String {
    format!("Search scope: {}", scope.label())
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
