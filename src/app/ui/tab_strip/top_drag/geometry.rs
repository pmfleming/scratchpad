use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PaneNode, SplitAxis, ViewId, WorkspaceTab};
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::{BUTTON_SIZE, HEADER_VERTICAL_PADDING};
use crate::app::ui::editor_area::split_rect;
use eframe::egui;

const DRAG_BUTTON_EDGE_MARGIN: f32 = 8.0;
const DRAG_BUTTON_CLEARANCE: f32 = 6.0;
const TOP_SPLIT_CENTER_BAND_RATIO: f32 = 0.2;
const TILE_CONTROL_PADDING: f32 = 6.0;
const TILE_CONTROL_MIN_SIZE: f32 = 18.0;
const TILE_CONTROL_SPACING: f32 = 4.0;
const TILE_CONTROL_RIGHT_INSET: f32 = 14.0;

pub(super) fn top_drag_button_position(app: &ScratchpadApp, viewport: egui::Rect) -> egui::Pos2 {
    egui::pos2(
        top_drag_button_center_x(app, viewport) - BUTTON_SIZE.x * 0.5,
        viewport.top() + HEADER_VERTICAL_PADDING,
    )
}

fn top_drag_button_center_x(app: &ScratchpadApp, viewport: egui::Rect) -> f32 {
    let bounds = drag_button_center_bounds(viewport);
    let default_center = viewport.center().x.clamp(bounds.0, bounds.1);
    let Some(tab) = app.active_tab() else {
        return default_center;
    };

    let workspace_rect = top_drag_workspace_rect(viewport, app.tab_list_position(), app);
    let preferred_center = preferred_top_split_center_x(&tab.root_pane, workspace_rect)
        .unwrap_or(default_center)
        .clamp(bounds.0, bounds.1);
    let exclusion_rects = top_tile_control_exclusion_rects(tab, workspace_rect);

    resolve_drag_button_center_x(preferred_center, default_center, bounds, &exclusion_rects)
}

fn top_drag_workspace_rect(
    viewport: egui::Rect,
    position: TabListPosition,
    app: &ScratchpadApp,
) -> egui::Rect {
    let mut min = viewport.min;
    let mut max = viewport.max;
    let inset = app.vertical_tab_list_width();
    match position {
        TabListPosition::Left => min.x += inset,
        TabListPosition::Right => max.x -= inset,
        TabListPosition::Top | TabListPosition::Bottom => {}
    }
    egui::Rect::from_min_max(min, max)
}

fn drag_button_center_bounds(viewport: egui::Rect) -> (f32, f32) {
    (
        viewport.left() + DRAG_BUTTON_EDGE_MARGIN + BUTTON_SIZE.x * 0.5,
        viewport.right() - DRAG_BUTTON_EDGE_MARGIN - BUTTON_SIZE.x * 0.5,
    )
}

fn preferred_top_split_center_x(root: &PaneNode, rect: egui::Rect) -> Option<f32> {
    let center_x = rect.center().x;
    let tolerance = rect.width() * TOP_SPLIT_CENTER_BAND_RATIO;
    let mut split_centers = Vec::new();
    collect_top_edge_vertical_split_centers(root, rect, rect.top(), &mut split_centers);
    split_centers
        .into_iter()
        .min_by(|a, b| {
            (a - center_x)
                .abs()
                .partial_cmp(&(b - center_x).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|split_x| (*split_x - center_x).abs() <= tolerance)
}

fn collect_top_edge_vertical_split_centers(
    node: &PaneNode,
    rect: egui::Rect,
    top_edge: f32,
    split_centers: &mut Vec<f32>,
) {
    if !touches_top_edge(rect, top_edge) {
        return;
    }

    match node {
        PaneNode::Leaf { .. } => {}
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let (first_rect, second_rect) = split_rect(rect, *axis, *ratio);
            if *axis == SplitAxis::Vertical {
                split_centers.push(rect.left() + rect.width() * ratio.clamp(0.2, 0.8));
                collect_top_edge_vertical_split_centers(first, first_rect, top_edge, split_centers);
                collect_top_edge_vertical_split_centers(
                    second,
                    second_rect,
                    top_edge,
                    split_centers,
                );
            } else {
                collect_top_edge_vertical_split_centers(first, first_rect, top_edge, split_centers);
            }
        }
    }
}

fn touches_top_edge(rect: egui::Rect, top_edge: f32) -> bool {
    (rect.top() - top_edge).abs() <= 1.0
}

fn top_tile_control_exclusion_rects(tab: &WorkspaceTab, rect: egui::Rect) -> Vec<egui::Rect> {
    let mut top_leaf_rects = Vec::new();
    collect_top_edge_leaf_rects(&tab.root_pane, rect, rect.top(), &mut top_leaf_rects);
    let can_close = tab.root_pane.leaf_count() > 1;

    top_leaf_rects
        .into_iter()
        .filter_map(|(view_id, tile_rect)| {
            top_tile_controls_rect(tile_rect, tab.can_promote_view(view_id), can_close)
        })
        .collect()
}

fn collect_top_edge_leaf_rects(
    node: &PaneNode,
    rect: egui::Rect,
    top_edge: f32,
    output: &mut Vec<(ViewId, egui::Rect)>,
) {
    if !touches_top_edge(rect, top_edge) {
        return;
    }

    match node {
        PaneNode::Leaf { view_id } => output.push((*view_id, rect)),
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let (first_rect, second_rect) = split_rect(rect, *axis, *ratio);
            collect_top_edge_leaf_rects(first, first_rect, top_edge, output);
            if *axis == SplitAxis::Vertical {
                collect_top_edge_leaf_rects(second, second_rect, top_edge, output);
            }
        }
    }
}

fn top_tile_controls_rect(
    tile_rect: egui::Rect,
    can_promote: bool,
    can_close: bool,
) -> Option<egui::Rect> {
    let button_size = if can_close {
        (tile_rect.width() * 0.12).clamp(TILE_CONTROL_MIN_SIZE, BUTTON_SIZE.x)
    } else {
        (tile_rect.width() * 0.15).clamp(TILE_CONTROL_MIN_SIZE, BUTTON_SIZE.x)
    };
    let scale = (button_size / BUTTON_SIZE.x).clamp(0.6, 1.0);
    let padding = (TILE_CONTROL_PADDING * scale).clamp(3.0, TILE_CONTROL_PADDING);
    let spacing = (TILE_CONTROL_SPACING * scale).clamp(2.0, TILE_CONTROL_SPACING);
    let control_y = tile_rect.top() + padding;
    let right_edge = tile_rect.right() - TILE_CONTROL_RIGHT_INSET;
    let close_hit_x = right_edge - button_size - padding;
    let split_hit_x = if can_close {
        close_hit_x - spacing - button_size
    } else {
        close_hit_x
    };
    let left_x = if can_promote {
        split_hit_x - spacing - button_size
    } else {
        split_hit_x
    };
    let right_x = if can_close {
        close_hit_x + button_size
    } else {
        split_hit_x + button_size
    };
    (right_x > left_x).then(|| {
        egui::Rect::from_min_max(
            egui::pos2(
                left_x - DRAG_BUTTON_CLEARANCE,
                control_y - DRAG_BUTTON_CLEARANCE,
            ),
            egui::pos2(
                right_x + DRAG_BUTTON_CLEARANCE,
                control_y + button_size + DRAG_BUTTON_CLEARANCE,
            ),
        )
    })
}

fn resolve_drag_button_center_x(
    preferred_center: f32,
    fallback_center: f32,
    bounds: (f32, f32),
    exclusion_rects: &[egui::Rect],
) -> f32 {
    let mut candidates = vec![preferred_center, fallback_center];
    for exclusion in exclusion_rects {
        candidates.push(exclusion.left() - DRAG_BUTTON_CLEARANCE - BUTTON_SIZE.x * 0.5);
        candidates.push(exclusion.right() + DRAG_BUTTON_CLEARANCE + BUTTON_SIZE.x * 0.5);
    }

    candidates
        .into_iter()
        .map(|candidate| candidate.clamp(bounds.0, bounds.1))
        .filter(|candidate| !drag_button_rect(*candidate).intersects_any(exclusion_rects))
        .min_by(|a, b| {
            (a - preferred_center)
                .abs()
                .partial_cmp(&(b - preferred_center).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(preferred_center.clamp(bounds.0, bounds.1))
}

fn drag_button_rect(center_x: f32) -> egui::Rect {
    egui::Rect::from_center_size(egui::pos2(center_x, 0.0), BUTTON_SIZE)
}

trait RectIntersections {
    fn intersects_any(&self, others: &[egui::Rect]) -> bool;
}

impl RectIntersections for egui::Rect {
    fn intersects_any(&self, others: &[egui::Rect]) -> bool {
        others.iter().any(|other| self.intersects(*other))
    }
}
