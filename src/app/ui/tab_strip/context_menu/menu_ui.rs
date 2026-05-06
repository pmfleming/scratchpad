use crate::app::services::settings_store::{TabListPosition, TabOrderMode};
use crate::app::theme::{action_hover_bg, text_primary};
use crate::app::ui::widget_ids;
use eframe::egui;
use egui_phosphor::regular::{ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_UP, CARET_RIGHT};
use std::hash::Hash;

pub(super) const WIDTH: f32 = 220.0;
pub(super) const SUBMENU_WIDTH: f32 = 176.0;
pub(super) const ROW_HEIGHT: f32 = 28.0;
pub(super) const CARET_WIDTH: f32 = 28.0;

const ICON_CENTER_X: f32 = 20.0;
const LABEL_X: f32 = 52.0;

pub(super) fn menu_button(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    icon: Option<&str>,
    enabled: bool,
) -> bool {
    with_row_visuals(ui, |ui| {
        let response = widget_ids::surface_response(
            ui,
            ("tab_context.menu_button", label),
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add_enabled(
                    enabled,
                    egui::Button::new("")
                        .min_size(egui::vec2(width, ROW_HEIGHT))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        paint_row_label(ui, response.rect, icon, label, enabled);
        response.clicked()
    })
}

pub(super) fn primary_menu_button(
    ui: &mut egui::Ui,
    id_source: impl Hash,
    label: &str,
    icon: &str,
) -> bool {
    with_row_visuals(ui, |ui| {
        let response = widget_ids::surface_response(
            ui,
            id_source,
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add(
                    egui::Button::new("")
                        .min_size(egui::vec2(WIDTH - CARET_WIDTH, ROW_HEIGHT))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        paint_row_label(ui, response.rect, Some(icon), label, true);
        response.clicked()
    })
}

pub(super) fn submenu_button(
    ui: &mut egui::Ui,
    id_source: impl Hash,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    with_row_visuals(ui, |ui| {
        let button = egui::Button::new(egui::RichText::new(CARET_RIGHT).color(text_primary(ui)))
            .min_size(egui::vec2(CARET_WIDTH, ROW_HEIGHT))
            .stroke(egui::Stroke::NONE);

        widget_ids::surface_widget(ui, id_source, "submenu", |ui| {
            egui::containers::menu::SubMenuButton::from_button(button).ui(ui, |ui| {
                ui.set_min_width(SUBMENU_WIDTH);
                ui.set_max_width(SUBMENU_WIDTH);
                add_contents(ui);
            });
        });
    });
}

pub(super) fn close_direction_label(position: TabListPosition) -> &'static str {
    if position.is_vertical() {
        "Close Down"
    } else {
        "Close Right"
    }
}

pub(super) fn close_direction_icon(position: TabListPosition) -> &'static str {
    if position.is_vertical() {
        ARROW_DOWN
    } else {
        ARROW_RIGHT
    }
}

pub(super) fn tab_order_mode_label(mode: TabOrderMode) -> &'static str {
    match mode {
        TabOrderMode::Custom => "Custom Order",
        TabOrderMode::FileName => "File Name",
        TabOrderMode::FileAge => "File Age",
        TabOrderMode::RecentEdit => "Recent Edit",
    }
}

pub(super) fn tab_list_position_label(position: TabListPosition) -> &'static str {
    match position {
        TabListPosition::Top => "Top",
        TabListPosition::Bottom => "Bottom",
        TabListPosition::Left => "Left",
        TabListPosition::Right => "Right",
    }
}

pub(super) fn tab_list_position_icon(position: TabListPosition) -> &'static str {
    match position {
        TabListPosition::Top => ARROW_UP,
        TabListPosition::Bottom => ARROW_DOWN,
        TabListPosition::Left => ARROW_LEFT,
        TabListPosition::Right => ARROW_RIGHT,
    }
}

fn paint_row_label(
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
            rect.left_center() + egui::vec2(ICON_CENTER_X, 0.0),
            egui::Align2::CENTER_CENTER,
            icon,
            font.clone(),
            color,
        );
    }
    ui.painter().text(
        rect.left_center() + egui::vec2(LABEL_X, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
}

fn apply_row_hover_style(ui: &mut egui::Ui) {
    let hover_bg = action_hover_bg(ui);
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    for widgets in [
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widgets.bg_fill = hover_bg;
        widgets.weak_bg_fill = hover_bg;
        widgets.bg_stroke = egui::Stroke::NONE;
    }
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
}

fn with_row_visuals<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let previous_visuals = ui.visuals().clone();
    apply_row_hover_style(ui);
    let result = add_contents(ui);
    *ui.visuals_mut() = previous_visuals;
    result
}
