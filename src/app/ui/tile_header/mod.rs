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
    let can_promote = crate::app::domain::tab::summary::can_promote_view(
        &app.tab_manager.tabs.as_slice()[request.tab_index],
        request.view_id,
    );
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
        pane_path: request.pane_path.clone(),
    };
    if layout.can_promote
        && show_control(
            ui,
            &control,
            rects.promote_hit,
            TileControlSpec {
                label: egui_phosphor::regular::ARROW_LINE_UP,
                tooltip: Some(shortcut_tooltips::action(
                    ui.ctx(),
                    ShortcutAction::PromoteTileToTab,
                    "Promote Tile",
                )),
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
        &request.pane_path,
        rects.split_hit,
        metrics.font_size,
        layout.controls_visible,
    );
    if let Some(preview_state) =
        split_handler.handle_interaction(ui, &split_response, state.actions)
    {
        let tab = &app.tab_manager.tabs.as_slice()[request.tab_index];
        let title = tab.buffer_for_view(request.view_id).map_or_else(
            || crate::app::domain::tab::summary::display_name(tab),
            |buffer| buffer.display_name(),
        );
        let preview_lines = preview_lines_for_view(tab, request.view_id);
        *state.preview_overlay = Some(split_handler.make_preview(
            preview_state,
            title.clone(),
            preview_lines,
            rects.split_hit,
        ));
    }
    if layout.can_close
        && show_control(
            ui,
            &control,
            rects.close_hit,
            TileControlSpec {
                label: "×",
                tooltip: Some(shortcut_tooltips::action(
                    ui.ctx(),
                    ShortcutAction::CloseTile,
                    "Close Tile",
                )),
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
    tab.buffer_for_view(view_id).map_or_else(
        || split::build_preview_lines(""),
        |buffer| split::build_preview_lines(&buffer.text()),
    )
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

#[derive(Clone)]
struct TileControlContext {
    font_size: f32,
    visibility: f32,
    pane_path: SplitPath,
}

struct TileControlSpec {
    label: &'static str,
    tooltip: Option<String>,
    style: TileControlStyle,
    sense: egui::Sense,
    id_prefix: &'static str,
}

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
    pane_path: &SplitPath,
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
        .show(
            ui,
            split_hit,
            widget_ids::root_id(("split_handle", pane_path)),
            egui::Sense::click_and_drag(),
        )
}

fn show_control(
    ui: &mut egui::Ui,
    control: &TileControlContext,
    hit_rect: egui::Rect,
    spec: TileControlSpec,
) -> egui::Response {
    let mut tile_control = TileControl::new(spec.label)
        .style(spec.style)
        .visibility(control.visibility)
        .font_size(control.font_size);
    if let Some(tooltip) = &spec.tooltip {
        tile_control = tile_control.tooltip(tooltip);
    }
    tile_control.show(
        ui,
        hit_rect,
        widget_ids::root_id((spec.id_prefix, &control.pane_path)),
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
