use crate::app::theme::*;
use crate::app::ui::transition;
use eframe::egui::{self, Color32, Rect, Sense, Vec2};

pub fn phosphor_button(
    ui: &mut egui::Ui,
    icon: &str,
    size: Vec2,
    background: Color32,
    hover_background: Color32,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    paint_phosphor_button(
        ui,
        rect,
        icon,
        response.hovered(),
        transition::suppress_interactive_chrome(ui.ctx()),
        background,
        hover_background,
    );

    response.on_hover_text(tooltip)
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
    phosphor_button(
        ui,
        egui_phosphor::regular::X,
        CAPTION_BUTTON_SIZE,
        CLOSE_BG,
        CLOSE_HOVER_BG,
        "Close",
    )
    .clicked()
}

fn paint_phosphor_button(
    ui: &egui::Ui,
    rect: Rect,
    icon: &str,
    hovered: bool,
    drag_in_progress: bool,
    background: Color32,
    hover_background: Color32,
) {
    let fill = button_fill(hovered, drag_in_progress, background, hover_background);
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(16.0),
        text_primary(ui),
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
