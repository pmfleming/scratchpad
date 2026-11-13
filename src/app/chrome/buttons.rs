use crate::app::theme::{
    CAPTION_BUTTON_SIZE, CAPTION_BUTTON_SPACING, CAPTION_TRAILING_PADDING, CLOSE_HOVER_BG,
    TAB_HEIGHT, action_bg, action_hover_bg, text_primary,
};
use crate::app::ui::transition;
use crate::app::ui::widget_ids::{self, WidgetRole};
use eframe::egui::{self, Color32, Rect, Sense, Vec2};
use std::hash::Hash;

#[derive(Clone, Copy)]
pub struct PhosphorButtonColors {
    pub background: Color32,
    pub hover_background: Color32,
    pub icon: Color32,
    pub hover_icon: Color32,
}

impl PhosphorButtonColors {
    #[must_use]
    pub fn new(background: Color32, hover_background: Color32, icon: Color32) -> Self {
        Self::with_hover_icon(background, hover_background, icon, icon)
    }

    #[must_use]
    pub fn with_hover_icon(
        background: Color32,
        hover_background: Color32,
        icon: Color32,
        hover_icon: Color32,
    ) -> Self {
        Self {
            background,
            hover_background,
            icon,
            hover_icon,
        }
    }

    /// Fill and icon colour for the current interaction state.
    fn resolve(self, highlighted: bool) -> (Color32, Color32) {
        if highlighted {
            (self.hover_background, self.hover_icon)
        } else {
            (self.background, self.icon)
        }
    }
}

pub fn phosphor_button(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    icon: &str,
    size: Vec2,
    background: Color32,
    hover_background: Color32,
    tooltip: &str,
) -> egui::Response {
    let colors = PhosphorButtonColors::new(background, hover_background, text_primary(ui));
    phosphor_button_with_colors(ui, surface_key, icon, size, colors, tooltip)
}

pub fn phosphor_button_with_colors(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    icon: &str,
    size: Vec2,
    colors: PhosphorButtonColors,
    tooltip: &str,
) -> egui::Response {
    let response = widget_ids::allocate_exact_interact(
        ui,
        size,
        widget_ids::surface_role(surface_key, WidgetRole::IconButton),
        Sense::click(),
        "phosphor_button",
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), tooltip));
    let highlighted = response.hovered() && !transition::suppress_interactive_chrome(ui.ctx());
    let (fill, icon_color) = colors.resolve(highlighted);
    paint_phosphor_button(ui, response.rect, icon, fill, icon_color);

    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

pub fn caption_controls(ui: &mut egui::Ui, ctx: &egui::Context, width: f32) -> bool {
    let mut close_requested = false;

    ui.allocate_ui_with_layout(
        egui::vec2(width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            close_requested = render_caption_buttons(ui, ctx);
        },
    );

    close_requested
}

fn render_caption_buttons(ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
    ui.spacing_mut().item_spacing.x = CAPTION_BUTTON_SPACING;

    render_minimize_button(ui, ctx);
    render_maximize_restore_button(ui, ctx);
    let close_requested = render_close_button(ui);

    if CAPTION_TRAILING_PADDING > 0.0 {
        ui.add_space(CAPTION_TRAILING_PADDING);
    }

    close_requested
}

fn render_minimize_button(ui: &mut egui::Ui, ctx: &egui::Context) {
    if phosphor_button(
        ui,
        "caption_minimize",
        egui_phosphor::regular::MINUS,
        CAPTION_BUTTON_SIZE,
        action_bg(ui),
        action_hover_bg(ui),
        "Minimize",
    )
    .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
}

fn render_maximize_restore_button(ui: &mut egui::Ui, ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    let (icon, tooltip, next_maximized) = if maximized {
        (egui_phosphor::regular::COPY, "Restore", false)
    } else {
        (egui_phosphor::regular::SQUARE, "Maximize", true)
    };

    if phosphor_button(
        ui,
        "caption_maximize",
        icon,
        CAPTION_BUTTON_SIZE,
        action_bg(ui),
        action_hover_bg(ui),
        tooltip,
    )
    .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(next_maximized));
    }
}

fn render_close_button(ui: &mut egui::Ui) -> bool {
    widget_ids::scope(ui, "caption_close", |ui| {
        phosphor_button_with_colors(
            ui,
            "caption_close",
            egui_phosphor::regular::X,
            CAPTION_BUTTON_SIZE,
            PhosphorButtonColors::with_hover_icon(
                action_bg(ui),
                CLOSE_HOVER_BG,
                text_primary(ui),
                Color32::WHITE,
            ),
            "Close",
        )
        .clicked()
    })
    .inner
}

fn paint_phosphor_button(
    ui: &egui::Ui,
    rect: Rect,
    icon: &str,
    fill: Color32,
    icon_color: Color32,
) {
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui_phosphor::font_id(16.0),
        icon_color,
    );
}
