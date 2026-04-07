use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{SplitAxis, SplitPath, ViewId};
use crate::app::theme::*;
use eframe::egui;

pub(crate) const TILE_GAP: f32 = 6.0;
pub(crate) const TILE_CONTROL_PADDING: f32 = 6.0;
pub(crate) const SPLIT_DRAG_THRESHOLD: f32 = 12.0;
const TILE_CONTROL_MIN_SIZE: f32 = 18.0;
const TILE_CONTROL_MAX_SIZE: f32 = BUTTON_SIZE.x;

#[derive(Clone, Copy)]
pub(crate) struct SplitHandleDragState {
    pub start_pos: egui::Pos2,
    pub current_pos: egui::Pos2,
}

pub(crate) struct SplitPreviewOverlay {
    pub axis: Option<SplitAxis>,
    pub new_view_first: bool,
    pub ratio: f32,
    pub pointer_pos: egui::Pos2,
    pub tile_rect: egui::Rect,
    pub handle_anchor: egui::Pos2,
    pub title: String,
    pub preview_lines: Vec<String>,
}

pub(crate) enum TileControlStyle {
    Default,
    Danger,
}

pub(crate) enum TileAction {
    Activate(ViewId),
    Close(ViewId),
    ResizeSplit {
        path: SplitPath,
        ratio: f32,
    },
    Split {
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    },
}

struct TileHeaderRects {
    split_hit: egui::Rect,
    split_icon: egui::Rect,
    close_hit: egui::Rect,
    close_icon: egui::Rect,
}

struct TileControlMetrics {
    button_size: f32,
    padding: f32,
    spacing: f32,
    font_size: f32,
}

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
    let title = app.tabs[tab_index].buffer.display_name();
    let split_drag_state_id = split_drag_state_id(ui, tab_index, view_id);
    let controls_visible = tile_controls_visibility(ui, tab_index, view_id, tile_rect).max(
        if split_drag_state(ui, split_drag_state_id).is_some() {
            1.0
        } else {
            0.0
        },
    );
    if controls_visible <= 0.0 {
        return;
    }

    let metrics = tile_control_metrics(tile_rect, can_close);
    let rects = tile_header_rects(tile_rect, can_close, &metrics);
    let split_response = ui.interact(
        rects.split_hit,
        ui.make_persistent_id(("split_handle", tab_index, view_id)),
        egui::Sense::click_and_drag(),
    );
    begin_split_drag_if_needed(ui, &split_response, split_drag_state_id);
    let split_drag_state =
        update_split_drag_state(ui, split_drag_state_id, tile_rect, view_id, actions);

    let split_hovered = split_is_hovered(ui, rects.split_hit);
    let split_dragging = split_drag_state.is_some();
    paint_tile_control(
        ui,
        rects.split_icon,
        egui_phosphor::regular::ARROWS_SPLIT,
        split_hovered || split_dragging,
        TileControlStyle::Default,
        controls_visible,
        metrics.font_size,
    );
    if split_response.hovered() {
        split_response.clone().on_hover_text(
            "Drag to split: left/right creates a vertical split, up/down creates a horizontal split",
        );
    }
    if let Some(state) = split_drag_state {
        let spec = split_preview_spec(tile_rect, state.current_pos - state.start_pos);
        *preview_overlay = Some(SplitPreviewOverlay {
            axis: spec.map(|(axis, _, _)| axis),
            new_view_first: spec
                .map(|(_, new_view_first, _)| new_view_first)
                .unwrap_or(false),
            ratio: spec.map(|(_, _, ratio)| ratio).unwrap_or(0.5),
            pointer_pos: state.current_pos,
            tile_rect,
            handle_anchor: rects.split_hit.right_top(),
            title,
            preview_lines: build_preview_lines(&app.tabs[tab_index].buffer.content),
        });
    }

    if can_close {
        let close_response = ui.interact(
            rects.close_hit,
            ui.make_persistent_id(("close_view", tab_index, view_id)),
            egui::Sense::click(),
        );
        paint_tile_control(
            ui,
            rects.close_icon,
            "×",
            close_response.hovered(),
            TileControlStyle::Danger,
            controls_visible,
            metrics.font_size,
        );
        if close_response.clicked() {
            actions.push(TileAction::Close(view_id));
        }
    }
}

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
        split_icon: split_hit,
        close_hit,
        close_icon: close_hit,
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

fn tile_controls_visibility(
    ui: &egui::Ui,
    tab_index: usize,
    view_id: ViewId,
    tile_rect: egui::Rect,
) -> f32 {
    let _ = (tab_index, view_id);
    let tile_hovered =
        tile_rect.contains(ui.input(|input| input.pointer.hover_pos().unwrap_or_default()));
    if tile_hovered { 1.0 } else { 0.0 }
}

fn split_drag_state_id(ui: &egui::Ui, tab_index: usize, view_id: ViewId) -> egui::Id {
    ui.make_persistent_id(("split_handle_drag", tab_index, view_id))
}

fn split_drag_state(ui: &egui::Ui, split_drag_state_id: egui::Id) -> Option<SplitHandleDragState> {
    ui.ctx()
        .data(|data| data.get_temp::<SplitHandleDragState>(split_drag_state_id))
}

fn begin_split_drag_if_needed(
    ui: &egui::Ui,
    split_response: &egui::Response,
    split_drag_state_id: egui::Id,
) {
    if split_response.hovered()
        && ui.input(|input| input.pointer.primary_pressed())
        && let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos())
    {
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                split_drag_state_id,
                SplitHandleDragState {
                    start_pos: pointer_pos,
                    current_pos: pointer_pos,
                },
            );
        });
    }
}

fn update_split_drag_state(
    ui: &egui::Ui,
    split_drag_state_id: egui::Id,
    tile_rect: egui::Rect,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) -> Option<SplitHandleDragState> {
    let state = ui
        .ctx()
        .data(|data| data.get_temp::<SplitHandleDragState>(split_drag_state_id));
    let state = state?;

    if ui.input(|input| input.pointer.primary_down()) {
        return refresh_split_drag_state(ui, split_drag_state_id, state);
    }

    clear_split_drag_state(ui, split_drag_state_id);
    commit_split_drag_action(tile_rect, state, view_id, actions);
    None
}

fn refresh_split_drag_state(
    ui: &egui::Ui,
    split_drag_state_id: egui::Id,
    mut state: SplitHandleDragState,
) -> Option<SplitHandleDragState> {
    let pointer_pos = ui.input(|input| input.pointer.latest_pos())?;
    state.current_pos = pointer_pos;
    ui.ctx().data_mut(|data| {
        data.insert_temp(split_drag_state_id, state);
    });
    Some(state)
}

fn clear_split_drag_state(ui: &egui::Ui, split_drag_state_id: egui::Id) {
    ui.ctx().data_mut(|data| {
        data.remove::<SplitHandleDragState>(split_drag_state_id);
    });
}

fn commit_split_drag_action(
    tile_rect: egui::Rect,
    state: SplitHandleDragState,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) {
    if let Some((axis, new_view_first, ratio)) =
        split_preview_spec(tile_rect, state.current_pos - state.start_pos)
    {
        actions.push(TileAction::Activate(view_id));
        actions.push(TileAction::Split {
            axis,
            new_view_first,
            ratio,
        });
    }
}

fn split_is_hovered(ui: &egui::Ui, split_hit_rect: egui::Rect) -> bool {
    split_hit_rect.contains(ui.input(|input| input.pointer.hover_pos().unwrap_or_default()))
}

pub(crate) fn paint_tile_control(
    ui: &egui::Ui,
    rect: egui::Rect,
    label: &str,
    hovered: bool,
    style: TileControlStyle,
    visibility: f32,
    font_size: f32,
) {
    if visibility <= 0.0 {
        return;
    }

    let (fill, stroke) = match style {
        TileControlStyle::Default => {
            let fill = if hovered {
                egui::Color32::from_rgb(56, 72, 98)
            } else {
                egui::Color32::from_white_alpha(12)
            };
            let stroke = if hovered {
                egui::Color32::from_rgb(104, 154, 232)
            } else {
                egui::Color32::from_white_alpha(20)
            };
            (fill, stroke)
        }
        TileControlStyle::Danger => {
            let fill = if hovered {
                CLOSE_HOVER_BG
            } else {
                CLOSE_BG.gamma_multiply(0.6)
            };
            let stroke = if hovered {
                egui::Color32::from_rgb(255, 196, 196)
            } else {
                egui::Color32::from_rgba_unmultiplied(255, 150, 150, 90)
            };
            (fill, stroke)
        }
    };

    let fill = fill.gamma_multiply(visibility);
    let stroke = stroke.gamma_multiply(visibility);
    let text_color = TEXT_PRIMARY.gamma_multiply(visibility);

    ui.painter().rect_filled(rect, 3.0, fill);
    ui.painter().rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, stroke),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(font_size),
        text_color,
    );
}

pub(crate) fn paint_split_preview(ui: &egui::Ui, overlay: &SplitPreviewOverlay) {
    let preview_shell_rect = overlay.tile_rect.shrink(1.0);
    ui.painter().rect_stroke(
        preview_shell_rect,
        6.0,
        egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(120, 180, 255, 90),
        ),
        egui::StrokeKind::Outside,
    );

    let Some(axis) = overlay.axis else {
        paint_pending_split_hint(ui, preview_shell_rect);
        paint_floating_preview_tile(ui, overlay, false);
        return;
    };

    let preview_rect = overlay.tile_rect.shrink(2.0);
    let (first_rect, second_rect) = split_rect(preview_rect, axis, overlay.ratio);
    let (new_tile_rect, existing_tile_rect) = if overlay.new_view_first {
        (first_rect, second_rect)
    } else {
        (second_rect, first_rect)
    };

    paint_target_split_region(ui, new_tile_rect);
    paint_preview_tile(
        ui,
        existing_tile_rect,
        false,
        "Current tile",
        &overlay.preview_lines,
    );
    paint_floating_preview_tile(ui, overlay, true);
}

pub(crate) fn split_preview_spec(
    tile_rect: egui::Rect,
    drag_delta: egui::Vec2,
) -> Option<(SplitAxis, bool, f32)> {
    let horizontal_fraction = (drag_delta.x.abs() / tile_rect.width().max(1.0)).clamp(0.0, 1.0);
    let vertical_fraction = (drag_delta.y.abs() / tile_rect.height().max(1.0)).clamp(0.0, 1.0);
    if horizontal_fraction.max(vertical_fraction) == 0.0
        || drag_delta.length() < SPLIT_DRAG_THRESHOLD
    {
        return None;
    }

    let axis = if horizontal_fraction >= vertical_fraction {
        SplitAxis::Vertical
    } else {
        SplitAxis::Horizontal
    };
    let (dominant_delta, extent, new_view_first) = match axis {
        SplitAxis::Vertical => (drag_delta.x.abs(), tile_rect.width(), drag_delta.x < 0.0),
        SplitAxis::Horizontal => (drag_delta.y.abs(), tile_rect.height(), drag_delta.y < 0.0),
    };
    let new_tile_fraction = (dominant_delta / extent.max(1.0)).clamp(0.3, 0.7);
    let ratio = if new_view_first {
        new_tile_fraction
    } else {
        1.0 - new_tile_fraction
    };

    Some((axis, new_view_first, ratio.clamp(0.2, 0.8)))
}

fn paint_pending_split_hint(ui: &egui::Ui, tile_rect: egui::Rect) {
    ui.painter().rect_filled(
        tile_rect,
        6.0,
        egui::Color32::from_rgba_unmultiplied(120, 180, 255, 18),
    );
    ui.painter().text(
        tile_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::ARROWS_SPLIT,
        egui::FontId::proportional(18.0),
        egui::Color32::from_rgba_unmultiplied(190, 220, 255, 180),
    );
}

fn paint_target_split_region(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().rect_filled(
        rect.shrink(1.0),
        6.0,
        egui::Color32::from_rgba_unmultiplied(120, 180, 255, 26),
    );
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        6.0,
        egui::Stroke::new(
            2.0,
            egui::Color32::from_rgba_unmultiplied(120, 180, 255, 160),
        ),
        egui::StrokeKind::Outside,
    );
}

fn paint_floating_preview_tile(ui: &egui::Ui, overlay: &SplitPreviewOverlay, resolved: bool) {
    let max_rect = ui.max_rect().shrink(8.0);
    let anchor = egui::pos2(
        overlay
            .handle_anchor
            .x
            .clamp(max_rect.left() + 32.0, max_rect.right()),
        overlay
            .handle_anchor
            .y
            .clamp(max_rect.top(), max_rect.bottom() - 32.0),
    );
    let pointer = egui::pos2(
        overlay
            .pointer_pos
            .x
            .clamp(max_rect.left(), max_rect.right() - 32.0),
        overlay
            .pointer_pos
            .y
            .clamp(anchor.y + 32.0, max_rect.bottom()),
    );
    let rect = egui::Rect::from_min_max(
        egui::pos2(pointer.x.min(anchor.x - 32.0), anchor.y),
        egui::pos2(anchor.x, pointer.y.max(anchor.y + 32.0)),
    );
    paint_preview_tile(
        ui,
        rect,
        true,
        if resolved {
            &overlay.title
        } else {
            "Split preview"
        },
        &overlay.preview_lines,
    );
}

fn paint_preview_tile(
    ui: &egui::Ui,
    rect: egui::Rect,
    is_new_tile: bool,
    title: &str,
    preview_lines: &[String],
) {
    let painter = ui.painter();
    let frame_fill = if is_new_tile {
        egui::Color32::from_rgba_unmultiplied(46, 63, 88, 220)
    } else {
        HEADER_BG.gamma_multiply(0.94)
    };
    let border = if is_new_tile {
        egui::Color32::from_rgba_unmultiplied(120, 180, 255, 220)
    } else {
        egui::Color32::from_rgba_unmultiplied(120, 180, 255, 70)
    };
    let body_rect = rect;

    painter.rect_filled(rect, 6.0, frame_fill);
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(if is_new_tile { 2.0 } else { 1.0 }, border),
        egui::StrokeKind::Outside,
    );

    let line_color = if is_new_tile {
        egui::Color32::from_rgba_unmultiplied(220, 235, 255, 180)
    } else {
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 54)
    };
    let usable_width = body_rect.width() - 20.0;
    let top_offset = 12.0;
    for (index, line) in preview_lines.iter().take(4).enumerate() {
        let y = body_rect.top() + top_offset + index as f32 * 14.0;
        if y > body_rect.bottom() - 6.0 {
            break;
        }
        painter.text(
            egui::pos2(body_rect.left() + 10.0, y),
            egui::Align2::LEFT_TOP,
            elide_preview_line(line, usable_width),
            egui::FontId::monospace(11.0),
            line_color,
        );
    }

    if !title.is_empty() {
        painter.text(
            egui::pos2(body_rect.left() + 10.0, body_rect.bottom() - 10.0),
            egui::Align2::LEFT_BOTTOM,
            elide_preview_line(title, usable_width),
            egui::FontId::proportional(11.0),
            line_color.gamma_multiply(0.75),
        );
    }
}

pub(crate) fn build_preview_lines(content: &str) -> Vec<String> {
    let mut lines = content
        .lines()
        .take(4)
        .map(|line| line.replace('\t', "    "))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::from("Untitled"));
    }
    lines
}

pub(crate) fn elide_preview_line(line: &str, max_width: f32) -> String {
    let max_chars = ((max_width / 7.0).floor() as usize).max(8);
    if line.chars().count() <= max_chars {
        return line.to_owned();
    }

    let trimmed = line
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    format!("{trimmed}…")
}

pub(crate) fn split_rect(
    rect: egui::Rect,
    axis: SplitAxis,
    ratio: f32,
) -> (egui::Rect, egui::Rect) {
    match axis {
        SplitAxis::Horizontal => {
            let split_y = rect.top() + rect.height() * ratio;
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), split_y)),
                egui::Rect::from_min_max(egui::pos2(rect.left(), split_y), rect.max),
            )
        }
        SplitAxis::Vertical => {
            let split_x = rect.left() + rect.width() * ratio;
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(split_x, rect.bottom())),
                egui::Rect::from_min_max(egui::pos2(split_x, rect.top()), rect.max),
            )
        }
    }
}
