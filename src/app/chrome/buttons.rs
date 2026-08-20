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
        Self {
            background,
            hover_background,
            icon,
            hover_icon: icon,
        }
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
    phosphor_button_with_icon_color(
        ui,
        surface_key,
        icon,
        size,
        PhosphorButtonColors::new(background, hover_background, text_primary(ui)),
        tooltip,
    )
}

pub fn phosphor_button_with_icon_color(
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
    let rect = response.rect;
    paint_phosphor_button(PhosphorButtonPaint {
        ui,
        rect,
        icon,
        hovered: response.hovered(),
        drag_in_progress: transition::suppress_interactive_chrome(ui.ctx()),
        colors,
    });

    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

pub fn phosphor_button_with_hover_icon_color(
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
    let rect = response.rect;
    let hovered = response.hovered();
    let drag_in_progress = transition::suppress_interactive_chrome(ui.ctx());
    paint_phosphor_button(PhosphorButtonPaint {
        ui,
        rect,
        icon,
        hovered,
        drag_in_progress,
        colors,
    });

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
        phosphor_button_with_hover_icon_color(
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

struct PhosphorButtonPaint<'a> {
    ui: &'a egui::Ui,
    rect: Rect,
    icon: &'a str,
    hovered: bool,
    drag_in_progress: bool,
    colors: PhosphorButtonColors,
}

fn paint_phosphor_button(request: PhosphorButtonPaint<'_>) {
    let fill = button_fill(
        request.hovered,
        request.drag_in_progress,
        request.colors.background,
        request.colors.hover_background,
    );
    let icon_color = if request.hovered && !request.drag_in_progress {
        request.colors.hover_icon
    } else {
        request.colors.icon
    };
    request.ui.painter().rect_filled(request.rect, 4.0, fill);
    request.ui.painter().text(
        request.rect.center(),
        egui::Align2::CENTER_CENTER,
        request.icon,
        egui_phosphor::font_id(16.0),
        icon_color,
    );
}

fn button_fill(
    hovered: bool,
    drag_in_progress: bool,
    background: Color32,
    hover_background: Color32,
) -> Color32 {
    if hovered && !drag_in_progress {
        hover_background
    } else {
        background
    }
}
