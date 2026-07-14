pub mod divider;
pub mod tile;

use crate::app::app_state::ScratchpadApp;
use crate::app::app_state::SearchStatus;
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::domain::{PaneBranch, PaneNode, ViewId};
use crate::app::ui::search_replace;
use crate::app::ui::tile_header::{self, SplitPreviewOverlay, TileAction};
use crate::app::ui::widget_ids;
use eframe::egui;
use std::time::Duration;

pub use divider::{render_split_divider, split_rect};
use tile::{TileRenderRequest, TileRenderState};

const SHIFT_SCROLL_FONT_SIZE_PX_PER_POINT: f32 = 48.0;

pub(crate) fn show_editor(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show(ui, |ui| {
        widget_ids::feature_scope(ui, "editor_area", |ui| {
            if app.tab_manager.tabs.as_slice().is_empty() {
                return;
            }

            apply_deferred_tile_actions(ui.ctx(), app);

            crate::app::app_state::search_runtime::refresh_search_state(app);
            search_replace::show_search_strip(ui, app);
            let workspace_rect = ui.available_rect_before_wrap();
            if workspace_rect.width() <= 0.0 || workspace_rect.height() <= 0.0 {
                return;
            }

            paint_workspace_background(
                ui,
                workspace_rect,
                app.state.app_settings.editor_background_color(),
            );

            app.state.workspace_reflow_axis = preferred_workspace_reflow_axis(workspace_rect);
            handle_editor_zoom(ui, workspace_rect, app);
            let editor_state = prepare_editor_state(app);
            let render_outcome = render_editor_workspace(ui, app, &editor_state, workspace_rect);
            finalize_editor_render(ui, app, &editor_state, render_outcome);
            crate::app::app_state::search_runtime::refresh_search_state(app);
            request_search_repaint(
                ui.ctx(),
                matches!(
                    app.state.search_state.progress().status,
                    SearchStatus::Searching { .. }
                ),
            );
        });
    });
}

fn paint_workspace_background(ui: &egui::Ui, workspace_rect: egui::Rect, fill: egui::Color32) {
    ui.painter().rect_filled(workspace_rect, 0.0, fill);
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
}

struct EditorRenderOutcome {
    actions: Vec<TileAction>,
    any_editor_changed: bool,
    preview_overlay: Option<SplitPreviewOverlay>,
}

fn prepare_editor_state(app: &mut ScratchpadApp) -> EditorRenderState {
    let active_tab_index = app
        .tab_manager
        .active_tab_index
        .min(app.tab_manager.tabs.as_slice().len() - 1);
    app.tab_manager
        .set_active_tab_index_clamped(active_tab_index);
    crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);

    let pane_tree = app.tab_manager.tabs.as_slice()[active_tab_index]
        .layout
        .root_pane
        .clone();
    let active_view_id = app.tab_manager.tabs.as_slice()[active_tab_index]
        .layout
        .active_view_id;
    let leaf_count = pane_tree.leaf_count();

    EditorRenderState {
        active_tab_index,
        pane_tree,
        active_view_id,
        leaf_count,
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

struct SplitPane<'a> {
    axis: crate::app::domain::SplitAxis,
    ratio: f32,
    first: &'a PaneNode,
    second: &'a PaneNode,
}

fn paint_preview_overlay(ui: &egui::Ui, preview_overlay: Option<SplitPreviewOverlay>) {
    if let Some(preview_overlay) = preview_overlay {
        tile_header::paint_split_preview(ui, &preview_overlay);
    }
}

fn finalize_editor_render(
    ui: &egui::Ui,
    app: &mut ScratchpadApp,
    state: &EditorRenderState,
    render_outcome: EditorRenderOutcome,
) {
    paint_preview_overlay(ui, render_outcome.preview_overlay);
    defer_tile_actions(ui.ctx(), render_outcome.actions);
    if render_outcome.any_editor_changed {
        apply_editor_change(app, state);
    }
}

fn deferred_tile_actions_id() -> egui::Id {
    widget_ids::ctx_key("editor_area.deferred_tile_actions")
}

fn apply_deferred_tile_actions(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if ctx.current_pass_index() != 0 {
        return;
    }
    let actions = ctx
        .data_mut(|data| data.remove_temp::<Vec<TileAction>>(deferred_tile_actions_id()))
        .unwrap_or_default();
    apply_tile_actions(app, actions);
}

fn defer_tile_actions(ctx: &egui::Context, actions: Vec<TileAction>) {
    if actions.is_empty() {
        return;
    }
    ctx.data_mut(|data| {
        let mut pending = data
            .remove_temp::<Vec<TileAction>>(deferred_tile_actions_id())
            .unwrap_or_default();
        pending.extend(actions);
        data.insert_temp(deferred_tile_actions_id(), pending);
    });
    ctx.request_repaint();
}

fn apply_tile_actions(app: &mut ScratchpadApp, actions: Vec<TileAction>) {
    for action in actions {
        let command = match action {
            TileAction::Activate(view_id) => {
                AppCommand::Workspace(WorkspaceCommand::ActivateView { view_id })
            }
            TileAction::Close(view_id) => {
                AppCommand::Workspace(WorkspaceCommand::CloseView { view_id })
            }
            TileAction::Promote(view_id) => {
                AppCommand::Workspace(WorkspaceCommand::PromoteViewToTab { view_id })
            }
            TileAction::ResizeSplit { path, ratio } => {
                AppCommand::Workspace(WorkspaceCommand::ResizeSplit { path, ratio })
            }
            TileAction::Split {
                axis,
                new_view_first,
                ratio,
            } => AppCommand::Workspace(WorkspaceCommand::SplitActiveView {
                axis,
                new_view_first,
                ratio,
            }),
        };
        crate::app::commands::handle_command(app, command);
    }
}

fn request_search_repaint(ctx: &egui::Context, searching: bool) {
    if searching {
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

fn handle_editor_zoom(ui: &egui::Ui, workspace_rect: egui::Rect, app: &mut ScratchpadApp) {
    let pointer_over_editor = ui.rect_contains_pointer(workspace_rect);
    let zoom_factor = ui.ctx().input(|input| input.zoom_delta());
    if pointer_over_editor && zoom_factor != 1.0 {
        crate::app::app_state::settings_controller::set_font_size(
            app,
            app.state.app_settings.font_size() * zoom_factor,
        );
    }
    let shift_scroll_delta = ui.ctx().input(|input| {
        font_size_delta_from_shift_scroll(input.modifiers, input.smooth_scroll_delta)
    });
    if pointer_over_editor && shift_scroll_delta != 0.0 {
        crate::app::app_state::settings_controller::set_font_size(
            app,
            app.state.app_settings.font_size() + shift_scroll_delta,
        );
    }
}

fn font_size_delta_from_shift_scroll(modifiers: egui::Modifiers, scroll_delta: egui::Vec2) -> f32 {
    if !modifiers.shift || modifiers.ctrl || modifiers.command || modifiers.alt {
        return 0.0;
    }
    if scroll_delta.y.abs() < f32::EPSILON {
        return 0.0;
    }
    scroll_delta.y / SHIFT_SCROLL_FONT_SIZE_PX_PER_POINT
}

fn render_pane_node(
    ui: &mut egui::Ui,
    context: &mut PaneRenderContext<'_>,
    node: &PaneNode,
    path: Vec<PaneBranch>,
    rect: egui::Rect,
) {
    match node {
        PaneNode::Leaf { view_id } => render_leaf_tile(ui, context, *view_id, path, rect),
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => render_split_pane(
            ui,
            context,
            path,
            rect,
            SplitPane {
                axis: *axis,
                ratio: *ratio,
                first,
                second,
            },
        ),
    }
}

fn render_leaf_tile(
    ui: &mut egui::Ui,
    context: &mut PaneRenderContext<'_>,
    view_id: ViewId,
    path: Vec<PaneBranch>,
    rect: egui::Rect,
) {
    let request = TileRenderRequest {
        tab_index: context.tab_index,
        view_id,
        pane_path: path,
        rect,
        is_active: view_id == context.active_view_id,
        can_close: context.leaf_count > 1,
    };
    let app = &mut *context.app;
    let outcome = &mut *context.outcome;
    let mut tile_state = TileRenderState {
        actions: &mut outcome.actions,
        any_editor_changed: &mut outcome.any_editor_changed,
        preview_overlay: &mut outcome.preview_overlay,
    };
    tile::render_tile(ui, app, request, &mut tile_state);
}

fn render_split_pane(
    ui: &mut egui::Ui,
    context: &mut PaneRenderContext<'_>,
    path: Vec<PaneBranch>,
    rect: egui::Rect,
    split: SplitPane<'_>,
) {
    let (first_rect, second_rect) = split_rect(rect, split.axis, split.ratio);
    render_pane_node(
        ui,
        context,
        split.first,
        branched_path(&path, PaneBranch::First),
        first_rect,
    );
    render_pane_node(
        ui,
        context,
        split.second,
        branched_path(&path, PaneBranch::Second),
        second_rect,
    );
    render_split_divider(
        ui,
        rect,
        split.axis,
        split.ratio,
        path,
        &mut context.outcome.actions,
    );
}

fn branched_path(path: &[PaneBranch], branch: PaneBranch) -> Vec<PaneBranch> {
    let mut branched = Vec::with_capacity(path.len() + 1);
    branched.extend_from_slice(path);
    branched.push(branch);
    branched
}

fn apply_editor_change(app: &mut ScratchpadApp, state: &EditorRenderState) {
    crate::app::app_state::workspace::mutation::finalize_active_buffer_text_mutation(
        app,
        state.active_tab_index,
    );
}

#[cfg(test)]
mod tests {
    use super::{egui, font_size_delta_from_shift_scroll};

    #[test]
    fn shift_scroll_maps_to_font_size_delta() {
        let delta =
            font_size_delta_from_shift_scroll(egui::Modifiers::SHIFT, egui::vec2(0.0, 48.0));

        assert_eq!(delta, 1.0);
    }

    #[test]
    fn plain_scroll_does_not_change_font_size() {
        let delta = font_size_delta_from_shift_scroll(egui::Modifiers::NONE, egui::vec2(0.0, 48.0));

        assert_eq!(delta, 0.0);
    }

    #[test]
    fn ctrl_shift_scroll_stays_reserved_for_zoom_handling() {
        let delta = font_size_delta_from_shift_scroll(
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::vec2(0.0, 48.0),
        );

        assert_eq!(delta, 0.0);
    }
}
