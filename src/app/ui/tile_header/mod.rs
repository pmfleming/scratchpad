pub mod control;
pub mod split;

use crate::app::app_state::ScratchpadApp;
use crate::app::domain::ViewId;
use eframe::egui;

pub use control::{TileControl, TileControlStyle};
pub use split::{SplitPreviewOverlay, TileAction, TileSplitHandler, paint_split_preview, TILE_GAP};

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_tile_header(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    tile_rect: egui::Rect,
    can_close: bool,
    actions: &mut Vec<TileAction>,
    preview_overlay: &mut Option<SplitPreviewOverlay>,
) {
    let title = app.tabs()[tab_index].buffer.display_name();
    let content_preview = app.tabs()[tab_index].buffer.content.clone();
    let split_handler = TileSplitHandler::new(ui, tab_index, view_id, tile_rect);
    let controls_visible = if split_handler.is_dragging(ui)
        || tile_rect.contains(ui.input(|input| input.pointer.hover_pos().unwrap_or_default()))
    {
        1.0
    } else {
        0.0
    };
    if controls_visible <= 0.0 {
        return;
    }

    let metrics = tile_control_metrics(tile_rect, can_close);
    let rects = tile_header_rects(tile_rect, can_close, &metrics);
    let split_response = TileControl::new(egui_phosphor::regular::ARROWS_SPLIT)
        .visibility(controls_visible)
        .font_size(metrics.font_size)
        .tooltip("Drag to split: left/right creates a vertical split, up/down creates a horizontal split")
        .show(
            ui,
            rects.split_hit,
            ui.make_persistent_id(("split_handle", tab_index, view_id)),
            egui::Sense::click_and_drag(),
        );

    if let Some(state) = split_handler.handle_interaction(ui, &split_response, actions) {
        *preview_overlay = Some(split_handler.make_preview(state, title, &content_preview, rects.split_hit));
    }

    if can_close {
        let close_response = TileControl::new("×")
            .style(TileControlStyle::Danger)
            .visibility(controls_visible)
            .font_size(metrics.font_size)
            .show(
                ui,
                rects.close_hit,
                ui.make_persistent_id(("close_view", tab_index, view_id)),
                egui::Sense::click(),
            );
        if close_response.clicked() {
            actions.push(TileAction::Close(view_id));
        }
    }
}

struct TileHeaderRects {
    split_hit: egui::Rect,
    close_hit: egui::Rect,
}

struct TileControlMetrics {
    button_size: f32,
    padding: f32,
    spacing: f32,
    font_size: f32,
}

const TILE_CONTROL_PADDING: f32 = 6.0;
const TILE_CONTROL_MIN_SIZE: f32 = 18.0;
const TILE_CONTROL_MAX_SIZE: f32 = crate::app::theme::BUTTON_SIZE.x;

fn tile_header_rects(
    tile_rect: egui::Rect,
    can_close: bool,
    metrics: &TileControlMetrics,
) -> TileHeaderRects {
    let control_y = tile_rect.top() + metrics.padding;
    let split_hit_x = if can_close {
        tile_rect.right() - (metrics.button_size * 2.0) - metrics.padding - metrics.spacing
    } else {
        tile_rect.right() - metrics.button_size - metrics.padding
    };
    let split_hit = egui::Rect::from_min_size(
        egui::pos2(split_hit_x, control_y),
        egui::vec2(metrics.button_size, metrics.button_size),
    );
    let close_hit = egui::Rect::from_min_size(
        egui::pos2(
            tile_rect.right() - metrics.button_size - metrics.padding,
            control_y,
        ),
        egui::vec2(metrics.button_size, metrics.button_size),
    );

    TileHeaderRects {
        split_hit,
        close_hit,
    }
}

fn tile_control_metrics(tile_rect: egui::Rect, can_close: bool) -> TileControlMetrics {
    let button_size = if can_close {
        (tile_rect.width() * 0.12).clamp(TILE_CONTROL_MIN_SIZE, TILE_CONTROL_MAX_SIZE)
    } else {
        (tile_rect.width() * 0.15).clamp(TILE_CONTROL_MIN_SIZE, TILE_CONTROL_MAX_SIZE)
    };
    let scale = (button_size / TILE_CONTROL_MAX_SIZE).clamp(0.6, 1.0);

    TileControlMetrics {
        button_size,
        padding: (TILE_CONTROL_PADDING * scale).clamp(3.0, TILE_CONTROL_PADDING),
        spacing: (4.0 * scale).clamp(2.0, 4.0),
        font_size: (button_size * 0.55).clamp(12.0, 16.0),
    }
}
