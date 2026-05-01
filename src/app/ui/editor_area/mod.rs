pub mod divider;
pub mod tile;

use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PaneBranch, PaneNode, ViewId};
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

        app.refresh_search_state();
        search_replace::show_search_strip(ui, app);
        let workspace_rect = ui.available_rect_before_wrap();
        if workspace_rect.width() <= 0.0 || workspace_rect.height() <= 0.0 {
            return;
        }

        paint_workspace_background(ui, workspace_rect, app.editor_background_color());

        app.workspace_reflow_axis = preferred_workspace_reflow_axis(workspace_rect);
        handle_editor_zoom(ui, workspace_rect, app);
        let editor_state = prepare_editor_state(app);
        let render_outcome = render_editor_workspace(ui, app, &editor_state, workspace_rect);
        finalize_editor_render(ui, app, &editor_state, render_outcome);
        app.refresh_search_state();
        request_search_repaint(ui.ctx(), app.search_progress().searching);
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
    let active_tab_index = app.active_tab_index().min(app.tabs().len() - 1);
    app.tab_manager_mut().active_tab_index = active_tab_index;
    app.ensure_active_tab_slot_selected();

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
    apply_tile_actions(app, render_outcome.actions);
    if render_outcome.any_editor_changed {
        apply_editor_change(app, state);
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

fn request_search_repaint(ctx: &egui::Context, searching: bool) {
    if searching {
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

fn handle_editor_zoom(ui: &egui::Ui, workspace_rect: egui::Rect, app: &mut ScratchpadApp) {
    let pointer_over_editor = ui.rect_contains_pointer(workspace_rect);
    let zoom_factor = ui.ctx().input(|input| input.zoom_delta());
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
        PaneNode::Leaf { view_id } => render_leaf_tile(ui, context, *view_id, rect),
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
    rect: egui::Rect,
) {
    let request = TileRenderRequest {
        tab_index: context.tab_index,
        view_id,
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
    tile::render_tile(ui, app, request, &mut tile_state)
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
    app.finalize_active_buffer_text_mutation(state.active_tab_index);
}

#[cfg(test)]
mod tests {
    use super::{show_editor, split_rect};
    use crate::app::app_state::ScratchpadApp;
    use crate::app::domain::{BufferState, PaneNode, SplitAxis, ViewId};
    use crate::app::fonts;
    use crate::app::services::session_store::SessionStore;
    use eframe::egui;
    use std::collections::HashMap;

    const TEST_SCREEN_RECT: egui::Rect =
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1200.0, 900.0));

    fn test_app() -> ScratchpadApp {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());
        let mut app = ScratchpadApp::with_session_store(session_store);
        app.set_session_persist_on_drop(false);
        app
    }

    fn run_editor_frame(
        ctx: &egui::Context,
        app: &mut ScratchpadApp,
        time: f64,
        events: Vec<egui::Event>,
    ) {
        let _ = fonts::apply_editor_fonts(ctx, app.editor_font());
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(TEST_SCREEN_RECT),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| show_editor(ui, app),
        );
    }

    fn move_event(pos: egui::Pos2) -> egui::Event {
        egui::Event::PointerMoved(pos)
    }

    fn button_event(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn click_pointer(
        ctx: &egui::Context,
        app: &mut ScratchpadApp,
        mut time: f64,
        pos: egui::Pos2,
        button: egui::PointerButton,
    ) -> f64 {
        run_editor_frame(ctx, app, time, vec![move_event(pos)]);
        time += 1.0;
        run_editor_frame(
            ctx,
            app,
            time,
            vec![move_event(pos), button_event(pos, button, true)],
        );
        time += 1.0;
        run_editor_frame(
            ctx,
            app,
            time,
            vec![move_event(pos), button_event(pos, button, false)],
        );
        time + 1.0
    }

    fn settle_frame(ctx: &egui::Context, app: &mut ScratchpadApp, time: f64) -> f64 {
        run_editor_frame(ctx, app, time, Vec::new());
        time + 1.0
    }

    fn view_rects(node: &PaneNode, rect: egui::Rect) -> HashMap<ViewId, egui::Rect> {
        let mut rects = HashMap::new();
        collect_view_rects(node, rect, &mut rects);
        rects
    }

    fn collect_view_rects(
        node: &PaneNode,
        rect: egui::Rect,
        rects: &mut HashMap<ViewId, egui::Rect>,
    ) {
        match node {
            PaneNode::Leaf { view_id } => {
                rects.insert(*view_id, rect);
            }
            PaneNode::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (first_rect, second_rect) = split_rect(rect, *axis, *ratio);
                collect_view_rects(first, first_rect, rects);
                collect_view_rects(second, second_rect, rects);
            }
        }
    }

    fn editor_point(rect: egui::Rect) -> egui::Pos2 {
        let x = (rect.left() + 48.0).clamp(rect.left() + 12.0, rect.right() - 12.0);
        let y = (rect.top() + 72.0).clamp(rect.top() + 12.0, rect.bottom() - 12.0);
        egui::pos2(x, y)
    }

    #[test]
    fn focus_transitions_follow_primary_clicks_across_mixed_split_layout() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
        app.tabs_mut()[0]
            .buffer
            .replace_text("alpha beta gamma\nsecond line\nthird line\n".to_owned());

        let duplicate_view_id = app.tabs_mut()[0]
            .split_active_view(SplitAxis::Vertical)
            .expect("split duplicate view");
        let other_file_view_id = app.tabs_mut()[0]
            .open_buffer_with_balanced_layout(BufferState::new(
                "two.txt".to_owned(),
                "delta epsilon zeta\nother file line\n".to_owned(),
                None,
            ))
            .expect("open second file");
        let original_view_id = app.tabs()[0]
            .views
            .iter()
            .find(|view| view.id != duplicate_view_id && view.id != other_file_view_id)
            .expect("original view")
            .id;
        assert!(app.tabs_mut()[0].activate_view(original_view_id));

        let rects = view_rects(&app.tabs()[0].root_pane, TEST_SCREEN_RECT);
        let duplicate_target =
            editor_point(*rects.get(&duplicate_view_id).expect("duplicate rect"));
        let other_target = editor_point(*rects.get(&other_file_view_id).expect("other rect"));

        let mut time = 0.0;
        time = settle_frame(&ctx, &mut app, time);
        time = click_pointer(
            &ctx,
            &mut app,
            time,
            duplicate_target,
            egui::PointerButton::Primary,
        );
        time = settle_frame(&ctx, &mut app, time);

        assert_eq!(app.tabs()[0].active_view_id, duplicate_view_id);
        assert!(
            app.tabs()[0]
                .view(duplicate_view_id)
                .is_some_and(|view| view.editor_has_focus && view.cursor_range.is_some())
        );
        assert!(
            app.tabs()[0]
                .view(original_view_id)
                .is_some_and(|view| !view.editor_has_focus)
        );

        time = click_pointer(
            &ctx,
            &mut app,
            time,
            other_target,
            egui::PointerButton::Primary,
        );
        let _ = settle_frame(&ctx, &mut app, time);

        assert_eq!(app.tabs()[0].active_view_id, other_file_view_id);
        assert!(
            app.tabs()[0]
                .view(other_file_view_id)
                .is_some_and(|view| view.editor_has_focus && view.cursor_range.is_some())
        );
        assert!(
            app.tabs()[0]
                .view(duplicate_view_id)
                .is_some_and(|view| !view.editor_has_focus)
        );
    }

    // -----------------------------------------------------------------------
    // Phase 6: replacement coverage for the deleted visible-window tests.
    // These exercise the new `ScrollManager`-backed view state instead of the
    // removed `VisibleWindow*` snapshot machinery.
    // -----------------------------------------------------------------------

    #[test]
    fn pixel_offset_round_trips_through_scroll_manager() {
        let mut app = test_app();
        app.tabs_mut()[0].buffer.replace_text(
            (0..150)
                .map(|line| format!("line {line:03}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let active_view_id = app.tabs()[0].active_view_id;
        let view = app.tabs_mut()[0]
            .view_mut(active_view_id)
            .expect("active view");
        // Establish row height so anchor↔pixel conversion is meaningful.
        view.scroll
            .set_metrics(crate::app::ui::scrolling::ViewportMetrics {
                viewport_rect: egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(800.0, 360.0),
                ),
                row_height: 18.0,
                column_width: 8.0,
                visible_rows: 20,
                visible_columns: 80,
            });

        view.set_editor_pixel_offset(egui::vec2(0.0, 100.0 * 18.0));

        assert_eq!(view.editor_pixel_offset(), egui::vec2(0.0, 100.0 * 18.0));
    }

    #[test]
    fn queued_intents_drain_through_scroll_manager() {
        use crate::app::domain::EditorViewState;
        use crate::app::ui::scrolling::{
            ContentExtent, ScrollIntent, ViewportMetrics, naive_anchor_to_row, naive_row_to_anchor,
        };

        let mut view = EditorViewState::new(1, false);
        view.scroll.set_metrics(ViewportMetrics {
            viewport_rect: egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 360.0),
            ),
            row_height: 18.0,
            column_width: 8.0,
            visible_rows: 20,
            visible_columns: 80,
        });
        view.scroll.set_extent(ContentExtent {
            display_rows: 500,
            height: 500.0 * 18.0,
            max_line_width: 800.0,
        });

        view.request_intent(ScrollIntent::Pages(2));
        view.request_intent(ScrollIntent::Lines(3));
        assert_eq!(view.pending_intents.len(), 2);

        // Mirror the renderer's drain step so this test stays isolated from
        // the egui render frame ordering — the production drain helper lives
        // in `tile.rs::drain_pending_scroll_intents`.
        let intents = std::mem::take(&mut view.pending_intents);
        for intent in intents {
            view.scroll
                .apply_intent(intent, naive_anchor_to_row, naive_row_to_anchor);
        }

        assert!(view.pending_intents.is_empty());
        assert_eq!(view.scroll.anchor().logical_line(), Some(2 * 20 + 3));
    }

    #[test]
    fn clear_cursor_reveal_settles_without_panicking_with_scroll_manager() {
        // Smoke test: a sequence of frame settles and explicit cursor-reveal
        // clears must not panic with the new ScrollManager-backed view. This
        // is the minimal end-to-end check that the rebuilt scroll plumbing
        // survives a normal render lifecycle.
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.tabs_mut()[0].buffer.replace_text(
            (0..40)
                .map(|line| format!("line {line:03}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let active_view_id = app.tabs()[0].active_view_id;

        let mut time = 0.0;
        time = settle_frame(&ctx, &mut app, time);
        if let Some(view) = app.tabs_mut()[0].view_mut(active_view_id) {
            view.clear_cursor_reveal();
        }
        let _ = settle_frame(&ctx, &mut app, time);
    }
}
