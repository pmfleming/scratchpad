use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{SearchFocusTarget, SearchScope};
use crate::app::ui::callout;
use crate::app::theme::{
    action_bg, action_hover_bg, border, tab_selected_accent, tab_selected_bg, text_muted,
    text_primary,
};
use eframe::egui;

const TOOLBAR_BUTTON_HEIGHT: f32 = 30.0;
const INPUT_HEIGHT: f32 = 38.0;
const TOOLBAR_ICON_SIZE: f32 = 18.0;
const INPUT_ACTION_BUTTON_WIDTH: f32 = 36.0;

pub(super) fn show_search_controls(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    find_input_id: egui::Id,
    replace_input_id: egui::Id,
) {
    let (find_response, replace_response) = ui
        .vertical(|ui| {
            show_toolbar(ui, state, actions);
            ui.add_space(2.0);
            let responses = show_input_row(ui, state, actions, find_input_id, replace_input_id);
            ui.add_space(2.0);
            show_footer(ui, state, actions);
            responses
        })
        .inner;

    if find_response.has_focus() {
        consume_find_input_keys(ui, actions);
    }
    if replace_response.has_focus() {
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            actions.replace_current_requested = true;
        }
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            actions.close_requested = true;
        }
    }
}

fn show_toolbar(ui: &mut egui::Ui, state: &mut SearchStripState, actions: &mut SearchStripActions) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(egui_phosphor::regular::MAGNIFYING_GLASS)
                .size(TOOLBAR_ICON_SIZE)
                .color(text_primary(ui)),
        );

        if toggle_chip(
            ui,
            state.match_case,
            "",
            Some("Aa"),
            "Match case",
        )
        .clicked()
        {
            state.match_case = !state.match_case;
        }

        if toggle_chip(
            ui,
            state.whole_word,
            "",
            Some("Whole Word"),
            "Match whole words only",
        )
        .clicked()
        {
            state.whole_word = !state.whole_word;
        }

        if callout::icon_button(
            ui,
            scope_icon(state.scope),
            TOOLBAR_ICON_SIZE,
            egui::vec2(30.0, TOOLBAR_BUTTON_HEIGHT),
            action_bg(ui),
            &scope_tooltip(state.scope),
            true,
        )
        .clicked()
        {
            state.scope = next_scope(state.scope);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if callout::close_button(ui, "Close search")
            .clicked()
            {
                actions.close_requested = true;
            }

            if callout::icon_button(
                ui,
                egui_phosphor::regular::PENCIL_SIMPLE_LINE,
                TOOLBAR_ICON_SIZE,
                egui::vec2(30.0, TOOLBAR_BUTTON_HEIGHT),
                action_bg(ui),
                "Replace current match",
                state.match_count > 0,
            )
            .clicked()
            {
                actions.replace_current_requested = true;
            }
        });
    });
}

fn show_input_row(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    find_input_id: egui::Id,
    replace_input_id: egui::Id,
) -> (egui::Response, egui::Response) {
    ui.horizontal(|ui| {
        let field_width =
            ((ui.available_width() - (INPUT_ACTION_BUTTON_WIDTH * 3.0) - 26.0) / 2.0).max(170.0);
        let find_response = ui.add_sized(
            [field_width, INPUT_HEIGHT],
            search_text_edit(&mut state.query, find_input_id, "Search"),
        );
        state.sync_focus(&find_response, SearchFocusTarget::FindInput);

        let replace_response = ui.add_sized(
            [field_width, INPUT_HEIGHT],
            search_text_edit(&mut state.replacement, replace_input_id, "Replace"),
        );
        state.sync_focus(&replace_response, SearchFocusTarget::ReplaceInput);

        ui.add_space(2.0);

        if callout::icon_button(
            ui,
            egui_phosphor::regular::CARET_UP,
            16.0,
            egui::vec2(INPUT_ACTION_BUTTON_WIDTH, INPUT_HEIGHT),
            action_hover_bg(ui),
            "Jump to the previous match",
            state.match_count > 0,
        )
        .clicked()
        {
            actions.previous_requested = true;
        }
        if callout::icon_button(
            ui,
            egui_phosphor::regular::CARET_DOWN,
            16.0,
            egui::vec2(INPUT_ACTION_BUTTON_WIDTH, INPUT_HEIGHT),
            action_hover_bg(ui),
            "Jump to the next match",
            state.match_count > 0,
        )
        .clicked()
        {
            actions.next_requested = true;
        }
        if callout::icon_button(
            ui,
            egui_phosphor::regular::PENCIL_LINE,
            16.0,
            egui::vec2(INPUT_ACTION_BUTTON_WIDTH, INPUT_HEIGHT),
            action_hover_bg(ui),
            "Replace all matches in the current scope",
            state.match_count > 0,
        )
        .clicked()
        {
            actions.replace_all_requested = true;
        }

        (find_response, replace_response)
    })
    .inner
}

fn show_footer(ui: &mut egui::Ui, state: &SearchStripState, _actions: &mut SearchStripActions) {
    let left_label = if state.query.is_empty() {
        format!("{} | Enter next, Shift+Enter previous", state.scope.label())
    } else {
        format!("{} | {}", state.scope.label(), state.match_label)
    };
    ui.label(
        egui::RichText::new(left_label)
            .small()
            .color(text_muted(ui)),
    );
}

fn toggle_chip(
    ui: &mut egui::Ui,
    selected: bool,
    icon: &str,
    label: Option<&str>,
    tooltip: &str,
) -> egui::Response {
    let content = match (icon.is_empty(), label) {
        (true, Some(label)) => label.to_owned(),
        (false, Some(label)) => format!("{} {}", icon, label),
        (false, None) => icon.to_owned(),
        (true, None) => String::new(),
    };

    ui.add(
        egui::Button::new(egui::RichText::new(content).color(text_primary(ui)))
            .min_size(egui::vec2(30.0, TOOLBAR_BUTTON_HEIGHT))
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
            .corner_radius(egui::CornerRadius::same(10)),
    )
    .on_hover_text(tooltip)
}

fn search_text_edit<'a>(text: &'a mut String, id: egui::Id, hint: &str) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .id(id)
        .hint_text(hint)
        .margin(egui::Margin::symmetric(10, 8))
        .vertical_align(egui::Align::Center)
}

fn next_scope(scope: SearchScope) -> SearchScope {
    match scope {
        SearchScope::ActiveBuffer => SearchScope::ActiveWorkspaceTab,
        SearchScope::ActiveWorkspaceTab => SearchScope::AllOpenTabs,
        SearchScope::AllOpenTabs => SearchScope::ActiveBuffer,
    }
}

fn scope_icon(scope: SearchScope) -> &'static str {
    match scope {
        SearchScope::ActiveBuffer => egui_phosphor::regular::FILE_TEXT,
        SearchScope::ActiveWorkspaceTab => egui_phosphor::regular::TABS,
        SearchScope::AllOpenTabs => egui_phosphor::regular::BROWSERS,
    }
}

fn scope_tooltip(scope: SearchScope) -> String {
    let next_scope = next_scope(scope);
    format!(
        "Search scope: {}. Click to switch to {}.",
        scope.label(),
        next_scope.label()
    )
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
