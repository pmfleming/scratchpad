mod model;
mod persistence;

use self::model::{
    TextHistoryAction, TextHistoryFileGroup, TextHistoryRow, file_groups_from_entries,
    per_file_now_line_insert_index, row_from_entry, timeline_now_line_anchors,
};
use self::persistence::{read_active_tab, read_follow_focus, write_active_tab, write_follow_focus};
use super::common::show_centered_callout;
use crate::app::app_state::ScratchpadApp;
use crate::app::theme::{action_bg, action_hover_bg, border, tab_selected_accent, tab_selected_bg};
use crate::app::ui::settings::dialog_card_frame;
use crate::app::ui::{callout, settings, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{
    CARET_DOWN, CARET_RIGHT, CLOCK_COUNTER_CLOCKWISE, CROSSHAIR, FILES, TRASH,
};

const TEXT_HISTORY_SIZE: egui::Vec2 =
    egui::vec2(crate::app::ui::search_replace::SEARCH_DIALOG_WIDTH, 520.0);
const TEXT_HISTORY_TITLE_SIZE: f32 = 24.0;
const TEXT_HISTORY_LIST_MIN_HEIGHT: f32 = 330.0;
const HISTORY_PILL_CORNER_RADIUS: u8 = 8;
const HISTORY_PILL_INNER_MARGIN: i8 = 10;
const HISTORY_PILL_ICON_SIZE: f32 = 16.0;
const HISTORY_PILL_SPACING: f32 = 6.0;
const NOW_LINE_HEIGHT: f32 = 22.0;
const TAB_BUTTON_HEIGHT: f32 = 30.0;
const HISTORY_CARD_CORNER_RADIUS: u8 = 12;
const UNDONE_OPACITY: f32 = 0.55;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HistoryTab {
    Timeline,
    ByFile,
}

struct TextHistoryWindowInputs<'a> {
    timeline_rows: &'a [TextHistoryRow],
    file_groups: &'a [TextHistoryFileGroup],
    active_tab: HistoryTab,
    follow_focus: bool,
}

struct TextHistoryWindowState<'a> {
    next_tab: &'a mut HistoryTab,
    next_follow_focus: &'a mut bool,
    action: &'a mut Option<TextHistoryAction>,
    close_requested: &'a mut bool,
    clear_requested: &'a mut bool,
}

pub(crate) fn show_text_history_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.text_history_open {
        return;
    }

    let entries = app.cached_text_history_entries();
    let timeline_rows = entries.iter().rev().map(row_from_entry).collect::<Vec<_>>();
    let file_groups = file_groups_from_entries(entries.iter());

    let mut action: Option<TextHistoryAction> = None;
    let mut close_requested = false;
    let mut clear_requested = false;
    let active_tab = read_active_tab(ctx);
    let mut next_tab = active_tab;
    let follow_focus = read_follow_focus(ctx);
    let mut next_follow_focus = follow_focus;

    show_centered_callout(ctx, "text_history_window", TEXT_HISTORY_SIZE, |ui| {
        widget_ids::feature_scope(ui, "text_history_dialog", |ui| {
            ui.set_width(TEXT_HISTORY_SIZE.x);
            ui.set_min_width(TEXT_HISTORY_SIZE.x);
            ui.set_max_width(TEXT_HISTORY_SIZE.x);
            render_text_history_window(
                ui,
                TextHistoryWindowInputs {
                    timeline_rows: &timeline_rows,
                    file_groups: &file_groups,
                    active_tab,
                    follow_focus,
                },
                TextHistoryWindowState {
                    next_tab: &mut next_tab,
                    next_follow_focus: &mut next_follow_focus,
                    action: &mut action,
                    close_requested: &mut close_requested,
                    clear_requested: &mut clear_requested,
                },
            );
        });
    });

    if next_tab != active_tab {
        write_active_tab(ctx, next_tab);
    }
    if next_follow_focus != follow_focus {
        write_follow_focus(ctx, next_follow_focus);
    }
    if close_requested {
        app.close_text_history();
    }
    if clear_requested {
        let _ = app.clear_text_history();
    }
    if let Some(action) = action {
        let _ =
            app.apply_text_history_to_entry(action.buffer_id, action.entry_id, next_follow_focus);
    }
}

fn render_text_history_window(
    ui: &mut egui::Ui,
    inputs: TextHistoryWindowInputs<'_>,
    state: TextHistoryWindowState<'_>,
) {
    settings::apply_dialog_typography(ui);
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);
    if render_header(ui) {
        *state.close_requested = true;
    }
    ui.add_space(4.0);
    history_card(ui, |ui| {
        render_controls(
            ui,
            inputs.active_tab,
            state.next_tab,
            inputs.follow_focus,
            state.next_follow_focus,
            !inputs.timeline_rows.is_empty(),
            state.clear_requested,
        );
    });

    match inputs.active_tab {
        HistoryTab::Timeline => render_timeline(ui, inputs.timeline_rows, state.action),
        HistoryTab::ByFile => render_by_file(ui, inputs.file_groups, state.action),
    }
}

fn history_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let width = ui.available_width();
    let content_width = (width - 24.0).max(0.0);
    dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(HISTORY_CARD_CORNER_RADIUS))
        .show(ui, |ui| {
            ui.set_width(content_width);
            ui.set_min_width(content_width);
            ui.set_max_width(content_width);
            add_contents(ui)
        })
        .inner
}

fn render_header(ui: &mut egui::Ui) -> bool {
    callout::header_row(ui, "text_history.header", "Close history", |ui| {
        ui.label(
            egui::RichText::new("History")
                .size(TEXT_HISTORY_TITLE_SIZE)
                .color(callout::text(ui)),
        );
    })
}

fn render_controls(
    ui: &mut egui::Ui,
    active: HistoryTab,
    next: &mut HistoryTab,
    follow_focus: bool,
    next_follow_focus: &mut bool,
    has_history: bool,
    clear_requested: &mut bool,
) {
    let tabs = [
        (
            "timeline",
            CLOCK_COUNTER_CLOCKWISE,
            "Timeline",
            HistoryTab::Timeline,
        ),
        ("by_file", FILES, "By file", HistoryTab::ByFile),
    ];

    ui.horizontal(|ui| {
        for (id_source, icon, tooltip, tab) in tabs {
            if control_icon_button(ui, id_source, icon, tooltip, active == tab, true).clicked() {
                *next = tab;
            }
        }
        if control_icon_button(
            ui,
            "follow_focus",
            CROSSHAIR,
            if follow_focus {
                "Follow undo is on"
            } else {
                "Follow undo is off"
            },
            follow_focus,
            true,
        )
        .clicked()
        {
            *next_follow_focus = !follow_focus;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if control_icon_button(
                ui,
                "clear_history",
                TRASH,
                "Clear all text history",
                false,
                has_history,
            )
            .clicked()
            {
                *clear_requested = true;
            }
        });
    });
}

fn control_icon_button(
    ui: &mut egui::Ui,
    id_source: &'static str,
    icon: &str,
    tooltip: &str,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    let (fill, stroke_color, text_color) = if selected {
        (
            tab_selected_bg(ui),
            tab_selected_accent(ui),
            callout::text(ui),
        )
    } else {
        (action_bg(ui), border(ui), callout::muted_text(ui))
    };
    let button = egui::Button::new(
        egui::RichText::new(icon)
            .font(egui::FontId::proportional(16.0))
            .color(if enabled {
                text_color
            } else {
                text_color.gamma_multiply(UNDONE_OPACITY)
            }),
    )
    .min_size(egui::vec2(36.0, TAB_BUTTON_HEIGHT))
    .fill(fill)
    .stroke(egui::Stroke::new(1.0, stroke_color))
    .corner_radius(egui::CornerRadius::same(8));
    widget_ids::scope(ui, ("text_history.control", id_source), |ui| {
        ui.add_enabled(enabled, button)
    })
    .inner
    .on_hover_text(tooltip)
}

fn render_timeline(
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
            .id_salt(text_history_content_scroll_id())
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
    let anchors = timeline_now_line_anchors(rows);

    for row in rows {
        if anchors.contains(&(row.buffer_id, row.entry_id)) {
            render_now_line(ui);
        }
        render_row(ui, row, action);
    }
}

fn render_by_file(
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

fn text_history_content_scroll_id() -> egui::Id {
    widget_ids::ctx_key("text_history.scroll.content")
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
            |ui| {
                render_file_history_rows(ui, &group.rows, action);
            },
        );
    });
}

fn render_file_header_pill(
    ui: &mut egui::Ui,
    group: &TextHistoryFileGroup,
    expanded: bool,
) -> (egui::Response, bool) {
    let count_label = match group.rows.len() {
        1 => "1 change".to_owned(),
        count => format!("{count} changes"),
    };
    let (caret_icon, caret_tooltip) = if expanded {
        (CARET_DOWN, "Collapse history for this file")
    } else {
        (CARET_RIGHT, "Expand history for this file")
    };
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
            let mut toggle_requested = false;
            let group_response = ui
                .horizontal(|ui| {
                    let caret = ui
                        .add_sized(
                            egui::vec2(24.0, 24.0),
                            egui::Button::new(
                                egui::RichText::new(caret_icon)
                                    .size(14.0)
                                    .color(callout::muted_text(ui)),
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                        )
                        .on_hover_text(caret_tooltip);
                    if caret.clicked() {
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
                            egui::RichText::new(count_label)
                                .size(12.0)
                                .color(callout::muted_text(ui)),
                        );
                    });
                    response
                })
                .inner;

            (group_response, toggle_requested)
        })
        .inner
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

fn dim_if(undone: bool, color: egui::Color32) -> egui::Color32 {
    if undone {
        color.gamma_multiply(UNDONE_OPACITY)
    } else {
        color
    }
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
        });

    widget_ids::interact(
        ui,
        inner.response.rect,
        frame_id,
        egui::Sense::click(),
        "text_history.row.pill",
    )
}

fn truncated_label(
    ui: &mut egui::Ui,
    text: &str,
    width: f32,
    size: f32,
    color: egui::Color32,
    sense: egui::Sense,
) -> egui::Response {
    ui.add_sized(
        egui::vec2(width, 0.0),
        egui::Label::new(egui::RichText::new(text).size(size).color(color))
            .truncate()
            .sense(sense),
    )
}

fn render_now_line(ui: &mut egui::Ui) {
    let accent = tab_selected_accent(ui);
    let muted = callout::muted_text(ui);
    let label_font = egui::FontId::proportional(11.0);

    let rect =
        widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), NOW_LINE_HEIGHT));
    let painter = ui.painter_at(rect);
    let mid_y = rect.center().y;
    let label = "Now";
    let label_galley = painter.layout_no_wrap(label.to_owned(), label_font.clone(), accent);
    let label_width = label_galley.size().x;
    let gap = 8.0;
    let label_x = rect.left() + 4.0;
    let line_start_x = label_x + label_width + gap;
    let line_end_x = rect.right() - 4.0;

    painter.galley(
        egui::pos2(label_x, mid_y - label_galley.size().y * 0.5),
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
    let _ = muted;
    ui.add_space(HISTORY_PILL_SPACING);
}

#[cfg(test)]
mod tests;
