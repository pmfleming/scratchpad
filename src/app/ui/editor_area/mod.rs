pub mod divider;
pub mod tile;

use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{BufferId, PaneBranch, PaneNode, ViewId};
use crate::app::transactions::TransactionSnapshot;
use crate::app::ui::search_replace;
use crate::app::ui::tile_header::{self, SplitPreviewOverlay, TileAction};
use eframe::egui;
use std::time::Duration;

pub use divider::{render_split_divider, split_rect};
use tile::{TileRenderRequest, TileRenderState};

pub(crate) fn show_editor(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        if app.tabs().is_empty() {
            return;
        }

        let ctx = ui.ctx().clone();
        app.refresh_search_state();
        search_replace::show_search_strip(ui, app);
        let workspace_rect = ui.available_rect_before_wrap();
        if workspace_rect.width() <= 0.0 || workspace_rect.height() <= 0.0 {
            return;
        }

        app.workspace_reflow_axis = preferred_workspace_reflow_axis(workspace_rect);
        handle_editor_zoom(&ctx, workspace_rect, ui, app);
        let editor_state = prepare_editor_state(app);
        let render_outcome = render_editor_workspace(ui, app, &editor_state, workspace_rect);
        paint_preview_overlay(ui, render_outcome.preview_overlay);
        apply_tile_actions(app, render_outcome.actions);
        if render_outcome.any_editor_changed {
            apply_editor_change(app, &editor_state);
        }
        app.refresh_search_state();
        if app.search_progress().searching {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    });
}

fn preferred_workspace_reflow_axis(rect: egui::Rect) -> crate::app::domain::SplitAxis {
    if rect.width() >= rect.height() {
        crate::app::domain::SplitAxis::Vertical
    } else {
        crate::app::domain::SplitAxis::Horizontal
    }
}

struct EditorRenderState {
    active_tab_index: usize,
    pane_tree: PaneNode,
    active_view_id: ViewId,
    leaf_count: usize,
    transaction_snapshot: TransactionSnapshot,
    active_buffer_label: String,
    active_buffer_id: BufferId,
}

struct EditorRenderOutcome {
    actions: Vec<TileAction>,
    any_editor_changed: bool,
    preview_overlay: Option<SplitPreviewOverlay>,
}

fn prepare_editor_state(app: &mut ScratchpadApp) -> EditorRenderState {
    let active_tab_index = app.active_tab_index().min(app.tabs().len() - 1);
    app.tab_manager_mut().active_tab_index = active_tab_index;

    let pane_tree = app.tabs()[active_tab_index].root_pane.clone();
    let active_view_id = app.tabs()[active_tab_index].active_view_id;
    let leaf_count = pane_tree.leaf_count();
    let transaction_snapshot = app.capture_transaction_snapshot();
    let active_buffer_id = app.tabs()[active_tab_index].active_buffer().id;
    let active_buffer_label = app
        .active_buffer_transaction_label()
        .unwrap_or_else(|| "Untitled".to_owned());

    EditorRenderState {
        active_tab_index,
        pane_tree,
        active_view_id,
        leaf_count,
        transaction_snapshot,
        active_buffer_label,
        active_buffer_id,
    }
}

fn render_editor_workspace(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &EditorRenderState,
    workspace_rect: egui::Rect,
) -> EditorRenderOutcome {
    let mut outcome = EditorRenderOutcome {
        actions: Vec::new(),
        any_editor_changed: false,
        preview_overlay: None,
    };

    let mut context = PaneRenderContext {
        app,
        tab_index: state.active_tab_index,
        active_view_id: state.active_view_id,
        leaf_count: state.leaf_count,
        outcome: &mut outcome,
    };

    render_pane_node(
        ui,
        &mut context,
        &state.pane_tree,
        Vec::new(),
        workspace_rect,
    );
    outcome
}

struct PaneRenderContext<'a> {
    app: &'a mut ScratchpadApp,
    tab_index: usize,
    active_view_id: ViewId,
    leaf_count: usize,
    outcome: &'a mut EditorRenderOutcome,
}

fn paint_preview_overlay(ui: &egui::Ui, preview_overlay: Option<SplitPreviewOverlay>) {
    if let Some(preview_overlay) = preview_overlay {
        tile_header::paint_split_preview(ui, &preview_overlay);
    }
}

fn apply_tile_actions(app: &mut ScratchpadApp, actions: Vec<TileAction>) {
    for action in actions {
        match action {
            TileAction::Activate(view_id) => app.activate_view(view_id),
            TileAction::Close(view_id) => app.close_view(view_id),
            TileAction::Promote(view_id) => app.promote_view_to_tab(view_id),
            TileAction::ResizeSplit { path, ratio } => app.resize_split(path, ratio),
            TileAction::Split {
                axis,
                new_view_first,
                ratio,
            } => app.split_active_view_with_placement(axis, new_view_first, ratio),
        }
    }
}

fn handle_editor_zoom(
    ctx: &egui::Context,
    workspace_rect: egui::Rect,
    ui: &egui::Ui,
    app: &mut ScratchpadApp,
) {
    let pointer_over_editor = ui.rect_contains_pointer(workspace_rect);
    let zoom_factor = ctx.input(|input| input.zoom_delta());
    if pointer_over_editor && zoom_factor != 1.0 {
        app.set_font_size(app.font_size() * zoom_factor);
    }
}

fn render_pane_node(
    ui: &mut egui::Ui,
    context: &mut PaneRenderContext<'_>,
    node: &PaneNode,
    path: Vec<PaneBranch>,
    rect: egui::Rect,
) {
    match node {
        PaneNode::Leaf { view_id } => {
            let request = TileRenderRequest {
                tab_index: context.tab_index,
                view_id: *view_id,
                rect,
                is_active: *view_id == context.active_view_id,
                can_close: context.leaf_count > 1,
            };
            let app = &mut *context.app;
            let outcome = &mut *context.outcome;
            let mut tile_state = TileRenderState {
                actions: &mut outcome.actions,
                any_editor_changed: &mut outcome.any_editor_changed,
                preview_overlay: &mut outcome.preview_overlay,
            };
            tile::render_tile(ui, app, request, &mut tile_state)
        }
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let current_path = path;
            let (first_rect, second_rect) = split_rect(rect, *axis, *ratio);
            let mut first_path = current_path.clone();
            first_path.push(PaneBranch::First);
            render_pane_node(ui, context, first, first_path, first_rect);
            let mut second_path = current_path.clone();
            second_path.push(PaneBranch::Second);
            render_pane_node(ui, context, second, second_path, second_rect);
            render_split_divider(
                ui,
                rect,
                *axis,
                *ratio,
                current_path,
                &mut context.outcome.actions,
            );
        }
    }
}

fn apply_editor_change(app: &mut ScratchpadApp, state: &EditorRenderState) {
    app.finalize_active_buffer_text_mutation(
        state.active_tab_index,
        state.active_buffer_id,
        state.active_buffer_label.clone(),
        state.transaction_snapshot.clone(),
    );
}
