use super::{TileRenderRequest, context_menu};
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::ViewId;
use crate::app::theme::{border, header_bg};
use crate::app::ui::tile_header::TileAction;
use crate::app::ui::widget_ids;
use eframe::egui;

pub(super) fn handle_tile_click(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: &TileRenderRequest,
    actions: &mut Vec<TileAction>,
) -> egui::Response {
    let tile_response = widget_ids::interact(
        ui,
        request.rect,
        widget_ids::root_id(("tile", request.tab_index, request.view_id)),
        egui::Sense::click(),
        "editor_tile",
    );
    context_menu::activate_inactive_tile_on_secondary_click(app, &tile_response, request);
    if tile_response.clicked() {
        actions.push(TileAction::Activate(request.view_id));
        app.request_focus_for_view(request.view_id);
    }
    tile_response
}

pub(super) fn paint_tile_frame(
    ui: &egui::Ui,
    rect: egui::Rect,
    is_active: bool,
    background_color: egui::Color32,
) {
    let bg = if is_active {
        header_bg(ui)
    } else {
        background_color
    };
    let border_color = border(ui).gamma_multiply(0.0);

    ui.painter().rect_filled(rect, 4.0, bg);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Inside,
    );
}

pub(super) fn apply_tile_body_focus(
    body_focused: bool,
    is_active: bool,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) {
    if body_focused && !is_active {
        actions.push(TileAction::Activate(view_id));
    }
}
