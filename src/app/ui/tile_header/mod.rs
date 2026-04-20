pub mod control;
pub mod split;

use crate::app::app_state::ScratchpadApp;
use crate::app::domain::ViewId;
use crate::app::ui::transition;
use eframe::egui;

pub use control::{TileControl, TileControlStyle};
pub use split::{SplitPreviewOverlay, TILE_GAP, TileAction, TileSplitHandler, paint_split_preview};

pub(crate) struct TileHeaderRequest {
    pub(crate) tab_index: usize,
    pub(crate) view_id: ViewId,
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
    let title = app.tabs()[request.tab_index]
        .buffer_for_view(request.view_id)
        .map(|buffer| buffer.display_name())
        .unwrap_or_else(|| app.tabs()[request.tab_index].display_name());
    let preview_lines = preview_lines_for_view(&app.tabs()[request.tab_index], request.view_id);
    let split_handler =
        TileSplitHandler::new(ui, request.tab_index, request.view_id, request.tile_rect);
    let controls_visible = control_visibility(ui, &split_handler, request.tile_rect);
    if controls_visible <= 0.0 {
        return;
    }

    let can_promote = app.tabs()[request.tab_index].can_promote_view(request.view_id);
    let metrics = tile_control_metrics(request.tile_rect, request.can_close);
    let rects = tile_header_rects(request.tile_rect, can_promote, request.can_close, &metrics);
    let control = TileControlContext {
        tab_index: request.tab_index,
        view_id: request.view_id,
        font_size: metrics.font_size,
        visibility: controls_visible,
    };
    if can_promote
        && show_control(
            ui,
            control,
            rects.promote_hit,
            TileControlSpec {
                label: egui_phosphor::regular::ARROW_LINE_UP,
                tooltip: Some("Promote this file's tiles into a new top-level tab"),
                style: TileControlStyle::Default,
                sense: egui::Sense::click(),
                id_prefix: "promote_view",
            },
        )
        .clicked()
    {
        state.actions.push(TileAction::Promote(request.view_id));
    }
    let split_response = show_split_control(
        ui,
        request.tab_index,
        request.view_id,
        rects.split_hit,
        metrics.font_size,
        controls_visible,
    );
    if let Some(preview_state) =
        split_handler.handle_interaction(ui, &split_response, state.actions)
    {
        *state.preview_overlay = Some(split_handler.make_preview(
            preview_state,
            title.to_owned(),
            preview_lines,
            rects.split_hit,
        ));
    }
    if request.can_close
        && show_control(
            ui,
            control,
            rects.close_hit,
            TileControlSpec {
                label: "×",
                tooltip: None,
                style: TileControlStyle::Danger,
                sense: egui::Sense::click(),
                id_prefix: "close_view",
            },
        )
        .clicked()
    {
        state.actions.push(TileAction::Close(request.view_id));
    }
}

fn preview_lines_for_view(tab: &crate::app::domain::WorkspaceTab, view_id: ViewId) -> Vec<String> {
    tab.view(view_id)
        .and_then(|view| view.latest_layout.as_ref())
        .and_then(|layout| layout.visible_text.as_ref())
        .map(split::build_preview_lines_for_window)
        .or_else(|| {
            tab.buffer_for_view(view_id)
                .map(|buffer| split::build_preview_lines(&buffer.text()))
        })
        .unwrap_or_else(|| split::build_preview_lines(""))
}

struct TileHeaderRects {
    promote_hit: egui::Rect,
    split_hit: egui::Rect,
    close_hit: egui::Rect,
}

struct TileControlMetrics {
    button_size: f32,
    padding: f32,
    spacing: f32,
    font_size: f32,
}

#[derive(Clone, Copy)]
struct TileControlContext {
    tab_index: usize,
    view_id: ViewId,
    font_size: f32,
    visibility: f32,
}

struct TileControlSpec {
    label: &'static str,
    tooltip: Option<&'static str>,
    style: TileControlStyle,
    sense: egui::Sense,
    id_prefix: &'static str,
}

const TILE_CONTROL_PADDING: f32 = 6.0;
const TILE_CONTROL_MIN_SIZE: f32 = 18.0;
const TILE_CONTROL_MAX_SIZE: f32 = crate::app::theme::BUTTON_SIZE.x;
const TILE_CONTROL_RIGHT_INSET: f32 = 14.0;

fn control_visibility(
    ui: &egui::Ui,
    split_handler: &TileSplitHandler,
    tile_rect: egui::Rect,
) -> f32 {
    if !split_handler.is_dragging(ui) && transition::suppress_interactive_chrome(ui.ctx()) {
        return 0.0;
    }

    if split_handler.is_dragging(ui) || tile_rect.contains(pointer_hover_pos(ui)) {
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
    tab_index: usize,
    view_id: ViewId,
    split_hit: egui::Rect,
    font_size: f32,
    controls_visible: f32,
) -> egui::Response {
    TileControl::new(egui_phosphor::regular::ARROWS_SPLIT)
        .visibility(controls_visible)
        .font_size(font_size)
        .tooltip("Drag to split: left/right creates a vertical split, up/down creates a horizontal split")
        .show(
            ui,
            split_hit,
            ui.make_persistent_id(("split_handle", tab_index, view_id)),
            egui::Sense::click_and_drag(),
        )
}

fn show_control(
    ui: &mut egui::Ui,
    control: TileControlContext,
    hit_rect: egui::Rect,
    spec: TileControlSpec,
) -> egui::Response {
    let mut tile_control = TileControl::new(spec.label)
        .style(spec.style)
        .visibility(control.visibility)
        .font_size(control.font_size);
    if let Some(tooltip) = spec.tooltip {
        tile_control = tile_control.tooltip(tooltip);
    }
    tile_control.show(
        ui,
        hit_rect,
        ui.make_persistent_id((spec.id_prefix, control.tab_index, control.view_id)),
        spec.sense,
    )
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
    let split_hit_x = if can_close {
        close_hit_x - metrics.spacing - metrics.button_size
    } else {
        close_hit_x
    };
    let promote_hit_x = if can_promote {
        split_hit_x - metrics.spacing - metrics.button_size
    } else {
        split_hit_x
    };
    let promote_hit = egui::Rect::from_min_size(
        egui::pos2(promote_hit_x, control_y),
        egui::vec2(metrics.button_size, metrics.button_size),
    );
    let split_hit = egui::Rect::from_min_size(
        egui::pos2(split_hit_x, control_y),
        egui::vec2(metrics.button_size, metrics.button_size),
    );
    let close_hit = egui::Rect::from_min_size(
        egui::pos2(close_hit_x, control_y),
        egui::vec2(metrics.button_size, metrics.button_size),
    );

    TileHeaderRects {
        promote_hit,
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
