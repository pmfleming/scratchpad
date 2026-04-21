use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{SearchFreshness, SearchResultEntry, SearchResultGroup, SearchStatus};
use crate::app::theme::{
    action_bg, action_hover_bg, border, tab_selected_accent, tab_selected_bg, text_muted,
    text_primary,
};
use crate::app::ui::settings;
use eframe::egui;
use egui_phosphor::regular::{CARET_DOWN, CARET_RIGHT, FILE_TEXT};

const SEARCH_RESULT_CARD_HEIGHT: f32 = 44.0;
const SEARCH_RESULT_VISIBLE_ROWS: usize = 4;
const SEARCH_RESULT_CARD_CORNER_RADIUS: u8 = 12;
const SEARCH_RESULT_PILL_CORNER_RADIUS: u8 = 10;
const SEARCH_RESULT_ROW_SPACING: f32 = 8.0;
const SEARCH_RESULT_LINE_HEIGHT: f32 = 30.0;

pub(super) fn show_search_results(
    ui: &mut egui::Ui,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    let results_height = (SEARCH_RESULT_VISIBLE_ROWS as f32)
        * (SEARCH_RESULT_CARD_HEIGHT + SEARCH_RESULT_ROW_SPACING)
        - SEARCH_RESULT_ROW_SPACING;

    settings::dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(SEARCH_RESULT_CARD_CORNER_RADIUS))
        .show(ui, |ui| {
            render_results_header(ui, state);

            let result_groups = if state.query.is_empty() || status_message(state).is_some() {
                Vec::new()
            } else {
                state.result_groups.clone()
            };

            let empty_message = if state.query.is_empty() {
                Some("Type to search across the selected scope.")
            } else if status_message(state).is_some() {
                status_message(state)
            } else if result_groups.is_empty() {
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
                        for (index, group) in result_groups.iter().enumerate() {
                            show_result_group(ui, group, index, actions);
                            if index + 1 < result_groups.len() {
                                ui.add_space(SEARCH_RESULT_ROW_SPACING);
                            }
                        }
                    }
                });
        });
}

fn render_results_header(ui: &mut egui::Ui, state: &SearchStripState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(FILE_TEXT)
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

fn show_result_group(
    ui: &mut egui::Ui,
    group: &SearchResultGroup,
    group_index: usize,
    actions: &mut SearchStripActions,
) {
    let group_id = ui.make_persistent_id((
        "search_result_group",
        group.tab_index,
        group.buffer_id,
        group_index,
    ));
    let mut open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(group_id))
        .unwrap_or(false);

    egui::Frame::NONE
        .fill(match_fill(ui, group.active))
        .stroke(egui::Stroke::new(1.0, match_border(ui, group.active)))
        .corner_radius(egui::CornerRadius::same(SEARCH_RESULT_PILL_CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            let header = ui
                .horizontal(|ui| {
                    let toggle = ui.add(
                        egui::Button::new(
                            egui::RichText::new(if open { CARET_DOWN } else { CARET_RIGHT })
                                .size(16.0)
                                .color(text_muted(ui)),
                        )
                        .min_size(egui::vec2(24.0, 24.0))
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE),
                    );
                    if toggle.clicked() {
                        open = !open;
                    }

                    let label = ui.add_sized(
                        egui::vec2(ui.available_width() - 56.0, 28.0),
                        egui::Button::new(file_group_body(ui, group))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    );
                    if label.clicked() {
                        actions.focused_file_match_index =
                            group.entries.first().map(|entry| entry.match_index);
                    }
                    label.on_hover_text(group.tab_label.clone());
                })
                .response;

            if open {
                ui.add_space(8.0);
                for (entry_index, entry) in group.entries.iter().enumerate() {
                    if show_match_row(ui, entry).clicked() {
                        actions.selected_match_index = Some(entry.match_index);
                    }
                    if entry_index + 1 < group.entries.len() {
                        ui.add_space(6.0);
                    }
                }
            }

            header
        });

    ui.ctx().data_mut(|data| data.insert_temp(group_id, open));
}

fn file_group_body(ui: &egui::Ui, group: &SearchResultGroup) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();

    append_text(&mut job, &group.buffer_label, text_primary(ui), 13.5);
    append_text(&mut job, "  ", text_muted(ui), 12.0);
    append_text(
        &mut job,
        &match_count_label(group.total_match_count),
        text_muted(ui),
        12.0,
    );

    job.into()
}

fn show_match_row(ui: &mut egui::Ui, entry: &SearchResultEntry) -> egui::Response {
    egui::Frame::NONE
        .fill(match_row_fill(ui, entry.active))
        .stroke(egui::Stroke::new(1.0, match_row_border(ui, entry.active)))
        .corner_radius(egui::CornerRadius::same(SEARCH_RESULT_PILL_CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.add_sized(
                egui::vec2(ui.available_width(), SEARCH_RESULT_LINE_HEIGHT),
                egui::Button::new(match_body(ui, entry))
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE),
            )
        })
        .inner
}

fn match_body(ui: &egui::Ui, entry: &SearchResultEntry) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();

    append_text(
        &mut job,
        &format!("Line {}:", entry.line_number),
        text_muted(ui),
        12.0,
    );
    append_text(&mut job, " ", text_muted(ui), 12.0);
    append_text(
        &mut job,
        match_preview(entry),
        text_primary(ui).gamma_multiply(0.9),
        12.5,
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

fn match_count_label(count: usize) -> String {
    let suffix = if count == 1 { "match" } else { "matches" };
    format!("{count} {suffix}")
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

fn match_row_fill(ui: &egui::Ui, active: bool) -> egui::Color32 {
    if active {
        tab_selected_bg(ui).gamma_multiply(1.08)
    } else {
        action_hover_bg(ui)
    }
}

fn match_row_border(ui: &egui::Ui, active: bool) -> egui::Color32 {
    if active {
        tab_selected_accent(ui)
    } else {
        border(ui).gamma_multiply(0.8)
    }
}
