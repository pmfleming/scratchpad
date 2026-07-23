use super::{TileRenderRequest, context_menu};
use crate::app::app_state::{ScratchpadApp, workspace::accessors as workspace_accessors};
use crate::app::domain::ViewId;
use crate::app::platform::{PlatformProfile, resolved_profile};
use crate::app::theme::{border, tab_selected_accent};
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
        workspace_accessors::request_focus_for_view(app, request.view_id);
    }
    tile_response
}

pub(super) fn paint_tile_frame(ui: &egui::Ui, rect: egui::Rect, background_color: egui::Color32) {
    ui.painter().rect_filled(rect, 4.0, background_color);
}

pub(super) fn paint_tile_border(
    ui: &egui::Ui,
    rect: egui::Rect,
    is_active: bool,
    multiple_tiles_visible: bool,
    profile: PlatformProfile,
) {
    let Some(highlighted) = tile_border_highlight_state(is_active, multiple_tiles_visible) else {
        return;
    };
    let stroke = tile_border_stroke(ui, highlighted, profile);
    // Center shared-edge strokes so window and tab borders occupy the same line.
    ui.painter()
        .rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Middle);
}

fn tile_border_highlight_state(is_active: bool, multiple_tiles_visible: bool) -> Option<bool> {
    multiple_tiles_visible.then_some(is_active)
}

fn tile_border_stroke(ui: &egui::Ui, highlighted: bool, profile: PlatformProfile) -> egui::Stroke {
    if resolved_profile(profile) == PlatformProfile::Hyprland
        && let Some(style) = crate::app::system_appearance::hyprland_border_style()
    {
        return egui::Stroke::new(
            style.width,
            if highlighted {
                style.active
            } else {
                style.inactive
            },
        );
    }

    if highlighted {
        egui::Stroke::new(2.0, tab_selected_accent(ui))
    } else {
        egui::Stroke::new(1.0, border(ui).gamma_multiply(0.55))
    }
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

#[cfg(test)]
mod tests {
    use super::tile_border_highlight_state;

    #[test]
    fn single_visible_tile_has_no_border() {
        assert_eq!(tile_border_highlight_state(true, false), None);
        assert_eq!(tile_border_highlight_state(false, false), None);
    }

    #[test]
    fn multiple_visible_tiles_distinguish_active_border() {
        assert_eq!(tile_border_highlight_state(true, true), Some(true));
        assert_eq!(tile_border_highlight_state(false, true), Some(false));
    }
}
