use super::model::shortcut_tooltip_for_menu_label;
use crate::app::theme::{action_bg, action_hover_bg, border, text_primary};
use crate::app::ui::widget_ids;
use eframe::egui;

pub(super) const EDITOR_CONTEXT_MENU_WIDTH: f32 = 204.0;
pub(super) const EDITOR_CONTEXT_SUBMENU_WIDTH: f32 = 168.0;
pub(super) const EDITOR_UNICODE_INSERT_SUBMENU_WIDTH: f32 = 380.0;
pub(super) const EDITOR_CONTEXT_ROW_HEIGHT: f32 = 28.0;
pub(super) const EDITOR_CONTEXT_CARET_WIDTH: f32 = 28.0;
pub(super) const EDITOR_UNICODE_LABEL_X: f32 = 20.0;
pub(super) const EDITOR_UNICODE_DIVIDER_X: f32 = 76.0;
pub(super) const EDITOR_UNICODE_DESCRIPTION_X: f32 = 94.0;

const EDITOR_CONTEXT_ICON_BUTTON_SIZE: egui::Vec2 = egui::vec2(38.0, 30.0);
const EDITOR_CONTEXT_ICON_CENTER_X: f32 = 20.0;
const EDITOR_CONTEXT_LABEL_X: f32 = 52.0;
const EDITOR_CONTEXT_RAIL_BUTTON_COUNT: f32 = 4.0;

pub(super) fn set_menu_width(ui: &mut egui::Ui, width: f32) {
    ui.set_min_width(width);
    ui.set_max_width(width);
}

pub(super) fn icon_rail_leading_space(available_width: f32, button_spacing: f32) -> f32 {
    let rail_width = EDITOR_CONTEXT_ICON_BUTTON_SIZE.x * EDITOR_CONTEXT_RAIL_BUTTON_COUNT
        + button_spacing * (EDITOR_CONTEXT_RAIL_BUTTON_COUNT - 1.0);
    ((available_width - rail_width) * 0.5).max(0.0)
}

pub(super) fn menu_action_button(
    ui: &mut egui::Ui,
    label: &str,
    icon: Option<&str>,
    enabled: bool,
) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let response = widget_ids::surface_response(
            ui,
            ("editor_context.menu_action", label),
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add_enabled(
                    enabled,
                    egui::Button::new("")
                        .min_size(egui::vec2(
                            EDITOR_CONTEXT_MENU_WIDTH,
                            EDITOR_CONTEXT_ROW_HEIGHT,
                        ))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        paint_context_menu_row_label(ui, response.rect, icon, label, enabled);
        let clicked = response.clicked();
        if let Some(tooltip) = shortcut_tooltip_for_menu_label(ui.ctx(), label) {
            response.on_hover_text(tooltip);
        }
        clicked
    })
}

pub(super) fn split_menu_button(ui: &mut egui::Ui, label: &str, icon: &str) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let response = widget_ids::surface_response(
            ui,
            ("editor_context.split_submenu", label),
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add(
                    egui::Button::new("")
                        .min_size(egui::vec2(
                            EDITOR_CONTEXT_SUBMENU_WIDTH,
                            EDITOR_CONTEXT_ROW_HEIGHT,
                        ))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        paint_context_menu_row_label(ui, response.rect, Some(icon), label, true);
        let clicked = response.clicked();
        if let Some(tooltip) = shortcut_tooltip_for_menu_label(ui.ctx(), label) {
            response.on_hover_text(tooltip);
        }
        clicked
    })
}

pub(super) fn paint_context_menu_row_label(
    ui: &egui::Ui,
    rect: egui::Rect,
    icon: Option<&str>,
    label: &str,
    enabled: bool,
) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let color = if enabled {
        text_primary(ui)
    } else {
        text_primary(ui).gamma_multiply(0.45)
    };
    if let Some(icon) = icon {
        ui.painter().text(
            rect.left_center() + egui::vec2(EDITOR_CONTEXT_ICON_CENTER_X, 0.0),
            egui::Align2::CENTER_CENTER,
            icon,
            egui_phosphor::font_id(font.size),
            color,
        );
    }
    ui.painter().text(
        rect.left_center() + egui::vec2(EDITOR_CONTEXT_LABEL_X, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
}

pub(super) fn apply_context_menu_row_hover_style(ui: &mut egui::Ui) {
    let hover_bg = action_hover_bg(ui);
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.hovered.bg_fill = hover_bg;
    visuals.widgets.hovered.weak_bg_fill = hover_bg;
    visuals.widgets.active.bg_fill = hover_bg;
    visuals.widgets.active.weak_bg_fill = hover_bg;
    visuals.widgets.open.bg_fill = hover_bg;
    visuals.widgets.open.weak_bg_fill = hover_bg;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.open.bg_stroke = egui::Stroke::NONE;
}

pub(super) fn icon_rail_button(
    ui: &mut egui::Ui,
    icon: &str,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    with_visual_overrides(ui, apply_icon_rail_button_style, |ui| {
        let color = if enabled {
            text_primary(ui)
        } else {
            text_primary(ui).gamma_multiply(0.45)
        };
        let button = egui::Button::new(
            egui::RichText::new(icon)
                .font(egui_phosphor::font_id(17.0))
                .color(color),
        )
        .min_size(EDITOR_CONTEXT_ICON_BUTTON_SIZE)
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(8));

        widget_ids::surface_response(
            ui,
            ("editor_context.icon_rail", tooltip),
            widget_ids::WidgetRole::IconButton,
            |ui| ui.add_enabled(enabled, button),
        )
        .on_hover_text(tooltip)
    })
}

fn apply_icon_rail_button_style(ui: &mut egui::Ui) {
    let idle_bg = action_bg(ui);
    let hover_bg = action_hover_bg(ui);
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.bg_fill = idle_bg;
    visuals.widgets.inactive.weak_bg_fill = idle_bg;
    visuals.widgets.hovered.bg_fill = hover_bg;
    visuals.widgets.hovered.weak_bg_fill = hover_bg;
    visuals.widgets.active.bg_fill = hover_bg;
    visuals.widgets.active.weak_bg_fill = hover_bg;
    visuals.widgets.open.bg_fill = hover_bg;
    visuals.widgets.open.weak_bg_fill = hover_bg;
}

pub(super) fn with_visual_overrides<R>(
    ui: &mut egui::Ui,
    configure: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let previous_visuals = ui.visuals().clone();
    configure(ui);
    let result = add_contents(ui);
    *ui.visuals_mut() = previous_visuals;
    result
}

#[cfg(test)]
mod tests {
    use super::icon_rail_leading_space;

    #[test]
    fn icon_rail_leading_space_centers_fixed_button_rail() {
        assert_eq!(icon_rail_leading_space(200.0, 4.0), 18.0);
    }

    #[test]
    fn icon_rail_leading_space_clamps_when_menu_is_narrow() {
        assert_eq!(icon_rail_leading_space(80.0, 4.0), 0.0);
    }
}
