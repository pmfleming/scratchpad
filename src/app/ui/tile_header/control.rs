use crate::app::theme::{action_hover_bg, text_primary};
use crate::app::ui::transition;
use crate::app::ui::widget_ids;
use eframe::egui;

pub struct TileControl<'a> {
    label: &'a str,
    visibility: f32,
    font_size: f32,
    tooltip: Option<&'a str>,
}

impl<'a> TileControl<'a> {
    #[must_use]
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            visibility: 1.0,
            font_size: 14.0,
            tooltip: None,
        }
    }

    #[must_use]
    pub fn visibility(mut self, visibility: f32) -> Self {
        self.visibility = visibility;
        self
    }

    #[must_use]
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    #[must_use]
    pub fn tooltip(mut self, tooltip: &'a str) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub fn show(self, ui: &mut egui::Ui, rect: egui::Rect, sense: egui::Sense) -> egui::Response {
        let response = widget_ids::interact(
            ui,
            rect,
            widget_ids::rect_surface_id(rect, "tile_header_control"),
            sense,
            "tile_header_control",
        );
        let drag_in_progress = transition::suppress_interactive_chrome(ui.ctx());

        if self.visibility > 0.0 {
            paint_tile_control(
                ui,
                rect,
                self.label,
                !drag_in_progress && (response.hovered() || response.dragged()),
                self.visibility,
                self.font_size,
            );
            if response.has_focus() {
                ui.painter().rect_stroke(
                    rect,
                    egui::CornerRadius::ZERO,
                    ui.visuals().selection.stroke,
                    egui::StrokeKind::Inside,
                );
            }

            if let Some(tooltip) = self.tooltip {
                return response.on_hover_text(tooltip);
            }
        }

        response
    }
}

pub(crate) fn paint_tile_control(
    ui: &egui::Ui,
    rect: egui::Rect,
    label: &str,
    hovered: bool,
    visibility: f32,
    font_size: f32,
) {
    if visibility <= 0.0 {
        return;
    }

    let style_colors = tile_control_colors(ui, hovered, visibility);
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::ZERO, style_colors.fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui_phosphor::font_id(font_size),
        style_colors.text_color,
    );
}

struct TileControlColors {
    fill: egui::Color32,
    text_color: egui::Color32,
}

fn tile_control_colors(ui: &egui::Ui, hovered: bool, visibility: f32) -> TileControlColors {
    let fill = if hovered {
        action_hover_bg(ui)
    } else {
        egui::Color32::TRANSPARENT
    };
    let text_color = text_primary(ui);
    TileControlColors {
        fill: fill.gamma_multiply(visibility),
        text_color: text_color.gamma_multiply(visibility),
    }
}
