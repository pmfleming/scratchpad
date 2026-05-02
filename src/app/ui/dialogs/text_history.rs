use super::common::show_centered_callout;
use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::phosphor_button;
use crate::app::domain::{BufferId, PieceSource, source_label};
use crate::app::text_history::TextHistoryEntryView;
use crate::app::theme::{action_bg, action_hover_bg, border, tab_selected_accent, tab_selected_bg};
use crate::app::ui::settings::dialog_card_frame;
use crate::app::ui::{callout, settings, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{
    ARROWS_LEFT_RIGHT, BACKSPACE, CARET_DOWN, CARET_RIGHT, CROSSHAIR, MAGNIFYING_GLASS,
    PENCIL_SIMPLE, STACK,
};

const TEXT_HISTORY_SIZE: egui::Vec2 =
    egui::vec2(crate::app::ui::search_replace::SEARCH_DIALOG_WIDTH, 520.0);
const TEXT_HISTORY_TITLE_SIZE: f32 = 24.0;
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

#[derive(Clone)]
struct TextHistoryRow {
    entry_id: u64,
    title: String,
    detail: String,
    icon: &'static str,
    undone: bool,
}

#[derive(Clone)]
struct TextHistoryFileGroup {
    buffer_id: BufferId,
    label: String,
    rows: Vec<TextHistoryRow>,
}

pub(crate) fn show_text_history_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.text_history_open {
        return;
    }

    let entries = app.cached_text_history_entries();
    let chronological = entries.iter().map(row_from_entry).collect::<Vec<_>>();
    let file_groups = file_groups_from_entries(entries.iter());

    let mut action: Option<u64> = None;
    let mut close_requested = false;
    let active_tab = read_active_tab(ctx);
    let mut next_tab = active_tab;
    let follow_focus = read_follow_focus(ctx);
    let mut next_follow_focus = follow_focus;

    show_centered_callout(ctx, "text_history_window", TEXT_HISTORY_SIZE, |ui| {
        render_text_history_window(
            ui,
            &chronological,
            &file_groups,
            active_tab,
            &mut next_tab,
            follow_focus,
            &mut next_follow_focus,
            &mut action,
            &mut close_requested,
        );
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
    if let Some(entry_id) = action {
        let _ = app.apply_text_history_to_entry(entry_id, next_follow_focus);
    }
}

fn row_from_entry(entry: &TextHistoryEntryView) -> TextHistoryRow {
    TextHistoryRow {
        entry_id: entry.id,
        title: entry.summary.clone(),
        detail: format!("{} · {}", entry.label, source_label(entry.source)),
        icon: entry_icon(entry),
        undone: entry.undone,
    }
}

fn file_groups_from_entries<'a>(
    entries: impl Iterator<Item = &'a TextHistoryEntryView>,
) -> Vec<TextHistoryFileGroup> {
    let mut groups = Vec::<TextHistoryFileGroup>::new();
    for entry in entries {
        let row = row_from_entry(entry);
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.buffer_id == entry.buffer_id)
        {
            group.rows.push(row);
        } else {
            groups.push(TextHistoryFileGroup {
                buffer_id: entry.buffer_id,
                label: entry.label.clone(),
                rows: vec![row],
            });
        }
    }
    groups
}

fn entry_icon(entry: &TextHistoryEntryView) -> &'static str {
    if entry.source == PieceSource::SearchReplace {
        return MAGNIFYING_GLASS;
    }
    if entry.edit_count != 1 {
        return STACK;
    }
    match (
        entry.first_deleted_text.is_empty(),
        entry.first_inserted_text.is_empty(),
    ) {
        (true, false) => PENCIL_SIMPLE,
        (false, true) => BACKSPACE,
        (false, false) => ARROWS_LEFT_RIGHT,
        (true, true) => PENCIL_SIMPLE,
    }
}

fn render_text_history_window(
    ui: &mut egui::Ui,
    chronological: &[TextHistoryRow],
    file_groups: &[TextHistoryFileGroup],
    active_tab: HistoryTab,
    next_tab: &mut HistoryTab,
    follow_focus: bool,
    next_follow_focus: &mut bool,
    action: &mut Option<u64>,
    close_requested: &mut bool,
) {
    settings::apply_dialog_typography(ui);
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);
    if render_header(ui, follow_focus, next_follow_focus) {
        *close_requested = true;
    }
    ui.add_space(4.0);
    history_card(ui, |ui| {
        render_tabs(ui, active_tab, next_tab);
    });

    match active_tab {
        HistoryTab::Timeline => render_timeline(ui, chronological, action),
        HistoryTab::ByFile => render_by_file(ui, file_groups, action),
    }
}

fn history_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(HISTORY_CARD_CORNER_RADIUS))
        .show(ui, add_contents)
        .inner
}

fn render_header(ui: &mut egui::Ui, follow_focus: bool, next_follow_focus: &mut bool) -> bool {
    callout::header_row(ui, "Close text history", |ui| {
        ui.label(
            egui::RichText::new("Text History")
                .size(TEXT_HISTORY_TITLE_SIZE)
                .color(callout::text(ui)),
        );
        ui.add_space(8.0);
        render_follow_focus_toggle(ui, follow_focus, next_follow_focus);
    })
}

fn render_follow_focus_toggle(ui: &mut egui::Ui, follow_focus: bool, next_follow_focus: &mut bool) {
    let (fill, hover_fill, tooltip) = if follow_focus {
        (
            tab_selected_bg(ui),
            action_hover_bg(ui),
            "Follow undo: focus jumps to the last reverted edit. Click to disable.",
        )
    } else {
        (
            action_bg(ui),
            action_hover_bg(ui),
            "Follow undo is off: focus stays put. Click to re-enable.",
        )
    };
    let response = widget_ids::scope(ui, "text_history.follow_focus_toggle", |ui| {
        phosphor_button(
            ui,
            CROSSHAIR,
            egui::vec2(28.0, 28.0),
            fill,
            hover_fill,
            tooltip,
        )
    })
    .inner;
    if response.clicked() {
        *next_follow_focus = !follow_focus;
    }
}

fn render_tabs(ui: &mut egui::Ui, active: HistoryTab, next: &mut HistoryTab) {
    ui.horizontal(|ui| {
        if tab_button(ui, "Timeline", active == HistoryTab::Timeline).clicked() {
            *next = HistoryTab::Timeline;
        }
        if tab_button(ui, "By file", active == HistoryTab::ByFile).clicked() {
            *next = HistoryTab::ByFile;
        }
    });
}

fn tab_button(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (fill, stroke_color, text_color) = if selected {
        (
            tab_selected_bg(ui),
            tab_selected_accent(ui),
            callout::text(ui),
        )
    } else {
        (action_bg(ui), border(ui), callout::muted_text(ui))
    };
    let button = egui::Button::new(egui::RichText::new(label).size(13.0).color(text_color))
        .min_size(egui::vec2(0.0, TAB_BUTTON_HEIGHT))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .corner_radius(egui::CornerRadius::same(8));
    ui.add(button)
}

fn render_timeline(ui: &mut egui::Ui, chronological: &[TextHistoryRow], action: &mut Option<u64>) {
    widget_ids::scope(ui, "text_history.section.timeline", |ui| {
        if chronological.is_empty() {
            ui.label(
                egui::RichText::new("No entries")
                    .size(13.0)
                    .color(callout::muted_text(ui)),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(widget_ids::local(ui, "text_history.scroll.timeline"))
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                render_timeline_rows(ui, chronological, action);
            });
    });
}

fn render_timeline_rows(
    ui: &mut egui::Ui,
    chronological: &[TextHistoryRow],
    action: &mut Option<u64>,
) {
    let now_after = newest_applied_index(chronological);
    let nothing_to_redo = chronological.iter().all(|row| !row.undone);
    let mut newest_first = chronological.iter().enumerate().collect::<Vec<_>>();
    newest_first.reverse();
    let mut now_rendered = false;

    for (idx, row) in newest_first {
        if !nothing_to_redo && !now_rendered && now_line_belongs_above(idx, now_after) {
            render_now_line(ui);
            now_rendered = true;
        }
        render_row(ui, row, action);
    }
    if !nothing_to_redo && !now_rendered {
        render_now_line(ui);
    }
}

fn newest_applied_index(rows: &[TextHistoryRow]) -> Option<usize> {
    rows.iter().rposition(|row| !row.undone)
}

fn now_line_belongs_above(current_idx: usize, newest_applied: Option<usize>) -> bool {
    match newest_applied {
        Some(idx) => current_idx == idx,
        None => false,
    }
}

fn render_by_file(ui: &mut egui::Ui, groups: &[TextHistoryFileGroup], action: &mut Option<u64>) {
    widget_ids::scope(ui, "text_history.section.by_file", |ui| {
        if groups.is_empty() {
            ui.label(
                egui::RichText::new("No file history")
                    .size(13.0)
                    .color(callout::muted_text(ui)),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(widget_ids::local(ui, "text_history.scroll.by_file"))
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                for (index, group) in groups.iter().enumerate() {
                    render_file_group(ui, group, action);
                    if index + 1 < groups.len() {
                        ui.add_space(12.0);
                    }
                }
            });
    });
}

fn render_file_group(ui: &mut egui::Ui, group: &TextHistoryFileGroup, action: &mut Option<u64>) {
    widget_ids::scope(ui, ("text_history.file_group", group.buffer_id), |ui| {
        let expansion_id =
            widget_ids::local(ui, ("text_history.file_group.expanded", group.buffer_id));
        let expanded = ui
            .data_mut(|data| data.get_persisted::<bool>(expansion_id))
            .unwrap_or(false);
        let (group_response, toggle_requested) = render_file_header_pill(ui, group, expanded);
        if group_response.clicked() || toggle_requested {
            ui.data_mut(|data| data.insert_persisted(expansion_id, !expanded));
        }
        if !expanded {
            return;
        }
        ui.add_space(HISTORY_PILL_SPACING);
        ui.indent(("text_history.file_group.indent", group.buffer_id), |ui| {
            render_file_history_rows(ui, &group.rows, action);
        });
    });
}

fn render_file_header_pill(
    ui: &mut egui::Ui,
    group: &TextHistoryFileGroup,
    expanded: bool,
) -> (egui::Response, bool) {
    let count_label = if group.rows.len() == 1 {
        "1 change".to_owned()
    } else {
        format!("{} changes", group.rows.len())
    };
    egui::Frame::NONE
        .fill(action_bg(ui))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(HISTORY_PILL_CORNER_RADIUS))
        .inner_margin(egui::Margin::same(HISTORY_PILL_INNER_MARGIN))
        .show(ui, |ui| {
            let mut toggle_requested = false;
            let mut group_response = None;

            ui.horizontal(|ui| {
                let caret = ui
                    .add_sized(
                        egui::vec2(24.0, 24.0),
                        egui::Button::new(
                            egui::RichText::new(if expanded { CARET_DOWN } else { CARET_RIGHT })
                                .size(14.0)
                                .color(callout::muted_text(ui)),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE),
                    )
                    .on_hover_text(if expanded {
                        "Collapse history for this file"
                    } else {
                        "Expand history for this file"
                    });
                if caret.clicked() {
                    toggle_requested = true;
                }

                let label_width = (ui.available_width() - 96.0).max(120.0);
                let response = ui.add_sized(
                    egui::vec2(label_width, 24.0),
                    egui::Button::new(
                        egui::RichText::new(&group.label)
                            .size(13.0)
                            .color(callout::text(ui)),
                    )
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE),
                );
                group_response = Some(response);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(count_label)
                            .size(12.0)
                            .color(callout::muted_text(ui)),
                    );
                });
            });

            (
                group_response.unwrap_or_else(|| ui.label("")),
                toggle_requested,
            )
        })
        .inner
}

fn render_file_history_rows(ui: &mut egui::Ui, rows: &[TextHistoryRow], action: &mut Option<u64>) {
    for row in rows {
        render_row(ui, row, action);
    }
}

fn render_row(ui: &mut egui::Ui, row: &TextHistoryRow, action: &mut Option<u64>) {
    let response = widget_ids::scope(ui, ("text_history.row", row.entry_id), |ui| {
        history_pill(ui, row)
    })
    .inner
    .on_hover_text(if row.undone {
        "Click to redo this text change"
    } else {
        "Click to undo this text change"
    });

    if response.clicked() {
        *action = Some(row.entry_id);
    }
    ui.add_space(HISTORY_PILL_SPACING);
}

fn history_pill(ui: &mut egui::Ui, row: &TextHistoryRow) -> egui::Response {
    let frame_id = ui.next_auto_id();
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
    let fill = if row.undone {
        base_fill.gamma_multiply(UNDONE_OPACITY)
    } else {
        base_fill
    };
    let stroke = if row.undone {
        border(ui).gamma_multiply(UNDONE_OPACITY)
    } else {
        border(ui)
    };
    let title_color = if row.undone {
        callout::text(ui).gamma_multiply(UNDONE_OPACITY)
    } else {
        callout::text(ui)
    };
    let muted_color =
        callout::muted_text(ui).gamma_multiply(if row.undone { UNDONE_OPACITY } else { 1.0 });

    let inner = egui::Frame::NONE
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(HISTORY_PILL_CORNER_RADIUS))
        .inner_margin(egui::Margin::same(HISTORY_PILL_INNER_MARGIN))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(row.icon)
                        .font(egui::FontId::proportional(HISTORY_PILL_ICON_SIZE))
                        .color(muted_color),
                );
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(&row.title)
                            .size(14.0)
                            .color(title_color),
                    );
                    ui.label(
                        egui::RichText::new(&row.detail)
                            .size(12.0)
                            .color(muted_color),
                    );
                });
            });
        });

    ui.interact(inner.response.rect, frame_id, egui::Sense::click())
}

fn render_now_line(ui: &mut egui::Ui) {
    let accent = tab_selected_accent(ui);
    let muted = callout::muted_text(ui);
    let label_font = egui::FontId::proportional(11.0);

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), NOW_LINE_HEIGHT),
        egui::Sense::hover(),
    );
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

fn read_active_tab(ctx: &egui::Context) -> HistoryTab {
    let id = widget_ids::global("text_history.active_tab");
    ctx.data_mut(|data| data.get_persisted::<u8>(id))
        .and_then(tab_from_persisted)
        .unwrap_or(HistoryTab::Timeline)
}

fn write_active_tab(ctx: &egui::Context, tab: HistoryTab) {
    let id = widget_ids::global("text_history.active_tab");
    ctx.data_mut(|data| data.insert_persisted(id, tab_to_persisted(tab)));
}

fn read_follow_focus(ctx: &egui::Context) -> bool {
    let id = widget_ids::global("text_history.follow_undo");
    ctx.data_mut(|data| data.get_persisted::<bool>(id))
        .unwrap_or(true)
}

fn write_follow_focus(ctx: &egui::Context, follow: bool) {
    let id = widget_ids::global("text_history.follow_undo");
    ctx.data_mut(|data| data.insert_persisted(id, follow));
}

fn tab_from_persisted(value: u8) -> Option<HistoryTab> {
    match value {
        0 => Some(HistoryTab::Timeline),
        1 => Some(HistoryTab::ByFile),
        _ => None,
    }
}

fn tab_to_persisted(tab: HistoryTab) -> u8 {
    match tab {
        HistoryTab::Timeline => 0,
        HistoryTab::ByFile => 1,
    }
}
