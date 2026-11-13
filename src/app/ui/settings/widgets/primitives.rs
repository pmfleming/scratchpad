use super::{SettingsUi, egui};
use crate::app::ui::widget_ids;
use std::hash::Hash;

pub(super) fn toggle_switch(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    value: &mut bool,
) -> egui::Response {
    // Keep the compact track, with a full-height keyboard/pointer target.
    let desired_size = egui::vec2(42.0, SettingsUi::control_height(ui));
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
        SettingsUi::accent(ui)
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
        if *value {
            crate::app::color_contrast::optimal_text_color(track_fill)
        } else {
            super::text_primary(ui)
        },
    );

    if response.has_focus() && ui.is_enabled() {
        ui.painter().rect_stroke(
            rect.expand(3.0),
            radius + 3.0,
            egui::Stroke::new(2.0, SettingsUi::accent(ui)),
            egui::StrokeKind::Inside,
        );
    }

    response
}

/// Style only the action, not the entire row or subsequent ordinary buttons.
pub(in crate::app::ui::settings) fn destructive_button(
    ui: &mut egui::Ui,
    text: &str,
) -> egui::Response {
    let [foreground, border, hover] = SettingsUi::destructive_colors(ui.visuals().dark_mode);
    let field = SettingsUi::field_bg(ui);
    let focus = SettingsUi::accent(ui);
    widget_ids::scope(ui, ("settings.destructive", text), |ui| {
        let visuals = ui.visuals_mut();
        visuals.override_text_color = Some(foreground);
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
        ] {
            widget.fg_stroke.color = foreground;
            widget.bg_stroke = egui::Stroke::new(1.0, border);
            widget.bg_fill = hover;
            widget.weak_bg_fill = hover;
            widget.corner_radius = egui::CornerRadius::same(7);
        }
        visuals.widgets.inactive.bg_fill = field;
        visuals.widgets.inactive.weak_bg_fill = field;
        // Keep keyboard focus distinct from the resting destructive border.
        visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, focus);
        ui.add(egui::Button::new(text).min_size(egui::vec2(0.0, SettingsUi::control_height(ui))))
    })
    .inner
}

#[cfg(test)]
mod tests;

pub(in crate::app::ui::settings) fn color_button(
    ui: &mut egui::Ui,
    label: &str,
    color: &mut egui::Color32,
) -> egui::Response {
    let mut response = widget_ids::surface_response(ui, ("settings.color", label), "color", |ui| {
        ui.add(egui::Button::new("").min_size(egui::vec2(48.0, SettingsUi::control_height(ui))))
    });
    egui::color_picker::show_color_at(ui.painter(), *color, response.rect.shrink(4.0));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            ui.is_enabled(),
            format!("Edit {label} color"),
        )
    });
    let base = (**ui.style()).clone();
    let fill = ui.visuals().panel_fill;
    let flat = crate::app::ui::panel_style::menu_style(fill);
    egui::Popup::menu(&response)
        .style(egui::style::StyleModifier::new(move |style| {
            *style = base.clone();
            flat.apply(style);
        }))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let viewport = ui.ctx().content_rect().size() - egui::vec2(48.0, 48.0);
            egui::ScrollArea::both()
                .max_width(viewport.x.max(100.0))
                .max_height(viewport.y.max(100.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(label).size(14.0).strong());
                    if hex_field(ui, label, color) {
                        response.mark_changed();
                    }
                    ui.spacing_mut().slider_width = 275.0;
                    if egui::color_picker::color_picker_color32(
                        ui,
                        color,
                        egui::color_picker::Alpha::Opaque,
                    ) {
                        response.mark_changed();
                    }
                    ui.label(egui::RichText::new("Esc Close").size(11.0).weak());
                });
        });
    response
}

/// A `#RRGGBB` field that accepts typing and paste; returns true when it set `color`.
fn hex_field(ui: &mut egui::Ui, label: &str, color: &mut egui::Color32) -> bool {
    let id = egui::Id::unique(("settings.color.hex", label));
    let current = format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b());
    let editing = ui.memory(|m| m.has_focus(id));
    let mut text = if editing {
        ui.data(|d| d.get_temp::<String>(id)).unwrap_or(current)
    } else {
        current
    };
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .font(egui::TextStyle::Monospace)
            .desired_width(96.0),
    );
    response.widget_info(|| egui::WidgetInfo::text_edit(true, "", &text, format!("{label} hex")));
    ui.data_mut(|d| d.insert_temp(id, text.clone()));
    match parse_hex(&text) {
        Some(parsed) if response.changed() && parsed != *color => {
            *color = parsed;
            true
        }
        _ => false,
    }
}

/// Reads `#RGB`, `#RRGGBB`, or the same without `#`.
pub(crate) fn parse_hex(text: &str) -> Option<egui::Color32> {
    let digits = text.trim().trim_start_matches('#');
    let channel = |s: &str| u8::from_str_radix(s, 16).ok();
    match digits.len() {
        6 => Some(egui::Color32::from_rgb(
            channel(&digits[0..2])?,
            channel(&digits[2..4])?,
            channel(&digits[4..6])?,
        )),
        3 => {
            let short = |i: usize| channel(&digits[i..=i]).map(|v| v * 17);
            Some(egui::Color32::from_rgb(short(0)?, short(1)?, short(2)?))
        }
        _ => None,
    }
}

#[cfg(test)]
mod hex_tests {
    use super::parse_hex;
    use eframe::egui::Color32;

    #[test]
    fn hex_accepts_long_short_and_bare_forms() {
        assert_eq!(
            parse_hex("#1D2126"),
            Some(Color32::from_rgb(0x1D, 0x21, 0x26))
        );
        assert_eq!(parse_hex("fff"), Some(Color32::WHITE));
        assert_eq!(
            parse_hex(" #abc "),
            Some(Color32::from_rgb(0xAA, 0xBB, 0xCC))
        );
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("#GG0000"), None);
    }
}
