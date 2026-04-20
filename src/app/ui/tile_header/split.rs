mod drag;
mod geometry;
mod preview;

use crate::app::domain::{SplitAxis, ViewId};
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
    Promote(ViewId),
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
    view_id: ViewId,
    tile_rect: egui::Rect,
}

impl TileSplitHandler {
    pub fn new(ui: &egui::Ui, tab_index: usize, view_id: ViewId, tile_rect: egui::Rect) -> Self {
        Self {
            id: drag::split_drag_state_id(ui, tab_index, view_id),
            view_id,
            tile_rect,
        }
    }

    pub fn is_dragging(&self, ui: &egui::Ui) -> bool {
        drag::split_drag_active(ui, self.id)
    }

    pub fn handle_interaction(
        &self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        actions: &mut Vec<TileAction>,
    ) -> Option<SplitHandleDragState> {
        drag::handle_split_interaction(ui, response, self.id, self.tile_rect, self.view_id, actions)
    }

    pub fn make_preview(
        &self,
        state: SplitHandleDragState,
        title: String,
        preview_lines: Vec<String>,
        handle_rect: egui::Rect,
    ) -> SplitPreviewOverlay {
        let spec = geometry::split_preview_spec(self.tile_rect, state.start_pos, state.current_pos);
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
            preview_lines,
        }
    }
}

pub fn paint_split_preview(ui: &egui::Ui, overlay: &SplitPreviewOverlay) {
    preview::paint_split_preview(ui, overlay);
}

pub fn split_preview_spec(
    tile_rect: egui::Rect,
    start_pos: egui::Pos2,
    current_pos: egui::Pos2,
) -> Option<(SplitAxis, bool, f32)> {
    geometry::split_preview_spec(tile_rect, start_pos, current_pos)
}

pub(crate) use geometry::split_rect;
pub(crate) use preview::{build_preview_lines, build_preview_lines_for_window};
