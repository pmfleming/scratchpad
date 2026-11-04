use super::super::{presentation, style};
use crate::app::{domain::BufferState, theme, ui::widget_ids};
use eframe::egui;
use egui_phosphor::regular as icons;

pub(super) fn header(ui: &mut egui::Ui, buffer: &BufferState) -> bool {
    let change = presentation::change(ui, buffer, buffer.document().history_now());
    let heading = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), 26.0));
    style::text(
        ui,
        egui::Rect::from_min_size(heading.min, egui::vec2(20.0, 26.0)),
        change.icon,
        14.0,
        change.color,
    );
    let title = egui::Rect::from_min_max(
        heading.min + egui::vec2(22.0, 0.0),
        egui::pos2(heading.right() - 32.0, heading.bottom()),
    );
    change.paint_title(ui, title, theme::text_primary(ui));
    widget_ids::interact(
        ui,
        title,
        egui::Id::unique("history.scrubber.heading"),
        egui::Sense::hover(),
        "history-summary",
    )
    .on_hover_text(&change.title);
    let button = ui.put(
        egui::Rect::from_min_size(
            egui::pos2(heading.right() - 26.0, heading.top()),
            egui::vec2(26.0, 26.0),
        ),
        egui::Button::new(
            egui::RichText::new(icons::CLOCK_COUNTER_CLOCKWISE).font(egui_phosphor::font_id(15.0)),
        )
        .frame(false),
    );
    button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Open in History"));
    button
        .on_hover_text(crate::app::shortcut_tooltips::action(
            ui.ctx(),
            crate::app::shortcut_keymap::ShortcutAction::OpenTextHistory,
            "Open in History",
        ))
        .clicked()
}

pub(super) fn caption(ui: &mut egui::Ui, buffer: &BufferState) {
    let document = buffer.document();
    let caption = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), 20.0));
    let label = format!(
        "Now · {} of {} changes",
        document.operation_undo_depth(),
        document.history_route().len()
    );
    let change = presentation::change(ui, buffer, document.history_now());
    let time = presentation::clock(change.edited_at);
    let display = format!(
        "Now {} of {}{}",
        document.operation_undo_depth(),
        document.history_route().len(),
        if time.is_empty() {
            String::new()
        } else {
            format!(" · {time}")
        }
    );
    let caption = caption.shrink2(egui::vec2(59.0, 0.0));
    let position_width = ui
        .painter()
        .layout_no_wrap(
            display.clone(),
            style::font(ui, 12.0),
            theme::text_muted(ui),
        )
        .size()
        .x
        .min(caption.width());
    let file_rect = egui::Rect::from_min_max(
        caption.min,
        egui::pos2(caption.right() - position_width - 8.0, caption.bottom()),
    );
    style::text(ui, file_rect, &buffer.name, 12.0, theme::text_muted(ui));
    widget_ids::interact(
        ui,
        file_rect,
        egui::Id::unique("history.scrubber.file"),
        egui::Sense::hover(),
        "history-file-label",
    )
    .on_hover_text(crate::app::text_history::buffer_label(buffer));
    let label_rect = egui::Rect::from_min_max(
        egui::pos2(caption.right() - position_width, caption.top()),
        caption.max,
    );
    style::text_right(ui, label_rect, &display, 12.0, theme::text_muted(ui));
    widget_ids::interact(
        ui,
        label_rect,
        egui::Id::unique("history.scrubber.caption"),
        egui::Sense::hover(),
        "history-position-label",
    )
    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
}

pub(super) fn transport(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    position: &mut usize,
) -> (egui::Response, bool) {
    let len = buffer.document().history_route().len();
    let mut stepped = false;
    let response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let track_width = (ui.available_width() - 104.0).max(20.0);
            for (key, icon, label, destination, allowed) in [
                (
                    "first",
                    icons::SKIP_BACK,
                    "Go to start (Home)",
                    0,
                    *position > 0,
                ),
                (
                    "previous",
                    icons::CARET_LEFT,
                    "Previous change (←)",
                    position.saturating_sub(1),
                    *position > 0,
                ),
            ] {
                if style::icon_button(ui, key, icon, label, allowed).clicked() {
                    *position = destination;
                    stepped = true;
                }
            }
            let response = super::track::show(ui, buffer, position, track_width);
            for (key, icon, label, destination, allowed) in [
                (
                    "next",
                    icons::CARET_RIGHT,
                    "Next change (→)",
                    (*position + 1).min(len),
                    *position < len,
                ),
                (
                    "last",
                    icons::SKIP_FORWARD,
                    "Go to end (End)",
                    len,
                    *position < len,
                ),
            ] {
                if style::icon_button(ui, key, icon, label, allowed).clicked() {
                    *position = destination;
                    stepped = true;
                }
            }
            response
        })
        .inner;
    (response, stepped)
}
