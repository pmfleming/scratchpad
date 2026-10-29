use super::widgets::{EDITOR_CONTEXT_MENU_WIDTH, EDITOR_CONTEXT_ROW_HEIGHT};
use crate::app::{
    app_state::{
        ScratchpadApp,
        workspace::zoom::{self, ZoomChange},
    },
    domain::{
        ViewId,
        view::{MAX_ZOOM_FACTOR, MIN_ZOOM_FACTOR},
    },
    shortcut_keymap::ShortcutAction,
    shortcut_tooltips,
    ui::widget_ids,
};
use eframe::egui;
use egui_phosphor::regular::{ARROW_COUNTER_CLOCKWISE, MINUS, PLUS};

pub(super) fn render(ui: &mut egui::Ui, app: &mut ScratchpadApp, view_id: ViewId) {
    let Some(view) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.layout.view(view_id))
    else {
        return;
    };
    let factor = view.zoom_factor();
    let label = format!("Zoom · {}", view.zoom_label());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let button_width = 28.0;
        ui.add_sized(
            egui::vec2(
                EDITOR_CONTEXT_MENU_WIDTH - 3.0 * (button_width + 4.0),
                EDITOR_CONTEXT_ROW_HEIGHT,
            ),
            egui::Label::new(label).halign(egui::Align::LEFT),
        );
        for (icon, label, shortcut, change, enabled) in [
            (
                MINUS,
                "Zoom out",
                ShortcutAction::DecreaseFontSize,
                ZoomChange::Out,
                factor > MIN_ZOOM_FACTOR,
            ),
            (
                PLUS,
                "Zoom in",
                ShortcutAction::IncreaseFontSize,
                ZoomChange::In,
                factor < MAX_ZOOM_FACTOR,
            ),
            (
                ARROW_COUNTER_CLOCKWISE,
                "Reset zoom",
                ShortcutAction::ResetZoom,
                ZoomChange::Reset,
                factor != 1.0,
            ),
        ] {
            let response = widget_ids::surface_response(
                ui,
                ("editor_context.zoom", view_id, label),
                widget_ids::WidgetRole::IconButton,
                |ui| {
                    ui.add_enabled(
                        enabled,
                        egui::Button::new(
                            egui::RichText::new(icon).font(egui_phosphor::font_id(16.0)),
                        )
                        .min_size(egui::vec2(button_width, EDITOR_CONTEXT_ROW_HEIGHT)),
                    )
                    .accessible_name(label)
                },
            );
            let clicked = response.clicked();
            let tooltip = shortcut_tooltips::action(ui.ctx(), shortcut, label);
            response
                .on_hover_text(&tooltip)
                .on_disabled_hover_text(tooltip);
            if clicked && zoom::apply(app, view_id, change) {
                // Intentionally keep the popup open for repeated adjustments.
                ui.ctx().request_repaint();
            }
        }
    });
}

#[cfg(test)]
mod tests;
