pub mod actions;
mod entries;
pub mod layout;
mod outcome;
pub mod tab_cell;

use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PaneNode, SplitAxis, ViewId, WorkspaceTab};
use crate::app::theme::{
    BUTTON_SIZE, HEADER_HEIGHT, HEADER_VERTICAL_PADDING, action_bg, action_hover_bg, text_primary,
};
use crate::app::ui::editor_area::split_rect;
use crate::app::services::settings_store::TabListPosition;
use eframe::egui;
use std::collections::HashSet;
use std::time::Instant;

pub(crate) use actions::{show_caption_controls, show_primary_actions};
use entries::{show_tab_region, show_vertical_tab_region};
pub(crate) use layout::HeaderLayout;
use layout::{
    AUTO_HIDE_PEEK_SIZE, auto_hide_panel_extent, horizontal_bar_visible,
    show_horizontal_edge_tab_list, vertical_panel_visible, vertical_tab_list_frame,
    vertical_tab_panel,
};
use outcome::apply_tab_outcome;
pub(crate) use tab_cell::{TabInteraction, render_tab_cell_sized};

#[derive(Default)]
pub(crate) struct TabStripOutcome {
    pub(crate) activated_tab: Option<usize>,
    pub(crate) activate_settings: bool,
    pub(crate) close_requested_tab: Option<usize>,
    pub(crate) close_settings: bool,
    pub(crate) promote_all_files_tab: Option<usize>,
    pub(crate) reordered_tabs: Option<(usize, usize)>,
    pub(crate) reordered_tab_group: Option<(Vec<usize>, usize)>,
    pub(crate) combined_tabs: Option<(usize, usize)>,
    pub(crate) combined_tab_group: Option<(Vec<usize>, usize)>,
    pub(crate) consumed_scroll_request: bool,
}

pub(crate) fn show_header(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    show_horizontal_tab_list(ui, app, TabListPosition::Top, "header");
}

pub(crate) fn show_top_drag_bar(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if vertical_tab_side(app.tab_list_position()).is_none() {
        return;
    }

    let ctx = ui.ctx().clone();
    let viewport = ui.max_rect();
    if !pointer_near_top_edge(ui, viewport) {
        return;
    }

    let button_position = top_drag_button_position(app, viewport);
    egui::Area::new(egui::Id::new("top_drag_button"))
        .order(egui::Order::Foreground)
        .fixed_pos(button_position)
        .show(&ctx, |ui| {
            render_top_drag_button(&ctx, ui);
        });
}

fn pointer_near_top_edge(ui: &egui::Ui, viewport: egui::Rect) -> bool {
    ui.input(|input| {
        input
            .pointer
            .hover_pos()
            .is_some_and(|pos| pos.y <= viewport.top() + HEADER_HEIGHT + 12.0)
    })
}

fn render_top_drag_button(ctx: &egui::Context, ui: &mut egui::Ui) {
    let (rect, response) = ui.allocate_exact_size(BUTTON_SIZE, egui::Sense::click_and_drag());
    let fill = if response.hovered() {
        action_hover_bg(ui)
    } else {
        action_bg(ui)
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::DOTS_SIX,
        egui::FontId::proportional(16.0),
        text_primary(ui),
    );

    if response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if response.double_clicked() {
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}

const DRAG_BUTTON_EDGE_MARGIN: f32 = 8.0;
const DRAG_BUTTON_CLEARANCE: f32 = 6.0;
const TOP_SPLIT_CENTER_BAND_RATIO: f32 = 0.2;
const TILE_CONTROL_PADDING: f32 = 6.0;
const TILE_CONTROL_MIN_SIZE: f32 = 18.0;
const TILE_CONTROL_SPACING: f32 = 4.0;
const TILE_CONTROL_RIGHT_INSET: f32 = 14.0;

fn top_drag_button_position(app: &ScratchpadApp, viewport: egui::Rect) -> egui::Pos2 {
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
    split_centers.into_iter().min_by(|a, b| {
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
                collect_top_edge_vertical_split_centers(
                    first,
                    first_rect,
                    top_edge,
                    split_centers,
                );
                collect_top_edge_vertical_split_centers(
                    second,
                    second_rect,
                    top_edge,
                    split_centers,
                );
            } else {
                collect_top_edge_vertical_split_centers(
                    first,
                    first_rect,
                    top_edge,
                    split_centers,
                );
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
            egui::pos2(left_x - DRAG_BUTTON_CLEARANCE, control_y - DRAG_BUTTON_CLEARANCE),
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

pub(crate) fn show_vertical_tab_list(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if let Some(side) = vertical_tab_side(app.tab_list_position()) {
        show_vertical_tab_panel(ui, app, side);
    }
}

fn show_vertical_tab_panel(ui: &mut egui::Ui, app: &mut ScratchpadApp, side: TabListPosition) {
    app.overflow_popup_open = false;
    let now = Instant::now();
    let panel_visible = vertical_panel_visible(ui, app, side, now);
    let panel_width = auto_hide_panel_extent(panel_visible, app.vertical_tab_list_width());

    let panel_response = vertical_tab_panel(side, panel_visible)
        .default_size(panel_width)
        .size_range(vertical_tab_panel_size_range(panel_visible))
        .resizable(panel_visible)
        .frame(vertical_tab_list_frame(ui))
        .show_inside(ui, |ui| {
            if !panel_visible {
                return;
            }
            let outcome = show_vertical_tab_region(ui, app);
            apply_tab_outcome(app, outcome);
        });

    finalize_vertical_tab_panel(app, panel_visible, &panel_response.response);
}

fn vertical_tab_panel_size_range(panel_visible: bool) -> std::ops::RangeInclusive<f32> {
    if panel_visible {
        ScratchpadApp::VERTICAL_TAB_LIST_MIN_WIDTH..=ScratchpadApp::VERTICAL_TAB_LIST_MAX_WIDTH
    } else {
        AUTO_HIDE_PEEK_SIZE..=AUTO_HIDE_PEEK_SIZE
    }
}

fn finalize_vertical_tab_panel(
    app: &mut ScratchpadApp,
    panel_visible: bool,
    response: &egui::Response,
) {
    if !panel_visible {
        app.close_tab_list();
        return;
    }

    app.set_tab_list_width_from_layout(response.rect.width());
}

pub(crate) fn show_bottom_tab_list(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    show_horizontal_tab_list(ui, app, TabListPosition::Bottom, "bottom_tab_list");
}

fn show_horizontal_tab_list(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    position: TabListPosition,
    panel_id: &'static str,
) {
    if app.tab_list_position() != position {
        return;
    }

    let ctx = ui.ctx().clone();
    let bar_visible = horizontal_bar_visible(ui, app, position, Instant::now());
    show_horizontal_edge_tab_list(ui, position, panel_id, true, bar_visible, |ui| {
        let outcome = show_horizontal_tab_bar(&ctx, ui, app);
        apply_tab_outcome(app, outcome);
    });
}

fn show_horizontal_tab_bar(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
) -> TabStripOutcome {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        show_primary_actions(ui, app);

        ui.add_space(8.0);
        let layout = HeaderLayout::measure(app, ui.available_width(), 4.0, true);
        let outcome = show_tab_region(ctx, ui, app, &layout);

        ui.add_space(8.0);
        show_caption_controls(ctx, ui, app, &layout);
        outcome
    })
    .inner
}

pub(crate) fn maybe_auto_scroll_tab_strip(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    layout: &HeaderLayout,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
) {
    if let Some(scroll_state) = egui::scroll_area::State::load(ui.ctx(), scroll_area_id) {
        crate::app::ui::tab_drag::auto_scroll_tab_list(
            ui.ctx(),
            scroll_area_id,
            viewport_rect,
            app.estimated_tab_strip_width(layout.spacing),
            &scroll_state,
            crate::app::ui::tab_drag::TabDropAxis::Horizontal,
        );
    }
}

pub(crate) fn maybe_scroll_to_active_tab(
    ui: &mut egui::Ui,
    index: usize,
    active_tab_index: usize,
    pending_scroll_to_active: bool,
    rect: egui::Rect,
    outcome: &mut TabStripOutcome,
) {
    if index == active_tab_index && pending_scroll_to_active {
        ui.scroll_to_rect(rect, Some(egui::Align::Center));
        outcome.consumed_scroll_request = true;
    }
}

pub(crate) fn record_visible_tab(
    index: usize,
    rect: egui::Rect,
    viewport_rect: egui::Rect,
    visible_tab_indices: &mut HashSet<usize>,
) {
    if viewport_rect.intersects(rect) {
        visible_tab_indices.insert(index);
    }
}

fn vertical_tab_side(position: TabListPosition) -> Option<TabListPosition> {
    match position {
        TabListPosition::Left | TabListPosition::Right => Some(position),
        TabListPosition::Top | TabListPosition::Bottom => None,
    }
}

pub(crate) fn apply_tab_interaction(outcome: &mut TabStripOutcome, interaction: TabInteraction) {
    match interaction {
        TabInteraction::None => {}
        TabInteraction::Activate(index) => outcome.activated_tab = Some(index),
        TabInteraction::RequestClose(index) => outcome.close_requested_tab = Some(index),
        TabInteraction::PromoteAllFiles(index) => outcome.promote_all_files_tab = Some(index),
    }
}

#[cfg(test)]
mod tests {
    use super::TabStripOutcome;
    use super::{
        preferred_top_split_center_x, resolve_drag_button_center_x, top_tile_controls_rect,
    };
    use crate::app::domain::{BufferState, PaneNode, SplitAxis, WorkspaceTab};
    use crate::app::ui::tab_strip::entries::apply_settings_tab_interaction;
    use crate::app::theme::BUTTON_SIZE;
    use eframe::egui;

    #[test]
    fn settings_tab_close_gesture_closes_settings_surface() {
        let mut outcome = TabStripOutcome::default();

        apply_settings_tab_interaction(&mut outcome, true, true, false);

        assert!(outcome.close_settings);
        assert!(!outcome.activate_settings);
        assert!(outcome.close_requested_tab.is_none());
    }

    #[test]
    fn clicking_settings_tab_activates_settings_surface() {
        let mut outcome = TabStripOutcome::default();

        apply_settings_tab_interaction(&mut outcome, false, false, true);

        assert!(outcome.activate_settings);
        assert!(!outcome.close_settings);
    }

    #[test]
    fn top_drag_button_prefers_near_center_top_split() {
        let mut tab = WorkspaceTab::new(BufferState::new("one.txt".to_owned(), String::new(), None));
        let second_view_id = tab
            .open_buffer_with_balanced_layout(BufferState::new(
                "two.txt".to_owned(),
                String::new(),
                None,
            ))
            .expect("second view");
        tab.root_pane = PaneNode::Split {
            axis: SplitAxis::Vertical,
            ratio: 0.48,
            first: Box::new(PaneNode::leaf(tab.views[0].id)),
            second: Box::new(PaneNode::leaf(second_view_id)),
        };
        let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1000.0, 700.0));

        let split_x = preferred_top_split_center_x(&tab.root_pane, rect);

        assert_eq!(split_x, Some(480.0));
    }

    #[test]
    fn drag_button_moves_outside_tile_control_conflict() {
        let exclusion = top_tile_controls_rect(
            egui::Rect::from_min_max(egui::pos2(350.0, 0.0), egui::pos2(650.0, 300.0)),
            true,
            true,
        )
        .expect("controls rect");

        let resolved = resolve_drag_button_center_x(560.0, 500.0, (50.0, 950.0), &[exclusion]);
        let drag_rect = egui::Rect::from_center_size(egui::pos2(resolved, 0.0), BUTTON_SIZE);

        assert!(!drag_rect.intersects(exclusion));
        assert!(resolved < 560.0);
    }
}
