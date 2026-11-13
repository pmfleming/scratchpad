//! Flat, virtualized message rows on History's rail.
use super::Actions;
use super::model::{self, StatusRow};
use crate::app::app_state::StatusSeverity;
use crate::app::theme;
use crate::app::ui::{panel_style, widget_ids};
use eframe::egui;

pub(super) const ROW_HEIGHT: f32 = 44.0;
const RAIL_X: f32 = 11.0;
const DOT_Y: f32 = 12.5;
const ICON_X: f32 = 26.0;
const TITLE_X: f32 = 48.0;
const META_WIDTH: f32 = 84.0;
const TITLE_SIZE: f32 = 13.0;
const SUBTITLE_SIZE: f32 = 11.0;

#[derive(Clone, Copy, Default)]
struct Scroll {
    revealed: Option<u64>,
    offset: f32,
    viewport: f32,
}

fn scroll_key() -> egui::Id {
    egui::Id::unique("status_history.list.scroll")
}

pub(super) fn focus_id() -> egui::Id {
    egui::Id::unique("status_history.list.focus")
}

pub(super) struct ListView<'a> {
    pub(super) rows: &'a [StatusRow],
    pub(super) selected: Option<usize>,
    pub(super) new_from: u64,
    pub(super) empty: &'static str,
    pub(super) take_focus: bool,
}

pub(super) fn show(ui: &mut egui::Ui, view: &ListView<'_>, actions: &mut Actions) {
    let rows = view.rows;
    keyboard(ui, view, actions);
    let mut scroll = ui
        .ctx()
        .data(|d| d.get_temp::<Scroll>(scroll_key()))
        .unwrap_or_default();
    let mut area = egui::ScrollArea::vertical()
        .id_salt(widget_ids::scroll_id(ui, "status_history_list"))
        .auto_shrink([false, false]);
    let selected_id = view.selected.map(|index| rows[index].id);
    if let Some(index) = view.selected
        && scroll.revealed != selected_id
    {
        scroll.revealed = selected_id;
        let top = index as f32 * ROW_HEIGHT;
        if top < scroll.offset || top + ROW_HEIGHT > scroll.offset + scroll.viewport {
            area =
                area.vertical_scroll_offset((top - (scroll.viewport - ROW_HEIGHT) * 0.5).max(0.0));
        }
    }
    let output = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            area.show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
                for index in range {
                    let row = &rows[index];
                    let response = show_row(ui, row, view.selected == Some(index), view.new_from);
                    if response.clicked() {
                        actions.select = Some(row.id);
                        ui.memory_mut(|m| m.request_focus(focus_id()));
                    }
                }
            })
        })
        .inner;
    scroll.offset = output.state.clamped_offset().y;
    scroll.viewport = output.inner_rect.height();
    ui.ctx().data_mut(|d| d.insert_temp(scroll_key(), scroll));
    if rows.is_empty() {
        ui.painter().text(
            output.inner_rect.center_top() + egui::vec2(0.0, output.inner_rect.height() * 0.3),
            egui::Align2::CENTER_CENTER,
            view.empty,
            panel_style::font(ui, TITLE_SIZE),
            theme::text_muted(ui),
        );
    }
    // Focus-only, so rows keep their clicks; like History's list focus.
    let focus = widget_ids::interact(
        ui,
        output.inner_rect,
        focus_id(),
        egui::Sense::focusable_noninteractive(),
        "status-history-focus",
    );
    focus.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::ListBox,
            true,
            if rows.is_empty() {
                view.empty
            } else {
                "Status messages"
            },
        )
    });
    if view.take_focus {
        focus.request_focus();
    }
    if focus.has_focus() {
        panel_style::lock_navigation_focus(ui.ctx(), focus_id());
    }
}

/// ↑ ↓ move, Home/End jump and Ctrl+C copies the selected message.
fn keyboard(ui: &egui::Ui, view: &ListView<'_>, actions: &mut Actions) {
    let rows = view.rows;
    if rows.is_empty() || !ui.memory(|m| m.has_focus(focus_id())) {
        return;
    }
    let consume = |modifiers, key| ui.input_mut(|i| i.consume_key(modifiers, key));
    let last = rows.len() - 1;
    let mut target = None;
    if consume(egui::Modifiers::NONE, egui::Key::ArrowDown) {
        target = Some(view.selected.map_or(0, |i| (i + 1).min(last)));
    }
    if consume(egui::Modifiers::NONE, egui::Key::ArrowUp) {
        target = Some(view.selected.map_or(0, |i| i.saturating_sub(1)));
    }
    if consume(egui::Modifiers::NONE, egui::Key::Home) {
        target = Some(0);
    }
    if consume(egui::Modifiers::NONE, egui::Key::End) {
        target = Some(last);
    }
    if let Some(index) = target {
        actions.select = Some(rows[index].id);
    }
    // Platforms turn the copy shortcut into a Copy event rather than a key press.
    actions.copy |= consume(egui::Modifiers::COMMAND, egui::Key::C)
        || ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
}

fn show_row(ui: &mut egui::Ui, row: &StatusRow, selected: bool, new_from: u64) -> egui::Response {
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), ROW_HEIGHT));
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique(("status_history.row", row.id)),
        egui::Sense::click(),
        "status-history-row",
    );
    let new = row.id >= new_from;
    let label = format!("{}: {}", model::severity_name(row.severity), row.text);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, selected, &label)
    });
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_description(format!(
            "{} · {}{}",
            row.subtitle(),
            model::full_time(row.time),
            if new { " · New" } else { "" }
        ));
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            0,
            if selected {
                theme::tab_selected_bg(ui)
            } else {
                theme::action_bg(ui)
            },
        );
    }
    let dot = if selected {
        panel_style::RailDot::Current
    } else if new {
        panel_style::RailDot::Filled
    } else {
        panel_style::RailDot::Hollow
    };
    panel_style::rail(ui, rect, rect.left() + RAIL_X, rect.top() + DOT_Y, dot);
    panel_style::text(
        ui,
        egui::Rect::from_min_size(rect.min + egui::vec2(ICON_X, 0.0), egui::vec2(20.0, 25.0)),
        severity_icon(row.severity),
        14.0,
        severity_color(ui, row.severity),
    );
    let meta_left = rect.right() - META_WIDTH;
    let mut meta_right = rect.right() - 8.0;
    if new {
        let width = panel_style::badge_width(ui, "NEW");
        let tag = egui::Rect::from_center_size(
            egui::pos2(meta_right - width * 0.5, rect.top() + 12.5),
            egui::vec2(width, 15.0),
        );
        panel_style::badge(ui, tag, "NEW");
        meta_right = tag.left() - 6.0;
    }
    panel_style::text_right(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(meta_left, rect.top()),
            egui::pos2(meta_right, rect.top() + 25.0),
        ),
        &model::clock(row.time),
        SUBTITLE_SIZE,
        theme::text_muted(ui),
    );
    panel_style::text(
        ui,
        egui::Rect::from_min_max(
            rect.min + egui::vec2(TITLE_X, 2.0),
            egui::pos2(meta_left - 6.0, rect.top() + 23.0),
        ),
        &row.text,
        TITLE_SIZE,
        theme::text_primary(ui),
    );
    panel_style::text(
        ui,
        egui::Rect::from_min_max(
            rect.min + egui::vec2(TITLE_X, 25.0),
            egui::pos2(rect.right() - 8.0, rect.bottom() - 4.0),
        ),
        &row.subtitle(),
        SUBTITLE_SIZE,
        theme::text_muted(ui),
    );
    response.on_hover_text(format!("{}\n{}", row.text, model::full_time(row.time)))
}

pub(super) fn severity_icon(severity: StatusSeverity) -> &'static str {
    use egui_phosphor::regular as icons;
    match severity {
        StatusSeverity::Info => icons::INFO,
        StatusSeverity::Warning => icons::WARNING,
        StatusSeverity::Error => icons::WARNING_OCTAGON,
    }
}

pub(super) fn severity_color(ui: &egui::Ui, severity: StatusSeverity) -> egui::Color32 {
    match severity {
        StatusSeverity::Info => theme::text_muted(ui),
        StatusSeverity::Warning => ui.visuals().warn_fg_color,
        StatusSeverity::Error => theme::diff_del(ui),
    }
}
