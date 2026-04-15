use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{SearchFocusTarget, SearchScope};
use crate::app::theme::{action_hover_bg, text_muted, text_primary};
use eframe::egui;

pub(super) fn show_search_controls(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    find_input_id: egui::Id,
    replace_input_id: egui::Id,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Find").color(text_primary(ui)));

        let find_response = ui.add_sized(
            [220.0, 28.0],
            egui::TextEdit::singleline(&mut state.query)
                .id(find_input_id)
                .hint_text("Search all open text"),
        );
        state.sync_focus(&find_response, SearchFocusTarget::FindInput);

        ui.label(egui::RichText::new("Replace").color(text_primary(ui)));
        let replace_response = ui.add_sized(
            [180.0, 28.0],
            egui::TextEdit::singleline(&mut state.replacement)
                .id(replace_input_id)
                .hint_text("Replacement text"),
        );
        state.sync_focus(&replace_response, SearchFocusTarget::ReplaceInput);

        egui::ComboBox::from_id_salt("search_scope")
            .selected_text(state.scope.label())
            .show_ui(ui, |ui| {
                selectable_scope(ui, &mut state.scope, SearchScope::ActiveBuffer);
                selectable_scope(ui, &mut state.scope, SearchScope::ActiveWorkspaceTab);
                selectable_scope(ui, &mut state.scope, SearchScope::AllOpenTabs);
            });

        ui.toggle_value(&mut state.match_case, "Aa");
        ui.toggle_value(&mut state.whole_word, "Whole");

        if nav_button(ui, state.match_count > 0, "Prev").clicked() {
            actions.previous_requested = true;
        }
        if nav_button(ui, state.match_count > 0, "Next").clicked() {
            actions.next_requested = true;
        }
        if nav_button(ui, state.match_count > 0, "Replace").clicked() {
            actions.replace_current_requested = true;
        }
        if nav_button(ui, state.match_count > 0, "Replace All In File").clicked() {
            actions.replace_all_requested = true;
        }
        ui.label(egui::RichText::new(&state.match_label).color(text_muted(ui)));
        if ui.button("Close").clicked() {
            actions.close_requested = true;
        }

        if find_response.has_focus() {
            consume_find_input_keys(ui, actions);
        }
        if replace_response.has_focus()
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            actions.close_requested = true;
        }
    });
}

fn selectable_scope(ui: &mut egui::Ui, scope: &mut SearchScope, value: SearchScope) {
    ui.selectable_value(scope, value, value.label());
}

fn nav_button(ui: &mut egui::Ui, enabled: bool, label: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(label).fill(action_hover_bg(ui)),
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