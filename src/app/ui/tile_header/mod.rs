pub mod control;
pub mod split;

use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{SplitPath, ViewId};
use crate::app::shortcut_keymap::ShortcutAction;
use crate::app::shortcut_tooltips;
use crate::app::ui::transition;
use crate::app::ui::widget_ids;
use eframe::egui;

pub use control::{TileControl, TileControlStyle};
pub use split::{SplitPreviewOverlay, TILE_GAP, TileAction, TileSplitHandler, paint_split_preview};

pub(crate) struct TileHeaderRequest {
    pub(crate) tab_index: usize,
    pub(crate) view_id: ViewId,
    pub(crate) pane_path: SplitPath,
    pub(crate) tile_rect: egui::Rect,
    pub(crate) can_close: bool,
}

pub(crate) struct TileHeaderState<'a> {
    pub(crate) actions: &'a mut Vec<TileAction>,
    pub(crate) preview_overlay: &'a mut Option<SplitPreviewOverlay>,
}

pub(crate) fn render_tile_header(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileHeaderRequest,
    state: &mut TileHeaderState<'_>,
) {
    let split_handler =
        TileSplitHandler::new(&request.pane_path, request.view_id, request.tile_rect);
    let controls_visible = control_visibility(ui, &split_handler, request.tile_rect);
    let tab = &app.tab_manager.tabs.as_slice()[request.tab_index];
    let can_promote = crate::app::domain::tab::summary::can_promote_view(tab, request.view_id);
    let layout = pass_stable_header_layout(
        ui,
        request.tab_index,
        request.view_id,
        TileHeaderLayout {
            controls_visible,
            can_promote,
            can_close: request.can_close,
        },
    );
    if layout.controls_visible <= 0.0 {
        return;
    }

    let metrics = tile_control_metrics(request.tile_rect, layout.can_close);
    let rects = tile_header_rects(
        request.tile_rect,
        layout.can_promote,
        layout.can_close,
        &metrics,
    );
    let control = TileControlContext {
        font_size: metrics.font_size,
        visibility: controls_visible,
    };
    if layout.can_promote && header_button_clicked(ui, &control, rects.promote_hit, &PROMOTE_BUTTON)
    {
        state.actions.push(TileAction::Promote(request.view_id));
    }
    let split_response = show_split_control(
        ui,
        rects.split_hit,
        metrics.font_size,
        layout.controls_visible,
    );
    if let Some(preview_state) =
        split_handler.handle_interaction(ui, &split_response, state.actions)
    {
        let (title, preview_lines) = preview_content_for_view(tab, request.view_id);
        *state.preview_overlay =
            Some(split_handler.make_preview(preview_state, title, preview_lines, rects.split_hit));
    }
    if layout.can_close && header_button_clicked(ui, &control, rects.close_hit, &CLOSE_BUTTON) {
        state.actions.push(TileAction::Close(request.view_id));
    }
}

/// Title and first lines of the view's buffer, shown in the floating split preview.
fn preview_content_for_view(
    tab: &crate::app::domain::WorkspaceTab,
    view_id: ViewId,
) -> (String, Vec<String>) {
    match tab.buffer_for_view(view_id) {
        Some(buffer) => (
            buffer.display_name(),
            split::build_preview_lines(&buffer.text()),
        ),
        None => (
            crate::app::domain::tab::summary::display_name(tab),
            split::build_preview_lines(""),
        ),
    }
}

struct TileHeaderRects {
    promote_hit: egui::Rect,
    split_hit: egui::Rect,
    close_hit: egui::Rect,
}

#[derive(Clone, Copy)]
struct TileHeaderLayout {
    controls_visible: f32,
    can_promote: bool,
    can_close: bool,
}

struct TileControlMetrics {
    button_size: f32,
    padding: f32,
    spacing: f32,
    font_size: f32,
}

struct TileControlContext {
    font_size: f32,
    visibility: f32,
}

/// A click-only header button whose tooltip names its shortcut.
struct HeaderButton {
    icon: &'static str,
    action: ShortcutAction,
    label: &'static str,
    style: TileControlStyle,
}

const PROMOTE_BUTTON: HeaderButton = HeaderButton {
    icon: egui_phosphor::regular::ARROW_LINE_UP,
    action: ShortcutAction::PromoteTileToTab,
    label: "Promote Tile",
    style: TileControlStyle::Default,
};

const CLOSE_BUTTON: HeaderButton = HeaderButton {
    icon: egui_phosphor::regular::X,
    action: ShortcutAction::CloseTile,
    label: "Close Tile",
    style: TileControlStyle::Danger,
};

const TILE_CONTROL_PADDING: f32 = 6.0;
const TILE_CONTROL_MIN_SIZE: f32 = 18.0;
const TILE_CONTROL_MAX_SIZE: f32 = crate::app::theme::BUTTON_SIZE.x;
const TILE_CONTROL_RIGHT_INSET: f32 = 14.0;

fn pass_stable_header_layout(
    ui: &egui::Ui,
    tab_index: usize,
    view_id: ViewId,
    layout: TileHeaderLayout,
) -> TileHeaderLayout {
    let storage_id = widget_ids::root_id(("tile_header.layout", tab_index, view_id));
    let frame = ui.ctx().cumulative_frame_nr();
    if ui.ctx().current_pass_index() == 0 {
        ui.ctx()
            .data_mut(|data| data.insert_temp(storage_id, (frame, layout)));
        return layout;
    }

    ui.ctx()
        .data(|data| data.get_temp::<(u64, TileHeaderLayout)>(storage_id))
        .filter(|(stable_frame, _)| *stable_frame == frame)
        .map_or(layout, |(_, stable_layout)| stable_layout)
}

fn control_visibility(
    ui: &egui::Ui,
    split_handler: &TileSplitHandler,
    tile_rect: egui::Rect,
) -> f32 {
    let hovered = !transition::suppress_interactive_chrome(ui.ctx())
        && tile_rect.contains(pointer_hover_pos(ui));
    if split_handler.is_dragging(ui) || hovered {
        1.0
    } else {
        0.0
    }
}

fn pointer_hover_pos(ui: &egui::Ui) -> egui::Pos2 {
    ui.input(|input| input.pointer.hover_pos().unwrap_or_default())
}

fn show_split_control(
    ui: &mut egui::Ui,
    split_hit: egui::Rect,
    font_size: f32,
    controls_visible: f32,
) -> egui::Response {
    let tooltip = shortcut_tooltips::action(
        ui.ctx(),
        ShortcutAction::SplitTile,
        "Split Tile (drag in a direction to choose placement)",
    );
    TileControl::new(egui_phosphor::regular::ARROWS_SPLIT)
        .visibility(controls_visible)
        .font_size(font_size)
        .tooltip(&tooltip)
        .show(ui, split_hit, egui::Sense::click_and_drag())
}

fn header_button_clicked(
    ui: &mut egui::Ui,
    control: &TileControlContext,
    hit_rect: egui::Rect,
    button: &HeaderButton,
) -> bool {
    let tooltip = shortcut_tooltips::action(ui.ctx(), button.action, button.label);
    TileControl::new(button.icon)
        .style(button.style)
        .visibility(control.visibility)
        .font_size(control.font_size)
        .tooltip(&tooltip)
        .show(ui, hit_rect, egui::Sense::click())
        .clicked()
}

fn tile_header_rects(
    tile_rect: egui::Rect,
    can_promote: bool,
    can_close: bool,
    metrics: &TileControlMetrics,
) -> TileHeaderRects {
    let control_y = tile_rect.top() + metrics.padding;
    let right_edge = tile_rect.right() - TILE_CONTROL_RIGHT_INSET;
    let close_hit_x = right_edge - metrics.button_size - metrics.padding;
    let left_of = |x: f32, occupied: bool| {
        if occupied {
            x - metrics.spacing - metrics.button_size
        } else {
            x
        }
    };
    let split_hit_x = left_of(close_hit_x, can_close);
    let promote_hit_x = left_of(split_hit_x, can_promote);
    let button_at = |x: f32| {
        egui::Rect::from_min_size(
            egui::pos2(x, control_y),
            egui::Vec2::splat(metrics.button_size),
        )
    };

    TileHeaderRects {
        promote_hit: button_at(promote_hit_x),
        split_hit: button_at(split_hit_x),
        close_hit: button_at(close_hit_x),
    }
}

/// The area a tile's visible header controls occupy, so overlays can avoid them.
pub(crate) fn tile_controls_bounds(
    tile_rect: egui::Rect,
    can_promote: bool,
    can_close: bool,
) -> egui::Rect {
    let metrics = tile_control_metrics(tile_rect, can_close);
    let rects = tile_header_rects(tile_rect, can_promote, can_close, &metrics);
    let leftmost = if can_promote {
        rects.promote_hit
    } else {
        rects.split_hit
    };
    // Without a close button the close slot coincides with the split button.
    leftmost.union(rects.close_hit)
}

fn tile_control_metrics(tile_rect: egui::Rect, can_close: bool) -> TileControlMetrics {
    let width_share = if can_close { 0.12 } else { 0.15 };
    let button_size =
        (tile_rect.width() * width_share).clamp(TILE_CONTROL_MIN_SIZE, TILE_CONTROL_MAX_SIZE);
    let scale = (button_size / TILE_CONTROL_MAX_SIZE).clamp(0.6, 1.0);

    TileControlMetrics {
        button_size,
        padding: (TILE_CONTROL_PADDING * scale).clamp(3.0, TILE_CONTROL_PADDING),
        spacing: (4.0 * scale).clamp(2.0, 4.0),
        font_size: (button_size * 0.55).clamp(12.0, 16.0),
    }
}

#[cfg(test)]
mod tests {
    use super::tile_controls_bounds;
    use eframe::egui;

    #[test]
    fn control_bounds_cover_only_the_buttons_a_tile_shows() {
        let tile = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));
        for (can_promote, can_close, left) in [
            (true, true, 282.0),
            (false, true, 316.0),
            (true, false, 316.0),
            (false, false, 350.0),
        ] {
            assert_eq!(
                tile_controls_bounds(tile, can_promote, can_close),
                egui::Rect::from_min_max(egui::pos2(left, 6.0), egui::pos2(380.0, 36.0)),
                "promote {can_promote}, close {can_close}"
            );
        }
    }
}
