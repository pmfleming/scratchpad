use crate::app::ui::{panel_style, visuals::with_visual_overrides, widget_ids};
use eframe::egui;
use std::hash::Hash;

/// Contents of a flat context-menu action row.
pub struct MenuRow<'a> {
    pub label: &'a str,
    pub icon: Option<&'a str>,
    /// Right-aligned shortcut keys or current value.
    pub trailing: Option<&'a str>,
    pub tooltip: Option<String>,
    pub enabled: bool,
}

/// Renders a full-width menu action row and reports whether it was clicked.
pub fn menu_row_button(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    size: egui::Vec2,
    row: MenuRow<'_>,
) -> bool {
    with_flat_row_visuals(ui, |ui| {
        let response = widget_ids::surface_response(
            ui,
            surface_key,
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add_enabled(
                    row.enabled,
                    egui::Button::new("")
                        .min_size(size)
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, row.enabled, row.label));
        panel_style::paint_menu_row(
            ui,
            response.rect,
            row.icon,
            row.label,
            row.trailing,
            row.enabled,
        );
        let clicked = response.clicked();
        if let Some(tooltip) = row.tooltip {
            response.on_hover_text(tooltip);
        }
        clicked
    })
}

/// Runs `add_contents` with transparent rows that highlight on hover.
pub fn with_flat_row_visuals<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    with_visual_overrides(ui, panel_style::flat_rows, add_contents)
}
