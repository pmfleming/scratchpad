use super::geometry::top_drag_button_position;
use crate::app::app_state::ScratchpadApp;
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::{
    BUTTON_SIZE, HEADER_HEIGHT, action_bg, action_hover_bg, text_primary,
};
use eframe::egui;

pub(super) fn show_top_drag_bar(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if !matches!(app.tab_list_position(), TabListPosition::Left | TabListPosition::Right) {
        return;
    }

    let ctx = ui.ctx().clone();
    let viewport = ui.max_rect();
    if !pointer_near_top_edge(ui, viewport) {
        return;
    }

    let button_position = top_drag_button_position(app, viewport);
    egui::Area::new(egui::Id::new("top_drag_button"))
        .order(egui::Order::Foreground)
        .fixed_pos(button_position)
        .show(&ctx, |ui| {
            render_top_drag_button(&ctx, ui);
        });
}

fn pointer_near_top_edge(ui: &egui::Ui, viewport: egui::Rect) -> bool {
    ui.input(|input| {
        input
            .pointer
            .hover_pos()
            .is_some_and(|pos| pos.y <= viewport.top() + HEADER_HEIGHT + 12.0)
    })
}

fn render_top_drag_button(ctx: &egui::Context, ui: &mut egui::Ui) {
    let (rect, response) = ui.allocate_exact_size(BUTTON_SIZE, egui::Sense::click_and_drag());
    let fill = if response.hovered() {
        action_hover_bg(ui)
    } else {
        action_bg(ui)
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::DOTS_SIX,
        egui::FontId::proportional(16.0),
        text_primary(ui),
    );

    if response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if response.double_clicked() {
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}