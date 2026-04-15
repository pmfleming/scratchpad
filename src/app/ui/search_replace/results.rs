use super::state::{SearchStripActions, SearchStripState};
use crate::app::theme::{action_hover_bg, tab_selected_bg, text_muted, text_primary};
use eframe::egui;

const SEARCH_RESULTS_MAX_HEIGHT: f32 = 220.0;
const SEARCH_RESULT_ROW_HEIGHT: f32 = 42.0;

pub(super) fn show_search_results(
    ui: &mut egui::Ui,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    if state.query.is_empty() {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Type to search across the selected scope.")
                .color(text_muted(ui)),
        );
        return;
    }

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);
    show_search_progress(ui, state);

    if state.result_groups.is_empty() {
        if !state.progress.searching {
            ui.label(egui::RichText::new("No matches found.").color(text_muted(ui)));
        }
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("search_results_list")
        .max_height(SEARCH_RESULTS_MAX_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for group in &state.result_groups {
                show_search_group(ui, group, actions);
            }
        });
}

fn show_search_progress(ui: &mut egui::Ui, state: &SearchStripState) {
    if state.progress.searching {
        ui.label(egui::RichText::new("Searching...").color(text_muted(ui)));
        ui.add_space(4.0);
        return;
    }

    if state.progress.displayed_match_count < state.progress.total_match_count {
        ui.label(
            egui::RichText::new(format!(
                "Showing first {} of {} matches.",
                state.progress.displayed_match_count, state.progress.total_match_count
            ))
            .color(text_muted(ui)),
        );
        ui.add_space(4.0);
    }
}

fn show_search_group(
    ui: &mut egui::Ui,
    group: &crate::app::app_state::SearchResultGroup,
    actions: &mut SearchStripActions,
) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(group.tab_label.clone())
            .strong()
            .color(text_primary(ui)),
    );
    ui.add_space(2.0);

    for entry in &group.entries {
        let body = match_body(entry);
        let button = egui::Button::new(egui::RichText::new(body).color(text_primary(ui)))
            .wrap()
            .fill(match_fill(ui, entry.active));

        if ui
            .add_sized([ui.available_width(), SEARCH_RESULT_ROW_HEIGHT], button)
            .clicked()
        {
            actions.selected_match_index = Some(entry.match_index);
        }
    }
}

fn match_body(entry: &crate::app::app_state::SearchResultEntry) -> String {
    let subtitle = format!(
        "{}  Line {}, Col {}",
        entry.buffer_label, entry.line_number, entry.column_number
    );
    if entry.preview.is_empty() {
        subtitle
    } else {
        format!("{}\n{}", subtitle, entry.preview)
    }
}

fn match_fill(ui: &egui::Ui, active: bool) -> egui::Color32 {
    if active {
        tab_selected_bg(ui)
    } else {
        action_hover_bg(ui)
    }
}