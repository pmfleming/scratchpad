use super::{SettingsUi, egui};
use crate::app::ui::widget_ids;
use std::hash::Hash;

pub(super) fn toggle_switch(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    value: &mut bool,
) -> egui::Response {
    // Keep the compact track, with a full-height keyboard/pointer target.
    let desired_size = egui::vec2(42.0, SettingsUi::CONTROLS.height);
    let available_rect = ui.available_rect_before_wrap();
    let top = available_rect.center().y - desired_size.y * 0.5;
    let min = if ui.layout().main_dir() == egui::Direction::RightToLeft {
        egui::pos2(available_rect.right() - desired_size.x, top)
    } else {
        egui::pos2(available_rect.left(), top)
    };
    let rect = egui::Rect::from_min_size(min, desired_size);
    let mut response = widget_ids::interact(
        ui,
        rect,
        widget_ids::surface_role(surface_key, widget_ids::WidgetRole::ToggleSwitch),
        egui::Sense::click(),
        "settings_toggle_switch",
    );
    ui.advance_cursor_after_rect(rect);
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }

    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Switch, ui.is_enabled(), *value, "")
    });
    let how_on = ui.ctx().animate_bool(response.id, *value);
    let rect = egui::Rect::from_center_size(response.rect.center(), egui::vec2(42.0, 22.0));
    let radius = rect.height() * 0.5;
    let track_fill = if *value {
        SettingsUi::accent()
    } else {
        SettingsUi::control_bg(ui)
    };

    ui.painter().rect(
        rect,
        radius,
        track_fill,
        egui::Stroke::new(1.0, SettingsUi::card_border(ui)),
        egui::StrokeKind::Inside,
    );

    let thumb_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
    ui.painter().circle_filled(
        egui::pos2(thumb_x, rect.center().y),
        radius - 3.0,
        egui::Color32::WHITE,
    );

    if response.has_focus() && ui.is_enabled() {
        ui.painter().rect_stroke(
            rect.expand(3.0),
            radius + 3.0,
            egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }

    response
}

#[cfg(test)]
mod tests;
