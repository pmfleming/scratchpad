use super::result_model::{
    active_match_local_row, empty_message, match_count_label, match_preview,
    search_result_group_key,
};
use super::state::{
    SearchStripActions, SearchStripState, row_top, virtual_rows_for_clip, virtual_total_height,
};
use crate::app::app_state::{ScratchpadApp, SearchResultEntry, SearchResultGroup};
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme::{border, tab_selected_accent, tab_selected_bg, text_muted, text_primary};
use crate::app::ui::widget_ids;
use eframe::egui;
use egui_phosphor::regular::{CARET_DOWN, CARET_RIGHT};

const SEARCH_RESULT_FILE_SPACING: f32 = 10.0;
const SEARCH_RESULT_LINE_SPACING: f32 = 2.0;
const SEARCH_RESULT_LINE_PILL_CORNER_RADIUS: u8 = 8;
const SEARCH_RESULT_FILE_PILL_HEIGHT: f32 = 44.0;
const SEARCH_RESULT_LINE_PILL_HEIGHT: f32 = 34.0;
const SEARCH_RESULT_LINE_GUTTER_WIDTH: f32 = 48.0;
const SEARCH_RESULT_LINE_DIVIDER_GAP: f32 = 8.0;
const SEARCH_RESULT_ROW_OVERSCAN: usize = 2;

#[cfg(test)]
mod tests;

pub(super) fn show_search_results(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    let results_height = ui.available_height().max(SEARCH_RESULT_FILE_PILL_HEIGHT);
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

            let mut collapsed_height = None;
            let mut rendered = 0;
            for (index, group) in state.result_groups.iter().enumerate() {
                let (height, painted) = show_result_group(
                    ui,
                    app,
                    index,
                    group,
                    state.active_match_index,
                    actions,
                    collapsed_height,
                );
                collapsed_height = height.or(collapsed_height);
                rendered += usize::from(painted);
                if index + 1 < state.result_groups.len() {
                    ui.add_space(SEARCH_RESULT_FILE_SPACING);
                }
            }
            crate::app::capacity_metrics::record_search_result_groups(
                rendered,
                state.result_groups.len() - rendered,
            );
        });
}

fn show_result_group(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    group_index: usize,
    group: &SearchResultGroup,
    active_match_index: Option<usize>,
    actions: &mut SearchStripActions,
    collapsed_height: Option<f32>,
) -> (Option<f32>, bool) {
    let active_id = search_result_group_expanded_id(group);
    let pending_id = search_result_group_expanded_pending_id(group);
    let expanded = widget_ids::read_deferred_persisted::<bool>(ui.ctx(), pending_id, active_id)
        .unwrap_or(group.active && active_match_index.is_some());
    // Calibrate from a real row each frame so font/style/scale changes cannot
    // leave a stale height. Expanded groups retain their match-row virtualization
    // and active-match reveal even when their header is outside the viewport.
    if !expanded && let Some(height) = collapsed_height {
        let size = egui::vec2(ui.available_width(), height);
        let rect = egui::Rect::from_min_size(ui.cursor().min, size);
        if !ui.is_rect_visible(rect.expand(height * SEARCH_RESULT_ROW_OVERSCAN as f32)) {
            ui.allocate_space(size);
            return (Some(height), false);
        }
    }
    let response = widget_ids::surface_scope(ui, ("search_result_group", group_index), |ui| {
        if app.state.search_state.panel.replace_open
            && app.state.search_state.runtime.status == crate::app::app_state::SearchStatus::Ready
        {
            let indices =
                group.first_match_index..group.first_match_index + group.total_match_count;
            let count = indices
                .clone()
                .filter(|i| !app.state.search_state.replace.excluded.contains(i))
                .count();
            let mut included = count > 0;
            let response = ui.add(
                egui::Checkbox::new(&mut included, "Include file")
                    .indeterminate(count > 0 && count < group.total_match_count),
            );
            if response.changed() {
                for index in indices {
                    if included {
                        app.state.search_state.replace.excluded.remove(&index);
                    } else {
                        app.state.search_state.replace.excluded.insert(index);
                    }
                }
                app.state
                    .search_state
                    .replace
                    .pending_replace_all_confirmation = None;
            }
        }
        let (group_response, toggle_requested) = show_group_pill(ui, group_index, group, expanded);
        if group_response.clicked() && group.total_match_count > 0 {
            actions.focused_file_match_index = Some(group.first_match_index);
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
                show_virtual_match_rows(ui, app, group, active_match_index, actions);
            },
        );
    });
    ((!expanded).then_some(response.response.rect.height()), true)
}

fn show_virtual_match_rows(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    group: &SearchResultGroup,
    active_match_index: Option<usize>,
    actions: &mut SearchStripActions,
) {
    let total_rows = group.total_match_count;
    if total_rows == 0 {
        return;
    }

    let start_y = ui.cursor().min.y;
    let total_height = result_rows_height(total_rows);
    maybe_scroll_active_match_into_view(ui, group, active_match_index, start_y);
    let visible_rows = virtual_rows_for_clip(
        total_rows,
        start_y,
        ui.clip_rect(),
        SEARCH_RESULT_LINE_PILL_HEIGHT,
        SEARCH_RESULT_LINE_SPACING,
        SEARCH_RESULT_ROW_OVERSCAN,
    );

    if visible_rows.first >= visible_rows.last {
        ui.add_space(total_height);
        return;
    }

    ui.add_space(visible_rows.leading_space);
    for row_offset in visible_rows.first..visible_rows.last {
        let match_index = group.first_match_index + row_offset;
        if let Some(entry) = app.search_result_entry_at(match_index) {
            if app.state.search_state.panel.replace_open
                && app.state.search_state.runtime.status
                    == crate::app::app_state::SearchStatus::Ready
            {
                ui.horizontal(|ui| {
                    let mut included = !app.state.search_state.replace.excluded.contains(&match_index);
                    if ui.checkbox(&mut included, "").accessible_name(format!("Include match {}", match_index+1)).changed() {
                        if included { app.state.search_state.replace.excluded.remove(&match_index); } else { app.state.search_state.replace.excluded.insert(match_index); }
                        app.state.search_state.replace.pending_replace_all_confirmation = None;
                    }
                    let matched = app.state.search_state.results.matches.get(match_index).cloned();
                    let replacement = matched.as_ref().and_then(|m| crate::app::app_state::search_state::replace::replacement_for_match(app, m).ok());
                    if let (Some(matched), Some(replacement)) = (matched, replacement) {
                        let old = crate::app::app_state::search_state::replace::matched_text_for_search_match(app, &matched).unwrap_or_default();
                        let mut job = egui::text::LayoutJob::default();
                        append_text(&mut job, &format!("{}  ", entry.line_number), text_muted(ui), 12.5);
                        append_text(&mut job, &old.replace('\n', "⏎"), crate::app::theme::diff_del(ui), 12.5);
                        append_text(&mut job, " → ", text_muted(ui), 12.5);
                        append_text(&mut job, &replacement.replace('\n', "⏎"), crate::app::theme::diff_add(ui), 12.5);
                        if ui.add_sized([ui.available_width(), SEARCH_RESULT_LINE_PILL_HEIGHT], egui::Button::new(job).frame(false).selected(entry.active).truncate()).clicked() { actions.selected_match_index = Some(entry.match_index); }
                    } else { ui.add_space(SEARCH_RESULT_LINE_PILL_HEIGHT); }
                });
            } else if show_match_pill(ui, &entry).clicked() {
                actions.selected_match_index = Some(entry.match_index);
            }
        } else {
            ui.add_space(SEARCH_RESULT_LINE_PILL_HEIGHT);
        }
        if row_offset + 1 < total_rows {
            ui.add_space(SEARCH_RESULT_LINE_SPACING);
        }
    }

    ui.add_space(visible_rows.trailing_space);
}

fn result_row_top(row_index: usize) -> f32 {
    row_top(
        row_index,
        SEARCH_RESULT_LINE_PILL_HEIGHT,
        SEARCH_RESULT_LINE_SPACING,
    )
}

fn result_rows_height(row_count: usize) -> f32 {
    virtual_total_height(
        row_count,
        SEARCH_RESULT_LINE_PILL_HEIGHT,
        SEARCH_RESULT_LINE_SPACING,
    )
}

fn maybe_scroll_active_match_into_view(
    ui: &mut egui::Ui,
    group: &SearchResultGroup,
    active_match_index: Option<usize>,
    start_y: f32,
) {
    let Some(active_match_index) = active_match_index else {
        return;
    };
    let Some(local_row) = active_match_local_row(group, Some(active_match_index)) else {
        return;
    };

    let reveal_id = search_result_active_reveal_id(group);
    let already_revealed = ui
        .ctx()
        .data(|data| data.get_temp::<usize>(reveal_id))
        .is_some_and(|revealed| revealed == active_match_index);
    if already_revealed {
        return;
    }

    let rect = egui::Rect::from_min_size(
        egui::pos2(
            ui.available_rect_before_wrap().left(),
            start_y + result_row_top(local_row),
        ),
        egui::vec2(
            ui.available_width().max(1.0),
            SEARCH_RESULT_LINE_PILL_HEIGHT,
        ),
    );
    ui.scroll_to_rect(rect, Some(egui::Align::Center));
    ui.ctx()
        .data_mut(|data| data.insert_temp(reveal_id, active_match_index));
}

fn search_result_group_expanded_id(group: &SearchResultGroup) -> egui::Id {
    widget_ids::ctx_key((
        "search_result_group.expanded",
        search_result_group_key(group),
    ))
}

fn search_result_group_expanded_pending_id(group: &SearchResultGroup) -> egui::Id {
    widget_ids::ctx_key((
        "search_result_group.expanded.pending",
        search_result_group_key(group),
    ))
}

fn search_result_active_reveal_id(group: &SearchResultGroup) -> egui::Id {
    widget_ids::ctx_key((
        "search_result.active_revealed",
        search_result_group_key(group),
    ))
}

fn show_group_pill(
    ui: &mut egui::Ui,
    group_index: usize,
    group: &SearchResultGroup,
    expanded: bool,
) -> (egui::Response, bool) {
    ui.separator();
    let inner = ui.horizontal(|ui| {
        let caret_clicked = show_group_caret(ui, group_index, expanded).clicked();
        let response = show_group_body(ui, group_index, group);
        show_group_match_count(ui, group.total_match_count);
        (response, caret_clicked)
    });
    inner.inner
}

fn show_group_caret(ui: &mut egui::Ui, group_index: usize, expanded: bool) -> egui::Response {
    widget_ids::surface_scope(ui, ("search_result_group.caret", group_index), |ui| {
        ui.add_sized(
            egui::vec2(26.0, 26.0),
            egui::Button::new(
                egui::RichText::new(group_caret_icon(expanded))
                    .font(egui_phosphor::font_id(14.0))
                    .color(text_muted(ui)),
            )
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE),
        )
    })
    .inner
    .on_hover_text(group_caret_tooltip(expanded))
}

fn group_caret_icon(expanded: bool) -> &'static str {
    if expanded { CARET_DOWN } else { CARET_RIGHT }
}

fn group_caret_tooltip(expanded: bool) -> &'static str {
    if expanded {
        "Collapse results for this file"
    } else {
        "Expand results for this file"
    }
}

fn show_group_body(
    ui: &mut egui::Ui,
    group_index: usize,
    group: &SearchResultGroup,
) -> egui::Response {
    let label_width = (ui.available_width() - 110.0).max(120.0);
    let response = widget_ids::surface_scope(ui, ("search_result_group.body", group_index), |ui| {
        ui.add_sized(
            egui::vec2(label_width, SEARCH_RESULT_FILE_PILL_HEIGHT),
            egui::Button::new(group_body(ui, group))
                .truncate()
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
        )
    })
    .inner
    .on_hover_text(&group.buffer_label);
    if group.tab_label == group.buffer_label {
        response
    } else {
        response.on_hover_text(format!("Tab: {}", group.tab_label))
    }
}

fn show_group_match_count(ui: &mut egui::Ui, match_count: usize) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(
            egui::RichText::new(match_count_label(match_count))
                .size(12.0)
                .color(text_muted(ui)),
        );
    });
}

fn show_match_pill(ui: &mut egui::Ui, entry: &SearchResultEntry) -> egui::Response {
    widget_ids::surface_scope(ui, ("search_result.match", entry.match_index), |ui| {
        let row_width = ui.available_width();
        let response = widget_ids::allocate_exact_interact(
            ui,
            egui::vec2(row_width, SEARCH_RESULT_LINE_PILL_HEIGHT),
            widget_ids::surface_id(("search_result.match", entry.match_index)),
            egui::Sense::click(),
            "search_result.match",
        );
        let rect = response.rect;
        ui.painter().rect_filled(
            rect,
            SEARCH_RESULT_LINE_PILL_CORNER_RADIUS,
            match_fill(ui, entry.active),
        );

        paint_match_row(ui, rect.shrink2(egui::vec2(10.0, 0.0)), entry);
        response
    })
    .inner
}

fn group_body(ui: &egui::Ui, group: &SearchResultGroup) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    append_text(
        &mut job,
        &group.buffer_label,
        if group.active {
            tab_selected_accent(ui)
        } else {
            text_muted(ui)
        },
        12.5,
    );
    job.into()
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

fn match_fill(ui: &egui::Ui, active: bool) -> egui::Color32 {
    if active {
        tab_selected_bg(ui)
    } else {
        egui::Color32::TRANSPARENT
    }
}
