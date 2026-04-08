use crate::app::theme::*;
use eframe::egui;

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
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            style: TileControlStyle::Default,
            visibility: 1.0,
            font_size: 14.0,
            tooltip: None,
        }
    }

    pub fn style(mut self, style: TileControlStyle) -> Self {
        self.style = style;
        self
    }

    pub fn visibility(mut self, visibility: f32) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn tooltip(mut self, tooltip: &'a str) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub fn show(self, ui: &mut egui::Ui, rect: egui::Rect, id: egui::Id, sense: egui::Sense) -> egui::Response {
        let response = ui.interact(rect, id, sense);
        
        if self.visibility > 0.0 {
            paint_tile_control(
                ui,
                rect,
                self.label,
                response.hovered() || response.dragged(),
                self.style,
                self.visibility,
                self.font_size,
            );

            if let Some(tooltip) = self.tooltip {
                response.clone().on_hover_text(tooltip);
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

    let (fill, stroke) = match style {
        TileControlStyle::Default => {
            let fill = if hovered {
                egui::Color32::from_rgb(56, 72, 98)
            } else {
                egui::Color32::from_white_alpha(12)
            };
            let stroke = if hovered {
                egui::Color32::from_rgb(104, 154, 232)
            } else {
                egui::Color32::from_white_alpha(20)
            };
            (fill, stroke)
        }
        TileControlStyle::Danger => {
            let fill = if hovered {
                CLOSE_HOVER_BG
            } else {
                CLOSE_BG.gamma_multiply(0.6)
            };
            let stroke = if hovered {
                egui::Color32::from_rgb(255, 196, 196)
            } else {
                egui::Color32::from_rgba_unmultiplied(255, 150, 150, 90)
            };
            (fill, stroke)
        }
    };

    let fill = fill.gamma_multiply(visibility);
    let stroke = stroke.gamma_multiply(visibility);
    let text_color = TEXT_PRIMARY.gamma_multiply(visibility);

    ui.painter().rect_filled(rect, 3.0, fill);
    ui.painter().rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, stroke),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(font_size),
        text_color,
    );
}
