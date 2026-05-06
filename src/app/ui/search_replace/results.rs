use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{SearchFreshness, SearchResultEntry, SearchResultGroup, SearchStatus};
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme::{
    action_bg, border, tab_selected_accent, tab_selected_bg, text_muted, text_primary,
};
use crate::app::ui::widget_ids;
use eframe::egui;
use egui_phosphor::regular::{CARET_DOWN, CARET_RIGHT};

const SEARCH_RESULT_VISIBLE_ROWS: usize = 5;
const SEARCH_RESULT_FILE_SPACING: f32 = 10.0;
const SEARCH_RESULT_LINE_SPACING: f32 = 6.0;
const SEARCH_RESULT_FILE_PILL_CORNER_RADIUS: u8 = 10;
const SEARCH_RESULT_LINE_PILL_CORNER_RADIUS: u8 = 8;
const SEARCH_RESULT_FILE_PILL_HEIGHT: f32 = 44.0;
const SEARCH_RESULT_LINE_PILL_HEIGHT: f32 = 40.0;
const SEARCH_RESULT_LINE_GUTTER_WIDTH: f32 = 48.0;
const SEARCH_RESULT_LINE_DIVIDER_GAP: f32 = 8.0;

pub(super) fn show_search_results(
    ui: &mut egui::Ui,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    let results_height = (SEARCH_RESULT_VISIBLE_ROWS as f32)
        * (SEARCH_RESULT_FILE_PILL_HEIGHT + SEARCH_RESULT_FILE_SPACING)
        - SEARCH_RESULT_FILE_SPACING;
    let empty_message = empty_message(state);

    egui::ScrollArea::vertical()
        .id_salt(widget_ids::scroll_id(ui, "search_results_list"))
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
                return;
            }

            for (index, group) in state.result_groups.iter().enumerate() {
                show_result_group(ui, index, group, actions);
                if index + 1 < state.result_groups.len() {
                    ui.add_space(SEARCH_RESULT_FILE_SPACING);
                }
            }
        });
}

pub(super) fn results_summary(state: &SearchStripState) -> String {
    if state.progress.searching || state.progress.freshness == SearchFreshness::Stale {
        if state.progress.target_count > 0 {
            return format!(
                "Searching {} of {} targets...",
                state
                    .progress
                    .scanned_targets
                    .min(state.progress.target_count),
                state.progress.target_count
            );
        }
        return "Searching\u{2026}".to_owned();
    }

    if state.query.is_empty() {
        return String::new();
    }

    match &state.progress.status {
        SearchStatus::InvalidQuery(_) => return "Invalid query".to_owned(),
        SearchStatus::Error(message) => return message.clone(),
        SearchStatus::Idle
        | SearchStatus::Searching { .. }
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

fn show_result_group(
    ui: &mut egui::Ui,
    group_index: usize,
    group: &SearchResultGroup,
    actions: &mut SearchStripActions,
) {
    widget_ids::surface_scope(ui, ("search_result_group", group_index), |ui| {
        let active_id = search_result_group_expanded_id(group.buffer_id);
        let pending_id = search_result_group_expanded_pending_id(group.buffer_id);
        let expanded = widget_ids::read_deferred_persisted::<bool>(ui.ctx(), pending_id, active_id)
            .unwrap_or(false);

        let (group_response, toggle_requested) = show_group_pill(ui, group_index, group, expanded);
        if group_response.clicked() && !group.entries.is_empty() {
            actions.focused_file_match_index = Some(group.entries[0].match_index);
        }

        if toggle_requested {
            widget_ids::write_deferred_persisted(ui.ctx(), pending_id, !expanded);
        }

        if !expanded {
            return;
        }

        ui.add_space(SEARCH_RESULT_LINE_SPACING);
        ui.indent(
            widget_ids::local(ui, ("search_result_indent", group.buffer_id)),
            |ui| {
                for (index, entry) in group.entries.iter().enumerate() {
                    if show_match_pill(ui, index, entry).clicked() {
                        actions.selected_match_index = Some(entry.match_index);
                    }
                    if index + 1 < group.entries.len() {
                        ui.add_space(SEARCH_RESULT_LINE_SPACING);
                    }
                }
            },
        );
    });
}

fn search_result_group_expanded_id(buffer_id: u64) -> egui::Id {
    widget_ids::ctx_key(("search_result_group.expanded", buffer_id))
}

fn search_result_group_expanded_pending_id(buffer_id: u64) -> egui::Id {
    widget_ids::ctx_key(("search_result_group.expanded.pending", buffer_id))
}

fn show_group_pill(
    ui: &mut egui::Ui,
    group_index: usize,
    group: &SearchResultGroup,
    expanded: bool,
) -> (egui::Response, bool) {
    let fill = match_fill(ui, group.active);
    let stroke = match_border(ui, group.active);

    egui::Frame::NONE
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(
            SEARCH_RESULT_FILE_PILL_CORNER_RADIUS,
        ))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            let mut toggle_requested = false;

            let group_response = ui
                .horizontal(|ui| {
                    let caret = widget_ids::surface_scope(
                        ui,
                        ("search_result_group.caret", group_index),
                        |ui| {
                            ui.add_sized(
                                egui::vec2(26.0, 26.0),
                                egui::Button::new(
                                    egui::RichText::new(if expanded {
                                        CARET_DOWN
                                    } else {
                                        CARET_RIGHT
                                    })
                                    .size(14.0)
                                    .color(text_muted(ui)),
                                )
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(egui::Stroke::NONE),
                            )
                        },
                    )
                    .inner
                    .on_hover_text(if expanded {
                        "Collapse results for this file"
                    } else {
                        "Expand results for this file"
                    });
                    if caret.clicked() {
                        toggle_requested = true;
                    }

                    let label_width = (ui.available_width() - 110.0).max(120.0);
                    let mut response = widget_ids::surface_scope(
                        ui,
                        ("search_result_group.body", group_index),
                        |ui| {
                            ui.add_sized(
                                egui::vec2(label_width, SEARCH_RESULT_FILE_PILL_HEIGHT),
                                egui::Button::new(group_body(ui, group))
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE),
                            )
                        },
                    )
                    .inner;
                    if group.tab_label != group.buffer_label {
                        response = response.on_hover_text(format!("Tab: {}", group.tab_label));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(file_match_count_label(group.total_match_count))
                                .size(12.0)
                                .color(text_muted(ui)),
                        );
                    });
                    response
                })
                .inner;

            (group_response, toggle_requested)
        })
        .inner
}

fn show_match_pill(ui: &mut egui::Ui, index: usize, entry: &SearchResultEntry) -> egui::Response {
    widget_ids::surface_scope(ui, ("search_result.match", index), |ui| {
        egui::Frame::NONE
            .fill(match_fill(ui, entry.active))
            .stroke(egui::Stroke::new(1.0, match_border(ui, entry.active)))
            .corner_radius(egui::CornerRadius::same(
                SEARCH_RESULT_LINE_PILL_CORNER_RADIUS,
            ))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                let row_width = ui.available_width();
                let response = widget_ids::allocate_exact_interact(
                    ui,
                    egui::vec2(row_width, SEARCH_RESULT_LINE_PILL_HEIGHT),
                    widget_ids::surface_id(("search_result.match", index)),
                    egui::Sense::click(),
                    "search_result.match",
                );
                let rect = response.rect;
                paint_match_row(ui, rect, entry);
                response
            })
            .inner
    })
    .inner
}

fn group_body(ui: &egui::Ui, group: &SearchResultGroup) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    append_text(&mut job, &group.buffer_label, text_primary(ui), 13.0);
    job.into()
}

fn file_match_count_label(match_count: usize) -> String {
    if match_count == 1 {
        "1 match".to_owned()
    } else {
        format!("{match_count} matches")
    }
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
            font_id: egui::FontId::new(size, egui::FontFamily::Name(EDITOR_FONT_FAMILY.into())),
            color,
            ..Default::default()
        },
    );
}

fn paint_match_row(ui: &egui::Ui, rect: egui::Rect, entry: &SearchResultEntry) {
    let painter = ui.painter();
    let editor_font = egui::FontId::new(12.5, egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()));
    let gutter_rect = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.left() + SEARCH_RESULT_LINE_GUTTER_WIDTH, rect.bottom()),
    );
    let divider_x = gutter_rect.right() + SEARCH_RESULT_LINE_DIVIDER_GAP;
    let preview_rect = egui::Rect::from_min_max(
        egui::pos2(divider_x + SEARCH_RESULT_LINE_DIVIDER_GAP, rect.top()),
        rect.max,
    );

    painter.text(
        egui::pos2(gutter_rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{:>4}", entry.line_number),
        editor_font.clone(),
        text_muted(ui),
    );

    painter.line_segment(
        [
            egui::pos2(divider_x, rect.top() + 5.0),
            egui::pos2(divider_x, rect.bottom() - 5.0),
        ],
        egui::Stroke::new(1.0, border(ui).gamma_multiply(0.65)),
    );

    painter.with_clip_rect(preview_rect).text(
        egui::pos2(preview_rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        match_preview(entry),
        editor_font,
        text_primary(ui).gamma_multiply(0.92),
    );
}

fn empty_message(state: &SearchStripState) -> Option<&str> {
    if state.query.is_empty() {
        Some("Type to search across the selected scope.")
    } else if let Some(message) = state.progress.status.message() {
        Some(message)
    } else if state.result_groups.is_empty() {
        if state.progress.searching {
            Some("Searching\u{2026}")
        } else {
            Some("No matches found.")
        }
    } else {
        None
    }
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
