use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{SearchResultEntry, SearchResultGroup};
use crate::app::theme::{
    action_bg, action_hover_bg, border, tab_selected_accent, tab_selected_bg, text_muted,
    text_primary,
};
use crate::app::ui::settings;
use eframe::egui;

const SEARCH_RESULTS_VIEWPORT_HEIGHT_COLLAPSED: f32 = 300.0;
const SEARCH_RESULTS_VIEWPORT_HEIGHT_EXPANDED: f32 = 220.0;
const SEARCH_RESULT_ROW_HEIGHT: f32 = 40.0;

pub(super) fn show_search_results(
    ui: &mut egui::Ui,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    settings::dialog_card_frame(ui).show(ui, |ui| {
        render_results_header(ui, state);
        ui.add_space(10.0);

        if state.query.is_empty() {
            render_empty_results_state(ui, "Type to search across the selected scope.");
            return;
        }

        if state.result_groups.is_empty() {
            if state.progress.searching {
                render_empty_results_state(ui, "Searching...");
            } else {
                render_empty_results_state(ui, "No matches found.");
            }
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("search_results_list")
            .max_height(results_viewport_height(state))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for group in &state.result_groups {
                    show_search_group(ui, group, actions);
                }
            });
    });
}

fn render_results_header(ui: &mut egui::Ui, state: &SearchStripState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(egui_phosphor::regular::FILE_TEXT)
                .size(18.0)
                .color(text_muted(ui)),
        );
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Search results")
                    .size(15.0)
                    .color(text_primary(ui)),
            );
            ui.label(
                egui::RichText::new(results_summary(state))
                    .size(12.0)
                    .color(text_muted(ui)),
            );
        });
    });
}

fn results_summary(state: &SearchStripState) -> String {
    if state.progress.searching {
        return "Searching...".to_owned();
    }

    if state.query.is_empty() {
        return "Enter a query to populate results.".to_owned();
    }

    if state.progress.displayed_match_count < state.progress.total_match_count {
        return format!(
            "Showing first {} of {} matches.",
            state.progress.displayed_match_count, state.progress.total_match_count
        );
    }

    let file_count = state.result_groups.len();
    let file_label = if file_count == 1 { "file" } else { "files" };
    format!(
        "{} matches in {} {}.",
        state.match_count, file_count, file_label
    )
}

fn results_viewport_height(state: &SearchStripState) -> f32 {
    if state.replace_open {
        SEARCH_RESULTS_VIEWPORT_HEIGHT_EXPANDED
    } else {
        SEARCH_RESULTS_VIEWPORT_HEIGHT_COLLAPSED
    }
}

fn render_empty_results_state(ui: &mut egui::Ui, message: &str) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(message)
                .size(13.0)
                .color(text_muted(ui)),
        );
    });
    ui.add_space(12.0);
}

fn show_search_group(
    ui: &mut egui::Ui,
    group: &SearchResultGroup,
    actions: &mut SearchStripActions,
) {
    let mut start = 0;

    while start < group.entries.len() {
        let buffer_id = group.entries[start].buffer_id;
        let mut end = start + 1;
        while end < group.entries.len() && group.entries[end].buffer_id == buffer_id {
            end += 1;
        }

        show_file_section(
            ui,
            group,
            group.entries[start].buffer_label.as_str(),
            &group.entries[start..end],
            actions,
        );
        start = end;
    }
}

fn show_file_section(
    ui: &mut egui::Ui,
    group: &SearchResultGroup,
    buffer_label: &str,
    entries: &[SearchResultEntry],
    actions: &mut SearchStripActions,
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(egui_phosphor::regular::FILE_TEXT)
                .size(16.0)
                .color(text_muted(ui)),
        );
        ui.label(
            egui::RichText::new(buffer_label)
                .strong()
                .size(16.0)
                .color(text_primary(ui)),
        );
        show_count_badge(ui, entries.len());
    });

    if group.tab_label != buffer_label {
        ui.label(
            egui::RichText::new(&group.tab_label)
                .size(12.0)
                .color(text_muted(ui)),
        );
    }

    ui.add_space(6.0);
    for entry in entries {
        if show_result_row(ui, entry).clicked() {
            actions.selected_match_index = Some(entry.match_index);
        }

        ui.add_space(4.0);
    }
}

fn show_result_row(ui: &mut egui::Ui, entry: &SearchResultEntry) -> egui::Response {
    let inner = egui::Frame::NONE
        .fill(match_fill(ui, entry.active))
        .stroke(egui::Stroke::new(1.0, match_border(ui, entry.active)))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            let row_width = ui.available_width();

            ui.allocate_ui_with_layout(
                egui::vec2(row_width, SEARCH_RESULT_ROW_HEIGHT - 12.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add_sized(
                        egui::vec2(row_width, 0.0),
                        egui::Label::new(match_body(ui, entry)).truncate(),
                    );
                },
            )
            .response
        });

    inner.response.interact(egui::Sense::click())
}

fn match_body(ui: &egui::Ui, entry: &SearchResultEntry) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    append_text(
        &mut job,
        &format!("{}:{} ", entry.line_number, entry.column_number),
        text_muted(ui),
        15.0,
    );
    append_text(&mut job, match_preview(entry), text_primary(ui), 16.0);
    job.into()
}

fn match_preview(entry: &SearchResultEntry) -> &str {
    if entry.preview.is_empty() {
        "Match"
    } else {
        entry.preview.as_str()
    }
}

fn append_text(job: &mut egui::text::LayoutJob, text: &str, color: egui::Color32, size: f32) {
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            ..Default::default()
        },
    );
}

fn show_count_badge(ui: &mut egui::Ui, count: usize) {
    egui::Frame::NONE
        .fill(action_hover_bg(ui))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(count.to_string())
                    .size(14.0)
                    .color(text_muted(ui)),
            );
        });
}

fn match_fill(ui: &egui::Ui, active: bool) -> egui::Color32 {
    if active {
        tab_selected_bg(ui)
    } else {
        action_bg(ui)
    }
}

fn match_border(ui: &egui::Ui, active: bool) -> egui::Color32 {
    if active {
        tab_selected_accent(ui).gamma_multiply(0.95)
    } else {
        border(ui)
    }
}
