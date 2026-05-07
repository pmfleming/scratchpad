use super::model::{
    TextHistoryAction, TextHistoryFileGroup, TextHistoryRow, per_file_now_line_insert_index,
    timeline_now_line_insert_index,
};
use super::{
    HISTORY_PILL_CORNER_RADIUS, HISTORY_PILL_INNER_MARGIN, HISTORY_PILL_SPACING,
    TEXT_HISTORY_LIST_MIN_HEIGHT, dim_if, truncated_label,
};
use crate::app::theme::{action_bg, action_hover_bg, border, tab_selected_accent};
use crate::app::ui::{callout, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{CARET_DOWN, CARET_RIGHT};

const HISTORY_PILL_ICON_SIZE: f32 = 16.0;
const NOW_LINE_HEIGHT: f32 = 22.0;

pub(super) fn render_timeline(
    ui: &mut egui::Ui,
    rows: &[TextHistoryRow],
    action: &mut Option<TextHistoryAction>,
) {
    render_history_section(
        ui,
        "text_history.section.timeline",
        rows.is_empty(),
        "No entries",
        |ui| render_timeline_rows(ui, rows, action),
    );
}

pub(super) fn render_by_file(
    ui: &mut egui::Ui,
    groups: &[TextHistoryFileGroup],
    action: &mut Option<TextHistoryAction>,
) {
    render_history_section(
        ui,
        "text_history.section.by_file",
        groups.is_empty(),
        "No file history",
        |ui| {
            for (index, group) in groups.iter().enumerate() {
                render_file_group(ui, group, action);
                if index + 1 < groups.len() {
                    ui.add_space(12.0);
                }
            }
        },
    );
}

fn render_history_section(
    ui: &mut egui::Ui,
    scope_id: &'static str,
    is_empty: bool,
    empty_label: &'static str,
    show: impl FnOnce(&mut egui::Ui),
) {
    widget_ids::scope(ui, scope_id, |ui| {
        if is_empty {
            ui.label(
                egui::RichText::new(empty_label)
                    .size(13.0)
                    .color(callout::muted_text(ui)),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(widget_ids::ctx_key("text_history.scroll.content"))
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .min_scrolled_height(TEXT_HISTORY_LIST_MIN_HEIGHT)
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                show(ui);
            });
    });
}

fn render_timeline_rows(
    ui: &mut egui::Ui,
    rows: &[TextHistoryRow],
    action: &mut Option<TextHistoryAction>,
) {
    let now_line_index = timeline_now_line_insert_index(rows);
    let mut now_rendered = false;

    for (idx, row) in rows.iter().enumerate() {
        if !now_rendered && now_line_index == Some(idx) {
            render_now_line(ui);
            now_rendered = true;
        }
        render_row(ui, row, action);
    }
}

fn render_file_group(
    ui: &mut egui::Ui,
    group: &TextHistoryFileGroup,
    action: &mut Option<TextHistoryAction>,
) {
    widget_ids::scope(ui, ("text_history.file_group", group.buffer_id), |ui| {
        let expansion_id =
            widget_ids::local(ui, ("text_history.file_group.expanded", group.buffer_id));
        let expanded = ui
            .data_mut(|data| data.get_persisted::<bool>(expansion_id))
            .unwrap_or(true);
        let (group_response, toggle_requested) = render_file_header_pill(ui, group, expanded);
        if group_response.clicked() || toggle_requested {
            ui.data_mut(|data| data.insert_persisted(expansion_id, !expanded));
        }
        if !expanded {
            return;
        }
        ui.add_space(HISTORY_PILL_SPACING);
        ui.indent(
            widget_ids::local(ui, ("text_history.file_group.indent", group.buffer_id)),
            |ui| render_file_history_rows(ui, &group.rows, action),
        );
    });
}

fn render_file_header_pill(
    ui: &mut egui::Ui,
    group: &TextHistoryFileGroup,
    expanded: bool,
) -> (egui::Response, bool) {
    egui::Frame::NONE
        .fill(action_bg(ui))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(HISTORY_PILL_CORNER_RADIUS))
        .inner_margin(egui::Margin::same(HISTORY_PILL_INNER_MARGIN))
        .show(ui, |ui| {
            let content_width = ui.available_width();
            ui.set_width(content_width);
            ui.set_min_width(content_width);
            ui.set_max_width(content_width);
            render_file_header_contents(ui, group, expanded)
        })
        .inner
}

fn render_file_header_contents(
    ui: &mut egui::Ui,
    group: &TextHistoryFileGroup,
    expanded: bool,
) -> (egui::Response, bool) {
    let mut toggle_requested = false;
    let group_response = ui
        .horizontal(|ui| {
            if file_group_caret(ui, expanded).clicked() {
                toggle_requested = true;
            }
            let label_width = (ui.available_width() - 96.0).max(120.0);
            let response = truncated_label(
                ui,
                &group.label,
                label_width,
                13.0,
                callout::text(ui),
                egui::Sense::click(),
            )
            .on_hover_text(&group.label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(change_count_label(group.rows.len()))
                        .size(12.0)
                        .color(callout::muted_text(ui)),
                );
            });
            response
        })
        .inner;

    (group_response, toggle_requested)
}

fn file_group_caret(ui: &mut egui::Ui, expanded: bool) -> egui::Response {
    let (caret_icon, caret_tooltip) = if expanded {
        (CARET_DOWN, "Collapse history for this file")
    } else {
        (CARET_RIGHT, "Expand history for this file")
    };
    ui.add_sized(
        egui::vec2(24.0, 24.0),
        egui::Button::new(
            egui::RichText::new(caret_icon)
                .size(14.0)
                .color(callout::muted_text(ui)),
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE),
    )
    .on_hover_text(caret_tooltip)
}

fn change_count_label(count: usize) -> String {
    match count {
        1 => "1 change".to_owned(),
        count => format!("{count} changes"),
    }
}

fn render_file_history_rows(
    ui: &mut egui::Ui,
    rows: &[TextHistoryRow],
    action: &mut Option<TextHistoryAction>,
) {
    let now_line_index = per_file_now_line_insert_index(rows);
    let mut now_rendered = false;

    for (idx, row) in rows.iter().enumerate() {
        if !now_rendered && now_line_index == Some(idx) {
            render_now_line(ui);
            now_rendered = true;
        }
        render_row(ui, row, action);
    }
}

fn render_row(ui: &mut egui::Ui, row: &TextHistoryRow, action: &mut Option<TextHistoryAction>) {
    let response = widget_ids::scope(
        ui,
        ("text_history.row", row.buffer_id, row.entry_id),
        |ui| history_pill(ui, row),
    )
    .inner
    .on_hover_text(if row.undone {
        "Click to redo this text change"
    } else {
        "Click to undo this text change"
    });

    if response.clicked() {
        *action = Some(TextHistoryAction {
            buffer_id: row.buffer_id,
            entry_id: row.entry_id,
        });
    }
    ui.add_space(HISTORY_PILL_SPACING);
}

fn history_pill(ui: &mut egui::Ui, row: &TextHistoryRow) -> egui::Response {
    let frame_id = widget_ids::local(ui, "text_history.row.pill");
    let hovered = ui
        .ctx()
        .read_response(frame_id)
        .map(|r| r.hovered() || r.contains_pointer())
        .unwrap_or(false);

    let base_fill = if hovered {
        action_hover_bg(ui)
    } else {
        action_bg(ui)
    };
    let fill = dim_if(row.undone, base_fill);
    let stroke = dim_if(row.undone, border(ui));
    let title_color = dim_if(row.undone, callout::text(ui));
    let muted_color = dim_if(row.undone, callout::muted_text(ui));

    let inner = egui::Frame::NONE
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(HISTORY_PILL_CORNER_RADIUS))
        .inner_margin(egui::Margin::same(HISTORY_PILL_INNER_MARGIN))
        .show(ui, |ui| {
            render_row_pill_contents(ui, row, title_color, muted_color)
        });

    widget_ids::interact(
        ui,
        inner.response.rect,
        frame_id,
        egui::Sense::click(),
        "text_history.row.pill",
    )
}

fn render_row_pill_contents(
    ui: &mut egui::Ui,
    row: &TextHistoryRow,
    title_color: egui::Color32,
    muted_color: egui::Color32,
) {
    let content_width = ui.available_width();
    ui.set_width(content_width);
    ui.set_min_width(content_width);
    ui.set_max_width(content_width);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(row.icon)
                .font(egui::FontId::proportional(HISTORY_PILL_ICON_SIZE))
                .color(muted_color),
        );
        ui.add_space(8.0);
        ui.vertical(|ui| {
            let text_width = ui.available_width().max(0.0);
            truncated_label(
                ui,
                &row.title,
                text_width,
                14.0,
                title_color,
                egui::Sense::hover(),
            )
            .on_hover_text(&row.title);
            truncated_label(
                ui,
                &row.detail,
                text_width,
                12.0,
                muted_color,
                egui::Sense::hover(),
            )
            .on_hover_text(&row.detail);
        });
    });
}

fn render_now_line(ui: &mut egui::Ui) {
    let accent = tab_selected_accent(ui);
    let label_font = egui::FontId::proportional(11.0);

    let rect =
        widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), NOW_LINE_HEIGHT));
    let painter = ui.painter_at(rect);
    let mid_y = rect.center().y;
    let label_galley = painter.layout_no_wrap("Now".to_owned(), label_font, accent);
    let line_start_x = rect.left() + 4.0 + label_galley.size().x + 8.0;
    let line_end_x = rect.right() - 4.0;

    painter.galley(
        egui::pos2(rect.left() + 4.0, mid_y - label_galley.size().y * 0.5),
        label_galley,
        accent,
    );
    if line_end_x > line_start_x {
        painter.line_segment(
            [
                egui::pos2(line_start_x, mid_y),
                egui::pos2(line_end_x, mid_y),
            ],
            egui::Stroke::new(1.5, accent),
        );
    }
    ui.add_space(HISTORY_PILL_SPACING);
}
