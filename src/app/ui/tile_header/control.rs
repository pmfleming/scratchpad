use crate::app::theme::*;
use crate::app::ui::transition;
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

    pub fn show(
        self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        id: egui::Id,
        sense: egui::Sense,
    ) -> egui::Response {
        let response = ui.interact(rect, id, sense);
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
        egui::FontId::proportional(font_size),
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
    TileControlColors {
        fill: fill.gamma_multiply(visibility),
        stroke: stroke.gamma_multiply(visibility),
        text_color: text_primary(ui).gamma_multiply(visibility),
    }
}

fn base_tile_control_colors(
    ui: &egui::Ui,
    style: TileControlStyle,
    hovered: bool,
) -> (egui::Color32, egui::Color32) {
    match style {
        TileControlStyle::Default => default_tile_control_colors(ui, hovered),
        TileControlStyle::Danger => danger_tile_control_colors(ui, hovered),
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

fn danger_tile_control_colors(ui: &egui::Ui, hovered: bool) -> (egui::Color32, egui::Color32) {
    let fill = if hovered {
        CLOSE_HOVER_BG
    } else if ui.visuals().dark_mode {
        egui::Color32::from_white_alpha(12)
    } else {
        egui::Color32::from_rgb(252, 232, 232)
    };
    let stroke = if hovered {
        egui::Color32::from_rgb(255, 196, 196)
    } else if ui.visuals().dark_mode {
        egui::Color32::from_white_alpha(20)
    } else {
        egui::Color32::from_rgb(220, 150, 150)
    };
    (fill, stroke)
}
