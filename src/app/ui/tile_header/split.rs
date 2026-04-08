use crate::app::domain::{SplitAxis, ViewId};
use crate::app::theme::*;
use eframe::egui;

pub const TILE_GAP: f32 = 6.0;
pub const SPLIT_DRAG_THRESHOLD: f32 = 12.0;

#[derive(Clone, Copy)]
pub struct SplitHandleDragState {
    pub start_pos: egui::Pos2,
    pub current_pos: egui::Pos2,
}

pub struct SplitPreviewOverlay {
    pub axis: Option<SplitAxis>,
    pub new_view_first: bool,
    pub ratio: f32,
    pub pointer_pos: egui::Pos2,
    pub tile_rect: egui::Rect,
    pub handle_anchor: egui::Pos2,
    pub title: String,
    pub preview_lines: Vec<String>,
}

pub enum TileAction {
    Activate(ViewId),
    Close(ViewId),
    #[allow(dead_code)]
    ResizeSplit {
        path: crate::app::domain::SplitPath,
        ratio: f32,
    },
    Split {
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    },
}

pub struct TileSplitHandler {
    id: egui::Id,
    #[allow(dead_code)]
    tab_index: usize,
    view_id: ViewId,
    tile_rect: egui::Rect,
}

impl TileSplitHandler {
    pub fn new(ui: &egui::Ui, tab_index: usize, view_id: ViewId, tile_rect: egui::Rect) -> Self {
        Self {
            id: split_drag_state_id(ui, tab_index, view_id),
            tab_index,
            view_id,
            tile_rect,
        }
    }

    pub fn is_dragging(&self, ui: &egui::Ui) -> bool {
        split_drag_state(ui, self.id).is_some()
    }

    pub fn handle_interaction(
        &self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        actions: &mut Vec<TileAction>,
    ) -> Option<SplitHandleDragState> {
        begin_split_drag_if_needed(ui, response, self.id);
        update_split_drag_state(ui, self.id, self.tile_rect, self.view_id, actions)
    }

    pub fn make_preview(
        &self,
        state: SplitHandleDragState,
        title: String,
        content: &str,
        handle_rect: egui::Rect,
    ) -> SplitPreviewOverlay {
        let spec = split_preview_spec(self.tile_rect, state.current_pos - state.start_pos);
        SplitPreviewOverlay {
            axis: spec.map(|(axis, _, _)| axis),
            new_view_first: spec
                .map(|(_, new_view_first, _)| new_view_first)
                .unwrap_or(false),
            ratio: spec.map(|(_, _, ratio)| ratio).unwrap_or(0.5),
            pointer_pos: state.current_pos,
            tile_rect: self.tile_rect,
            handle_anchor: handle_rect.right_top(),
            title,
            preview_lines: build_preview_lines(content),
        }
    }
}

pub fn paint_split_preview(ui: &egui::Ui, overlay: &SplitPreviewOverlay) {
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

pub fn split_preview_spec(
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
    let line_color = if is_new_tile {
        egui::Color32::from_rgba_unmultiplied(220, 235, 255, 180)
    } else {
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 54)
    };

    paint_preview_frame(ui, rect, is_new_tile);
    paint_preview_content(ui, rect, title, preview_lines, line_color);
}

fn paint_preview_frame(ui: &egui::Ui, rect: egui::Rect, is_new_tile: bool) {
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

    painter.rect_filled(rect, 6.0, frame_fill);
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(if is_new_tile { 2.0 } else { 1.0 }, border),
        egui::StrokeKind::Outside,
    );
}

fn paint_preview_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    title: &str,
    preview_lines: &[String],
    line_color: egui::Color32,
) {
    let painter = ui.painter();
    let usable_width = rect.width() - 20.0;
    let top_offset = 12.0;

    for (index, line) in preview_lines.iter().take(4).enumerate() {
        let y = rect.top() + top_offset + index as f32 * 14.0;
        if y > rect.bottom() - 6.0 {
            break;
        }
        painter.text(
            egui::pos2(rect.left() + 10.0, y),
            egui::Align2::LEFT_TOP,
            elide_preview_line(line, usable_width),
            egui::FontId::monospace(11.0),
            line_color,
        );
    }

    if !title.is_empty() {
        painter.text(
            egui::pos2(rect.left() + 10.0, rect.bottom() - 10.0),
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
