use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{SearchFreshness, SearchResultEntry, SearchResultGroup, SearchStatus};
use crate::app::theme::{
    action_bg, border, tab_selected_accent, tab_selected_bg, text_muted, text_primary,
};
use crate::app::ui::settings;
use eframe::egui;

const SEARCH_RESULT_ROW_HEIGHT: f32 = 36.0;
const SEARCH_RESULT_VISIBLE_ROWS: usize = 4;
const SEARCH_RESULT_CARD_CORNER_RADIUS: u8 = 12;
const SEARCH_RESULT_ROW_CORNER_RADIUS: u8 = 10;
const SEARCH_RESULT_ROW_SPACING: f32 = 4.0;

pub(super) fn show_search_results(
    ui: &mut egui::Ui,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    let results_height = (SEARCH_RESULT_VISIBLE_ROWS as f32)
        * (SEARCH_RESULT_ROW_HEIGHT + SEARCH_RESULT_ROW_SPACING)
        + SEARCH_RESULT_ROW_SPACING;

    settings::dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(SEARCH_RESULT_CARD_CORNER_RADIUS))
        .show(ui, |ui| {
            render_results_header(ui, state);

            let all_entries = if state.query.is_empty() || status_message(state).is_some() {
                Vec::new()
            } else {
                collect_all_entries(&state.result_groups)
            };

            let empty_message = if state.query.is_empty() {
                Some("Type to search across the selected scope.")
            } else if status_message(state).is_some() {
                status_message(state)
            } else if all_entries.is_empty() {
                if state.progress.searching {
                    Some("Searching\u{2026}")
                } else {
                    Some("No matches found.")
                }
            } else {
                None
            };

            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .id_salt("search_results_list")
                .max_height(results_height)
                .min_scrolled_height(results_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(message) = empty_message {
                        ui.add_space(results_height * 0.3);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(message)
                                    .size(13.0)
                                    .color(text_muted(ui)),
                            );
                        });
                    } else {
                        for entry in &all_entries {
                            if show_result_row(ui, entry).clicked() {
                                actions.selected_match_index = Some(entry.match_index);
                            }
                            ui.add_space(SEARCH_RESULT_ROW_SPACING);
                        }
                    }
                });
        });
}

fn collect_all_entries(groups: &[SearchResultGroup]) -> Vec<&SearchResultEntry> {
    groups.iter().flat_map(|g| g.entries.iter()).collect()
}

fn render_results_header(ui: &mut egui::Ui, state: &SearchStripState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(egui_phosphor::regular::FILE_TEXT)
                .size(18.0)
                .color(text_muted(ui)),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Search results")
                .size(15.0)
                .color(text_primary(ui)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(results_summary(state))
                    .size(12.0)
                    .color(text_muted(ui)),
            );
        });
    });
}

fn results_summary(state: &SearchStripState) -> String {
    if state.progress.searching || state.progress.freshness == SearchFreshness::Stale {
        return "Searching\u{2026}".to_owned();
    }

    if state.query.is_empty() {
        return String::new();
    }

    match &state.progress.status {
        SearchStatus::InvalidQuery(_) => return "Invalid query".to_owned(),
        SearchStatus::Error(message) => return message.clone(),
        SearchStatus::Idle
        | SearchStatus::Searching
        | SearchStatus::Ready
        | SearchStatus::NoMatches => {}
    }

    if state.progress.displayed_match_count < state.progress.total_match_count {
        return format!(
            "{} of {} matches",
            state.progress.displayed_match_count, state.progress.total_match_count
        );
    }

    let file_count = state.result_groups.len();
    let file_label = if file_count == 1 { "file" } else { "files" };
    format!(
        "{} matches in {} {}",
        state.match_count, file_count, file_label
    )
}

fn status_message(state: &SearchStripState) -> Option<&str> {
    state.progress.status.message()
}

fn show_result_row(ui: &mut egui::Ui, entry: &SearchResultEntry) -> egui::Response {
    let inner = egui::Frame::NONE
        .fill(match_fill(ui, entry.active))
        .stroke(egui::Stroke::new(1.0, match_border(ui, entry.active)))
        .corner_radius(egui::CornerRadius::same(SEARCH_RESULT_ROW_CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            let row_width = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(row_width, SEARCH_RESULT_ROW_HEIGHT - 8.0),
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

    // File name pill-style prefix
    append_text(&mut job, &entry.buffer_label, text_muted(ui), 12.0);
    append_text(&mut job, "  ", text_muted(ui), 12.0);

    // Location
    append_text(
        &mut job,
        &format!("{}:{}", entry.line_number, entry.column_number),
        text_muted(ui).gamma_multiply(0.7),
        12.0,
    );
    append_text(&mut job, "  ", text_muted(ui), 12.0);

    // Preview text — less prominent
    append_text(
        &mut job,
        match_preview(entry),
        text_primary(ui).gamma_multiply(0.85),
        13.0,
    );

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
