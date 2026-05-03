use super::common::show_centered_callout;
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{BufferId, PieceSource, source_label};
use crate::app::text_history::TextHistoryEntryView;
use crate::app::theme::{action_bg, action_hover_bg, border, tab_selected_accent, tab_selected_bg};
use crate::app::ui::settings::dialog_card_frame;
use crate::app::ui::{callout, settings, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{
    ARROWS_LEFT_RIGHT, BACKSPACE, CARET_DOWN, CARET_RIGHT, CLIPBOARD, CLOCK_COUNTER_CLOCKWISE,
    CROSSHAIR, FILES, MAGNIFYING_GLASS, PENCIL_SIMPLE, SCISSORS, STACK, TRASH,
};
use std::collections::{HashMap, HashSet};

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

#[derive(Clone)]
struct TextHistoryRow {
    buffer_id: BufferId,
    entry_id: u64,
    title: String,
    detail: String,
    icon: &'static str,
    undone: bool,
}

#[derive(Clone, Copy)]
struct TextHistoryAction {
    buffer_id: BufferId,
    entry_id: u64,
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

    let mut action: Option<TextHistoryAction> = None;
    let mut close_requested = false;
    let mut clear_requested = false;
    let active_tab = read_active_tab(ctx);
    let mut next_tab = active_tab;
    let follow_focus = read_follow_focus(ctx);
    let mut next_follow_focus = follow_focus;

    show_centered_callout(ctx, "text_history_window", TEXT_HISTORY_SIZE, |ui| {
        ui.set_width(TEXT_HISTORY_SIZE.x);
        ui.set_min_width(TEXT_HISTORY_SIZE.x);
        ui.set_max_width(TEXT_HISTORY_SIZE.x);
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
            &mut clear_requested,
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
    if clear_requested {
        let _ = app.clear_text_history();
    }
    if let Some(action) = action {
        let _ =
            app.apply_text_history_to_entry(action.buffer_id, action.entry_id, next_follow_focus);
    }
}

fn row_from_entry(entry: &TextHistoryEntryView) -> TextHistoryRow {
    TextHistoryRow {
        buffer_id: entry.buffer_id,
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
    if entry.source == PieceSource::Paste {
        return CLIPBOARD;
    }
    if entry.source == PieceSource::Cut {
        return SCISSORS;
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
    action: &mut Option<TextHistoryAction>,
    close_requested: &mut bool,
    clear_requested: &mut bool,
) {
    settings::apply_dialog_typography(ui);
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);
    if render_header(ui) {
        *close_requested = true;
    }
    ui.add_space(4.0);
    history_card(ui, |ui| {
        render_controls(
            ui,
            active_tab,
            next_tab,
            follow_focus,
            next_follow_focus,
            !chronological.is_empty(),
            clear_requested,
        );
    });

    match active_tab {
        HistoryTab::Timeline => render_timeline(ui, chronological, action),
        HistoryTab::ByFile => render_by_file(ui, file_groups, action),
    }
}

fn history_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let width = ui.available_width();
    dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(HISTORY_CARD_CORNER_RADIUS))
        .show(ui, |ui| {
            ui.set_min_width(width - 24.0);
            add_contents(ui)
        })
        .inner
}

fn render_header(ui: &mut egui::Ui) -> bool {
    callout::header_row(ui, "Close history", |ui| {
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
    ui.horizontal(|ui| {
        if control_icon_button(
            ui,
            CLOCK_COUNTER_CLOCKWISE,
            "Timeline",
            active == HistoryTab::Timeline,
            true,
        )
        .clicked()
        {
            *next = HistoryTab::Timeline;
        }
        if control_icon_button(ui, FILES, "By file", active == HistoryTab::ByFile, true).clicked() {
            *next = HistoryTab::ByFile;
        }
        if control_icon_button(
            ui,
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
            if control_icon_button(ui, TRASH, "Clear all text history", false, has_history)
                .clicked()
            {
                *clear_requested = true;
            }
        });
    });
}

fn control_icon_button(
    ui: &mut egui::Ui,
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
    ui.add_enabled(enabled, button).on_hover_text(tooltip)
}

fn render_timeline(
    ui: &mut egui::Ui,
    chronological: &[TextHistoryRow],
    action: &mut Option<TextHistoryAction>,
) {
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
            .min_scrolled_height(TEXT_HISTORY_LIST_MIN_HEIGHT)
            .show(ui, |ui| {
                render_timeline_rows(ui, chronological, action);
            });
    });
}

fn render_timeline_rows(
    ui: &mut egui::Ui,
    chronological: &[TextHistoryRow],
    action: &mut Option<TextHistoryAction>,
) {
    let anchors = timeline_now_line_anchors(chronological);

    for row in chronological.iter().rev() {
        if anchors.contains(&(row.buffer_id, row.entry_id)) {
            render_now_line(ui);
        }
        render_row(ui, row, action);
    }
}

/// Each buffer with a mixed applied/undone state contributes one Now-line in
/// the global Timeline, anchored to its newest applied entry. Rendered in
/// newest-first order, the Now-line therefore appears just above that row,
/// separating the buffer's redo targets (newer, above) from its applied
/// entries (older, below).
fn timeline_now_line_anchors(rows: &[TextHistoryRow]) -> HashSet<(BufferId, u64)> {
    let mut newest_applied: HashMap<BufferId, u64> = HashMap::new();
    let mut has_undone: HashSet<BufferId> = HashSet::new();
    for row in rows {
        if row.undone {
            has_undone.insert(row.buffer_id);
        } else {
            // rows arrive in chronological order, so the last write per buffer
            // is the newest applied entry for that buffer.
            newest_applied.insert(row.buffer_id, row.entry_id);
        }
    }
    newest_applied
        .into_iter()
        .filter(|(buffer_id, _)| has_undone.contains(buffer_id))
        .collect()
}

fn newest_applied_index(rows: &[TextHistoryRow]) -> Option<usize> {
    rows.iter().rposition(|row| !row.undone)
}

fn render_by_file(
    ui: &mut egui::Ui,
    groups: &[TextHistoryFileGroup],
    action: &mut Option<TextHistoryAction>,
) {
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
            .min_scrolled_height(TEXT_HISTORY_LIST_MIN_HEIGHT)
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
            ui.set_min_width(ui.available_width());
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

fn per_file_now_line_insert_index(rows: &[TextHistoryRow]) -> Option<usize> {
    let insert_index = newest_applied_index(rows)?.saturating_add(1);
    (insert_index < rows.len()).then_some(insert_index)
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
            ui.set_min_width(ui.available_width());
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

    widget_ids::interact(
        ui,
        inner.response.rect,
        frame_id,
        egui::Sense::click(),
        "text_history.row.pill",
    )
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

#[cfg(test)]
mod tests {
    use super::{
        TextHistoryFileGroup, TextHistoryRow, file_groups_from_entries, newest_applied_index,
        per_file_now_line_insert_index, read_follow_focus, timeline_now_line_anchors,
        write_follow_focus,
    };
    use crate::app::domain::BufferId;
    use crate::app::domain::PieceSource;
    use crate::app::text_history::TextHistoryEntryView;

    #[test]
    fn follow_focus_defaults_to_enabled() {
        let ctx = eframe::egui::Context::default();

        assert!(read_follow_focus(&ctx));
    }

    #[test]
    fn follow_focus_persistence_round_trips() {
        let ctx = eframe::egui::Context::default();

        write_follow_focus(&ctx, false);
        assert!(!read_follow_focus(&ctx));

        write_follow_focus(&ctx, true);
        assert!(read_follow_focus(&ctx));
    }

    #[test]
    fn per_file_now_line_sits_between_applied_and_undone_rows() {
        let rows = vec![row(false), row(false), row(true), row(true)];

        let newest_applied = newest_applied_index(&rows);

        assert_eq!(newest_applied, Some(1));
        assert_eq!(per_file_now_line_insert_index(&rows), Some(2));
    }

    #[test]
    fn per_file_now_line_is_suppressed_when_it_would_sit_at_top() {
        let rows = vec![row(true), row(true)];

        let newest_applied = newest_applied_index(&rows);

        assert_eq!(newest_applied, None);
        assert_eq!(per_file_now_line_insert_index(&rows), None);
    }

    #[test]
    fn per_file_now_line_is_suppressed_when_everything_is_applied() {
        let rows = vec![row(false), row(false)];

        assert_eq!(newest_applied_index(&rows), Some(1));
        assert_eq!(per_file_now_line_insert_index(&rows), None);
    }

    #[test]
    fn per_file_now_line_is_absent_for_empty_file_history() {
        assert_eq!(per_file_now_line_insert_index(&[]), None);
    }

    #[test]
    fn per_file_groups_keep_rows_in_chronological_order() {
        let entries = vec![
            entry(1, 10, 1, "one"),
            entry(2, 11, 2, "other"),
            entry(3, 12, 1, "two"),
        ];

        let groups = file_groups_from_entries(entries.iter());

        assert_eq!(group_entry_ids(&groups, 1), vec![1, 3]);
    }

    #[test]
    fn per_file_rows_preserve_buffer_id_when_entry_ids_collide() {
        let entries = vec![entry(1, 10, 1, "one"), entry(1, 11, 2, "other")];

        let groups = file_groups_from_entries(entries.iter());

        assert_eq!(group_targets(&groups, 1), vec![(1, 1)]);
        assert_eq!(group_targets(&groups, 2), vec![(2, 1)]);
    }

    fn row(undone: bool) -> TextHistoryRow {
        TextHistoryRow {
            buffer_id: 0,
            entry_id: 0,
            title: String::new(),
            detail: String::new(),
            icon: "",
            undone,
        }
    }

    fn timeline_row(buffer_id: BufferId, entry_id: u64, undone: bool) -> TextHistoryRow {
        TextHistoryRow {
            buffer_id,
            entry_id,
            title: String::new(),
            detail: String::new(),
            icon: "",
            undone,
        }
    }

    #[test]
    fn timeline_now_line_anchors_one_per_buffer_in_mixed_state() {
        // Buffer 1: entries 1 (applied), 2 (applied), 3 (undone)
        // Buffer 2: entries 4 (applied), 5 (applied) — all applied, no anchor
        // Buffer 3: entries 6 (undone), 7 (undone) — all undone, no anchor
        let rows = vec![
            timeline_row(1, 1, false),
            timeline_row(1, 2, false),
            timeline_row(1, 3, true),
            timeline_row(2, 4, false),
            timeline_row(2, 5, false),
            timeline_row(3, 6, true),
            timeline_row(3, 7, true),
        ];

        let anchors = timeline_now_line_anchors(&rows);

        assert_eq!(anchors.len(), 1);
        assert!(anchors.contains(&(1, 2)));
    }

    #[test]
    fn timeline_now_line_anchors_multiple_buffers() {
        // Two buffers, both in mixed state — expect one anchor each.
        let rows = vec![
            timeline_row(1, 1, false),
            timeline_row(1, 2, true),
            timeline_row(2, 3, false),
            timeline_row(2, 4, false),
            timeline_row(2, 5, true),
        ];

        let anchors = timeline_now_line_anchors(&rows);

        assert_eq!(anchors.len(), 2);
        assert!(anchors.contains(&(1, 1)));
        assert!(anchors.contains(&(2, 4)));
    }

    #[test]
    fn timeline_now_line_anchors_empty_when_no_history() {
        let rows: Vec<TextHistoryRow> = Vec::new();

        assert!(timeline_now_line_anchors(&rows).is_empty());
    }

    fn entry(id: u64, seq: u64, buffer_id: u64, summary: &str) -> TextHistoryEntryView {
        TextHistoryEntryView {
            id,
            seq,
            buffer_id,
            label: format!("file-{buffer_id}"),
            source: PieceSource::Edit,
            summary: summary.to_owned(),
            undone: false,
            replayable: true,
            edit_count: 1,
            first_deleted_text: String::new(),
            first_inserted_text: summary.to_owned(),
        }
    }

    fn group_entry_ids(groups: &[TextHistoryFileGroup], buffer_id: u64) -> Vec<u64> {
        groups
            .iter()
            .find(|group| group.buffer_id == buffer_id)
            .map(|group| group.rows.iter().map(|row| row.entry_id).collect())
            .unwrap_or_default()
    }

    fn group_targets(groups: &[TextHistoryFileGroup], buffer_id: u64) -> Vec<(u64, u64)> {
        groups
            .iter()
            .find(|group| group.buffer_id == buffer_id)
            .map(|group| {
                group
                    .rows
                    .iter()
                    .map(|row| (row.buffer_id, row.entry_id))
                    .collect()
            })
            .unwrap_or_default()
    }
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
