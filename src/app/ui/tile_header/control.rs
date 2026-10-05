use crate::app::theme::{CLOSE_HOVER_BG, action_bg, border, text_primary};
use crate::app::ui::transition;
use crate::app::ui::widget_ids;
use eframe::egui;

#[derive(Clone, Copy)]
pub enum TileControlStyle {
    Default,
    Danger,
}

pub struct TileControl<'a> {
    label: &'a str,
    style: TileControlStyle,
    visibility: f32,
    font_size: f32,
    tooltip: Option<&'a str>,
}

impl<'a> TileControl<'a> {
    #[must_use]
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            style: TileControlStyle::Default,
            visibility: 1.0,
            font_size: 14.0,
            tooltip: None,
        }
    }

    #[must_use]
    pub fn style(mut self, style: TileControlStyle) -> Self {
        self.style = style;
        self
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
                self.style,
                self.visibility,
                self.font_size,
            );

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
    style: TileControlStyle,
    visibility: f32,
    font_size: f32,
) {
    if visibility <= 0.0 {
        return;
    }

    let style_colors = tile_control_colors(ui, style, hovered, visibility);
    ui.painter().rect_filled(rect, 3.0, style_colors.fill);
    ui.painter().rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, style_colors.stroke),
        egui::StrokeKind::Inside,
    );
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
    stroke: egui::Color32,
    text_color: egui::Color32,
}

fn tile_control_colors(
    ui: &egui::Ui,
    style: TileControlStyle,
    hovered: bool,
    visibility: f32,
) -> TileControlColors {
    let (fill, stroke) = base_tile_control_colors(ui, style, hovered);
    let text_color = match style {
        TileControlStyle::Danger => egui::Color32::WHITE,
        TileControlStyle::Default => text_primary(ui),
    };
    TileControlColors {
        fill: fill.gamma_multiply(visibility),
        stroke: stroke.gamma_multiply(visibility),
        text_color: text_color.gamma_multiply(visibility),
    }
}

fn base_tile_control_colors(
    ui: &egui::Ui,
    style: TileControlStyle,
    hovered: bool,
) -> (egui::Color32, egui::Color32) {
    match style {
        TileControlStyle::Default => default_tile_control_colors(ui, hovered),
        TileControlStyle::Danger => danger_tile_control_colors(hovered),
    }
}

fn default_tile_control_colors(ui: &egui::Ui, hovered: bool) -> (egui::Color32, egui::Color32) {
    let fill = if hovered {
        egui::Color32::from_rgb(56, 72, 98)
    } else {
        action_bg(ui).gamma_multiply(0.8)
    };
    let stroke = if hovered {
        egui::Color32::from_rgb(104, 154, 232)
    } else {
        border(ui).gamma_multiply(0.8)
    };
    (fill, stroke)
}

fn danger_tile_control_colors(hovered: bool) -> (egui::Color32, egui::Color32) {
    if hovered {
        (CLOSE_HOVER_BG, egui::Color32::from_rgb(255, 196, 196))
    } else {
        (egui::Color32::BLACK, egui::Color32::BLACK)
    }
}

#[cfg(test)]
mod tests {
    use super::{CLOSE_HOVER_BG, TileControlStyle, tile_control_colors};
    use eframe::egui;

    #[test]
    fn close_control_is_white_on_black_in_both_themes() {
        for visuals in [egui::Visuals::dark(), egui::Visuals::light()] {
            let ctx = egui::Context::default();
            ctx.set_visuals(visuals);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let colors = tile_control_colors(ui, TileControlStyle::Danger, false, 1.0);
                assert_eq!(colors.fill, egui::Color32::BLACK);
                assert_eq!(colors.text_color, egui::Color32::WHITE);
            })
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn close_control_keeps_white_icon_on_red_hover() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let colors = tile_control_colors(ui, TileControlStyle::Danger, true, 1.0);
            assert_eq!(colors.fill, CLOSE_HOVER_BG);
            assert_eq!(colors.text_color, egui::Color32::WHITE);
        })
        .drop_without_applying_deltas();
    }
}
