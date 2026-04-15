use super::state::{SearchStripActions, SearchStripState};
use crate::app::theme::{
    action_bg, action_hover_bg, border, tab_selected_bg, text_muted, text_primary,
};
use eframe::egui;

const SEARCH_RESULTS_VIEWPORT_HEIGHT: f32 = 320.0;
const SEARCH_RESULT_ROW_HEIGHT: f32 = 40.0;

pub(super) fn show_search_results(
    ui: &mut egui::Ui,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    if state.query.is_empty() {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Type to search across the selected scope.").color(text_muted(ui)),
        );
        return;
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);
    show_search_progress(ui, state);

    if state.result_groups.is_empty() {
        if !state.progress.searching {
            ui.label(egui::RichText::new("No matches found.").color(text_muted(ui)));
        }
        return;
    }

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), SEARCH_RESULTS_VIEWPORT_HEIGHT),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::ScrollArea::vertical()
                .id_salt("search_results_list")
                .max_height(SEARCH_RESULTS_VIEWPORT_HEIGHT)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for group in &state.result_groups {
                        show_search_group(ui, group, actions);
                    }
                });
        },
    );
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
    let mut start = 0;
    while start < group.entries.len() {
        let buffer_label = group.entries[start].buffer_label.clone();
        let mut end = start + 1;
        while end < group.entries.len() && group.entries[end].buffer_label == buffer_label {
            end += 1;
        }

        show_file_section(
            ui,
            group,
            &buffer_label,
            &group.entries[start..end],
            actions,
        );
        start = end;
    }
}

fn show_file_section(
    ui: &mut egui::Ui,
    group: &crate::app::app_state::SearchResultGroup,
    buffer_label: &str,
    entries: &[crate::app::app_state::SearchResultEntry],
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
                .small()
                .color(text_muted(ui)),
        );
    }

    ui.add_space(2.0);
    for entry in entries {
        if show_result_row(ui, entry).clicked() {
            actions.selected_match_index = Some(entry.match_index);
        }

        ui.add_space(2.0);
    }
}

fn show_result_row(
    ui: &mut egui::Ui,
    entry: &crate::app::app_state::SearchResultEntry,
) -> egui::Response {
    egui::Frame::NONE
        .fill(match_fill(ui, entry.active))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), SEARCH_RESULT_ROW_HEIGHT - 12.0),
                egui::Sense::click(),
            );
            let mut child_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child_ui.add(egui::Label::new(match_body(ui, entry)).wrap());
            response
        })
        .inner
}

fn match_body(ui: &egui::Ui, entry: &crate::app::app_state::SearchResultEntry) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    append_text(
        &mut job,
        &format!("{}: ", entry.line_number),
        text_muted(ui),
        15.0,
    );
    append_text(&mut job, &match_preview(entry), text_primary(ui), 16.0);
    job.into()
}

fn match_preview(entry: &crate::app::app_state::SearchResultEntry) -> String {
    if entry.preview.is_empty() {
        format!("Column {}", entry.column_number)
    } else {
        entry.preview.clone()
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
