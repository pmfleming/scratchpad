pub mod divider;
pub mod tile;

use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PaneBranch, PaneNode, ViewId};
use crate::app::logging::LogLevel;
use crate::app::ui::tile_header::{self, SplitPreviewOverlay, TileAction};
use eframe::egui;

pub use divider::{render_split_divider, split_rect};
pub use tile::render_tile;

pub(crate) fn show_editor(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        if app.tabs().is_empty() {
            return;
        }

        let ctx = ui.ctx().clone();
        handle_editor_zoom(&ctx, ui, app);
        let editor_state = prepare_editor_state(app);
        let render_outcome = render_editor_workspace(ui, app, &editor_state);
        paint_preview_overlay(ui, render_outcome.preview_overlay);
        apply_tile_actions(app, render_outcome.actions);
        if render_outcome.any_editor_changed {
            apply_editor_change(app, editor_state.active_tab_index);
        }
    });
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
    let active_tab_index = app.active_tab_index().min(app.tabs().len() - 1);
    app.tab_manager_mut().active_tab_index = active_tab_index;

    let pane_tree = app.tabs()[active_tab_index].root_pane.clone();
    let active_view_id = app.tabs()[active_tab_index].active_view_id;
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
) -> EditorRenderOutcome {
    let mut outcome = EditorRenderOutcome {
        actions: Vec::new(),
        any_editor_changed: false,
        preview_overlay: None,
    };

    render_pane_node(
        ui,
        app,
        state.active_tab_index,
        &state.pane_tree,
        Vec::new(),
        ui.max_rect(),
        state.active_view_id,
        state.leaf_count,
        &mut outcome.actions,
        &mut outcome.any_editor_changed,
        &mut outcome.preview_overlay,
    );

    outcome
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

fn handle_editor_zoom(ctx: &egui::Context, ui: &egui::Ui, app: &mut ScratchpadApp) {
    let panel_rect = ui.max_rect();
    let pointer_over_editor = ui.rect_contains_pointer(panel_rect);
    let zoom_factor = ctx.input(|input| input.zoom_delta());
    if pointer_over_editor && zoom_factor != 1.0 {
        let previous_font_size = app.font_size;
        app.font_size = (app.font_size * zoom_factor).clamp(8.0, 72.0);
        app.mark_session_dirty();
        app.log_event(
            LogLevel::Info,
            format!(
                "Adjusted editor zoom from {:.2} to {:.2} (zoom factor {:.3})",
                previous_font_size, app.font_size, zoom_factor
            ),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_pane_node(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    node: &PaneNode,
    path: Vec<PaneBranch>,
    rect: egui::Rect,
    active_view_id: ViewId,
    leaf_count: usize,
    actions: &mut Vec<TileAction>,
    any_editor_changed: &mut bool,
    preview_overlay: &mut Option<SplitPreviewOverlay>,
) {
    match node {
        PaneNode::Leaf { view_id } => render_tile(
            ui,
            app,
            tab_index,
            *view_id,
            rect,
            *view_id == active_view_id,
            leaf_count > 1,
            actions,
            any_editor_changed,
            preview_overlay,
        ),
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
            render_pane_node(
                ui,
                app,
                tab_index,
                first,
                first_path,
                first_rect,
                active_view_id,
                leaf_count,
                actions,
                any_editor_changed,
                preview_overlay,
            );
            let mut second_path = current_path.clone();
            second_path.push(PaneBranch::Second);
            render_pane_node(
                ui,
                app,
                tab_index,
                second,
                second_path,
                second_rect,
                active_view_id,
                leaf_count,
                actions,
                any_editor_changed,
                preview_overlay,
            );
            render_split_divider(ui, rect, *axis, *ratio, current_path, actions);
        }
    }
}

fn apply_editor_change(app: &mut ScratchpadApp, active_tab_index: usize) {
    let tab = &mut app.tabs_mut()[active_tab_index];
    let previous_dirty = tab.buffer.is_dirty;
    let previous_artifact_status = tab.buffer.artifact_summary.status_text();
    tab.buffer.refresh_text_metadata();
    let has_control_chars = tab.buffer.artifact_summary.has_control_chars();
    for view in &mut tab.views {
        if !has_control_chars {
            view.show_control_chars = false;
        }
    }
    tab.buffer.is_dirty = true;
    let tab_name = tab.buffer.name.clone();
    let current_artifact_status = tab.buffer.artifact_summary.status_text();
    let line_count = tab.buffer.line_count;
    let warning_message = tab
        .buffer
        .artifact_summary
        .status_text()
        .map(|message| format!("Formatting artifacts remain: {message}"));
    if let Some(message) = warning_message {
        app.set_warning_status(message);
    } else {
        app.clear_status_message();
    }
    app.mark_session_dirty();

    if !previous_dirty {
        app.log_event(
            LogLevel::Info,
            format!(
                "Buffer '{tab_name}' became dirty after edit (line_count={line_count}, artifact_status={})",
                current_artifact_status
                    .clone()
                    .unwrap_or_else(|| "none".to_owned())
            ),
        );
    }

    if previous_artifact_status != current_artifact_status {
        app.log_event(
            LogLevel::Info,
            format!(
                "Artifact status changed for '{tab_name}' from {} to {}",
                previous_artifact_status.unwrap_or_else(|| "none".to_owned()),
                current_artifact_status.unwrap_or_else(|| "none".to_owned())
            ),
        );
    }
}
