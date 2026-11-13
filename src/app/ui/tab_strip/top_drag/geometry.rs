use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PaneNode, SplitAxis, ViewId, WorkspaceTab};
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::{BUTTON_SIZE, HEADER_VERTICAL_PADDING};
use crate::app::ui::editor_area::split_rect;
use crate::app::ui::tile_header::tile_controls_bounds;
use eframe::egui;

const DRAG_BUTTON_EDGE_MARGIN: f32 = 8.0;
const DRAG_BUTTON_CLEARANCE: f32 = 6.0;
const TOP_SPLIT_CENTER_BAND_RATIO: f32 = 0.2;

pub(super) fn top_drag_button_position(app: &ScratchpadApp, viewport: egui::Rect) -> egui::Pos2 {
    egui::pos2(
        top_drag_button_center_x(app, viewport) - BUTTON_SIZE.x * 0.5,
        viewport.top() + HEADER_VERTICAL_PADDING,
    )
}

fn top_drag_button_center_x(app: &ScratchpadApp, viewport: egui::Rect) -> f32 {
    let bounds = drag_button_center_bounds(viewport);
    let default_center = viewport.center().x.clamp(bounds.0, bounds.1);
    let Some(tab) = app.tab_manager.active_tab() else {
        return default_center;
    };

    let workspace_rect =
        top_drag_workspace_rect(viewport, app.state.app_settings.tab_list_position(), app);
    let preferred_center = preferred_top_split_center_x(&tab.layout.root_pane, workspace_rect)
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
    let inset = crate::app::app_state::settings_state::vertical_tab_list_width(app);
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
        .min_by(|a, b| (a - center_x).abs().total_cmp(&(b - center_x).abs()))
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
    collect_top_edge_leaf_rects(&tab.layout.root_pane, rect, rect.top(), &mut top_leaf_rects);
    let can_close = tab.layout.root_pane.leaf_count() > 1;

    top_leaf_rects
        .into_iter()
        .map(|(view_id, tile_rect)| {
            let can_promote = crate::app::domain::tab::summary::can_promote_view(tab, view_id);
            tile_controls_bounds(tile_rect, can_promote, can_close).expand(DRAG_BUTTON_CLEARANCE)
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
        .filter(|&candidate| {
            let button = drag_button_rect(candidate);
            !exclusion_rects
                .iter()
                .any(|exclusion| button.intersects(*exclusion))
        })
        .min_by(|a, b| {
            (a - preferred_center)
                .abs()
                .total_cmp(&(b - preferred_center).abs())
        })
        .unwrap_or(preferred_center.clamp(bounds.0, bounds.1))
}

fn drag_button_rect(center_x: f32) -> egui::Rect {
    egui::Rect::from_center_size(egui::pos2(center_x, 0.0), BUTTON_SIZE)
}

#[cfg(test)]
mod tests {
    use super::{BUTTON_SIZE, preferred_top_split_center_x, resolve_drag_button_center_x};
    use crate::app::domain::{PaneNode, SplitAxis};
    use eframe::egui;

    fn split(axis: SplitAxis, ratio: f32, first: PaneNode, second: PaneNode) -> PaneNode {
        PaneNode::Split {
            axis,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    #[test]
    fn drag_button_prefers_a_top_edge_vertical_split_near_the_centre() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0));
        let leaf = PaneNode::leaf;
        let cases = [
            (leaf(1), None),
            (
                split(SplitAxis::Vertical, 0.4, leaf(1), leaf(2)),
                Some(400.0),
            ),
            (split(SplitAxis::Vertical, 0.75, leaf(1), leaf(2)), None),
            (
                split(
                    SplitAxis::Horizontal,
                    0.5,
                    split(SplitAxis::Vertical, 0.6, leaf(1), leaf(2)),
                    split(SplitAxis::Vertical, 0.5, leaf(3), leaf(4)),
                ),
                Some(600.0),
            ),
        ];
        for (root, expected) in cases {
            assert_eq!(preferred_top_split_center_x(&root, rect), expected);
        }
    }

    #[test]
    fn drag_button_moves_beside_tile_controls_and_stays_in_bounds() {
        let half = BUTTON_SIZE.x * 0.5;
        let controls = egui::Rect::from_min_max(egui::pos2(480.0, -10.0), egui::pos2(560.0, 30.0));
        let bounds = (20.0, 980.0);
        assert_eq!(
            resolve_drag_button_center_x(500.0, 500.0, bounds, &[]),
            500.0
        );
        assert_eq!(resolve_drag_button_center_x(5.0, 500.0, bounds, &[]), 20.0);
        assert_eq!(
            resolve_drag_button_center_x(500.0, 500.0, bounds, &[controls]),
            480.0 - 6.0 - half
        );
        assert_eq!(
            resolve_drag_button_center_x(550.0, 550.0, bounds, &[controls]),
            560.0 + 6.0 + half
        );
    }
}
