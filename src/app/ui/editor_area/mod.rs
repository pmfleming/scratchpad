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
    transaction_snapshot: Option<TransactionSnapshot>,
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
    let active_buffer_id = app.tabs()[active_tab_index].active_buffer().id;

    // Skip the expensive deep-clone when the next edit will coalesce into
    // an already-pending text transaction (the snapshot would be dropped unused).
    let transaction_snapshot = if app.has_coalescable_text_transaction(active_buffer_id) {
        None
    } else {
        Some(app.capture_transaction_snapshot())
    };

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
    let snapshot = state
        .transaction_snapshot
        .clone()
        .unwrap_or_else(|| app.capture_transaction_snapshot());
    app.finalize_active_buffer_text_mutation(
        state.active_tab_index,
        state.active_buffer_id,
        state.active_buffer_label.clone(),
        snapshot,
    );
}

#[cfg(test)]
mod tests {
    use super::{show_editor, split_rect, tile};
    use crate::app::app_state::ScratchpadApp;
    use crate::app::domain::{BufferState, PaneNode, SplitAxis, ViewId};
    use crate::app::fonts;
    use crate::app::services::session_store::SessionStore;
    use crate::app::ui::editor_content::native_editor::{
        VisibleWindowDebugSnapshot, load_visible_window_debug_snapshot,
    };
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

    fn run_editor_frame_with_rect(
        ctx: &egui::Context,
        app: &mut ScratchpadApp,
        time: f64,
        events: Vec<egui::Event>,
        screen_rect: egui::Rect,
    ) {
        let _ = fonts::apply_editor_fonts(ctx, app.editor_font());
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
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

    fn mouse_wheel_event(delta: egui::Vec2) -> egui::Event {
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta,
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn key_event(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
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

    fn click_pointer_with_rect(
        ctx: &egui::Context,
        app: &mut ScratchpadApp,
        mut time: f64,
        pos: egui::Pos2,
        button: egui::PointerButton,
        screen_rect: egui::Rect,
    ) -> f64 {
        run_editor_frame_with_rect(ctx, app, time, vec![move_event(pos)], screen_rect);
        time += 1.0;
        run_editor_frame_with_rect(
            ctx,
            app,
            time,
            vec![move_event(pos), button_event(pos, button, true)],
            screen_rect,
        );
        time += 1.0;
        run_editor_frame_with_rect(
            ctx,
            app,
            time,
            vec![move_event(pos), button_event(pos, button, false)],
            screen_rect,
        );
        time + 1.0
    }

    fn settle_frame(ctx: &egui::Context, app: &mut ScratchpadApp, time: f64) -> f64 {
        run_editor_frame(ctx, app, time, Vec::new());
        time + 1.0
    }

    fn settle_frame_with_rect(
        ctx: &egui::Context,
        app: &mut ScratchpadApp,
        time: f64,
        screen_rect: egui::Rect,
    ) -> f64 {
        run_editor_frame_with_rect(ctx, app, time, Vec::new(), screen_rect);
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

    fn active_scroll_area_state(
        ctx: &egui::Context,
        app: &ScratchpadApp,
    ) -> Option<tile::EditorScrollAreaDebugState> {
        let tab = &app.tabs()[0];
        tile::load_editor_scroll_debug_state(ctx, tab.active_view_id)
    }

    fn active_visible_window_debug(
        ctx: &egui::Context,
        app: &ScratchpadApp,
    ) -> Option<VisibleWindowDebugSnapshot> {
        let tab = &app.tabs()[0];
        load_visible_window_debug_snapshot(ctx, tab.active_view_id)
    }

    fn visible_window_click_point(
        snapshot: &VisibleWindowDebugSnapshot,
        target_line: usize,
    ) -> egui::Pos2 {
        assert!(
            !snapshot.line_range.is_empty(),
            "expected visible lines in snapshot: {snapshot:?}"
        );
        let clamped_line =
            target_line.clamp(snapshot.line_range.start, snapshot.line_range.end - 1);
        let row_index = clamped_line - snapshot.line_range.start;
        let x = (snapshot.rect.left() + 48.0)
            .clamp(snapshot.rect.left() + 4.0, snapshot.rect.right() - 4.0);
        let y = (snapshot.rect.top() + (row_index as f32 + 0.5) * snapshot.row_height)
            .clamp(snapshot.rect.top() + 1.0, snapshot.rect.bottom() - 1.0);
        egui::pos2(x, y)
    }

    fn line_index_for_active_cursor(app: &ScratchpadApp) -> Option<usize> {
        let tab = &app.tabs()[0];
        let view = tab.view(tab.active_view_id)?;
        let cursor = view.pending_cursor_range.or(view.cursor_range)?;
        Some(
            tab.buffer
                .document()
                .piece_tree()
                .char_position(cursor.primary.index)
                .line_index,
        )
    }

    fn active_view_visible_lines(app: &ScratchpadApp) -> Option<std::ops::Range<usize>> {
        let tab = &app.tabs()[0];
        let view = tab.view(tab.active_view_id)?;
        Some(view.latest_layout.as_ref()?.visible_line_range())
    }

    #[test]
    #[ignore = "asserts old visible-window scroll behavior; replaced in Phase 6"]
    fn visible_window_release_snapshot_tracks_widget_rect_and_pointer_path() {
        let screen_rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(900.0, 320.0));
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.set_font_size(32.0);
        app.set_word_wrap(false);
        let long_line = "x".repeat(20_000);
        app.tabs_mut()[0].buffer.replace_text(
            (0..300)
                .map(|line| format!("line {line:03} {long_line}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let active_view_id = app.tabs()[0].active_view_id;

        let mut time = 0.0;
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);
        {
            let view = app.tabs_mut()[0]
                .view_mut(active_view_id)
                .expect("active view");
            view.editor_has_focus = false;
            view.cursor_range = None;
            view.pending_cursor_range = None;
            view.clear_cursor_reveal();
        }
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);

        let visible_window =
            active_visible_window_debug(&ctx, &app).expect("visible window snapshot after settle");
        let target_line = visible_window.line_range.start.saturating_add(1);
        let target = visible_window_click_point(&visible_window, target_line);

        let _ = click_pointer_with_rect(
            &ctx,
            &mut app,
            time,
            target,
            egui::PointerButton::Primary,
            screen_rect,
        );

        let release_snapshot =
            active_visible_window_debug(&ctx, &app).expect("visible window release snapshot");
        assert!(
            release_snapshot.primary_released,
            "expected release-frame snapshot, got {release_snapshot:?}"
        );
        assert!(
            release_snapshot.rect.contains(target),
            "expected click target inside release rect, target={target:?}, snapshot={release_snapshot:?}"
        );
        assert_eq!(release_snapshot.latest_pointer_pos, Some(target));
    }

    #[test]
    #[ignore = "asserts old visible-window scroll behavior; replaced in Phase 6"]
    fn scrolled_visible_window_click_places_cursor_in_scrolled_document_region() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.set_font_size(14.0);
        app.set_word_wrap(false);
        app.tabs_mut()[0].buffer.replace_text(
            (0..150)
                .map(|line| format!("line {line:03}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let active_view_id = app.tabs()[0].active_view_id;

        let mut time = 0.0;
        time = settle_frame(&ctx, &mut app, time);
        {
            let view = app.tabs_mut()[0]
                .view_mut(active_view_id)
                .expect("active view");
            view.set_editor_pixel_offset(egui::vec2(0.0, 100.0 * 18.0));
            view.editor_has_focus = false;
            view.cursor_range = None;
            view.pending_cursor_range = None;
            view.clear_cursor_reveal();
        }
        time = settle_frame(&ctx, &mut app, time);

        let settled_offset =
            active_scroll_area_state(&ctx, &app).expect("scroll area state after settle");
        let visible_lines = active_view_visible_lines(&app).expect("visible lines after settle");
        assert!(
            settled_offset.offset.y >= 95.0 * 18.0,
            "expected scrolled offset near line 100, got {settled_offset:?}"
        );
        assert!(
            visible_lines.start >= 95,
            "expected visible window near line 100 before click, got {visible_lines:?}"
        );
        let visible_window =
            active_visible_window_debug(&ctx, &app).expect("visible window snapshot after settle");
        assert!(
            visible_window.line_range.contains(&100),
            "expected release target line in visible window, got {visible_window:?}"
        );

        let target = visible_window_click_point(&visible_window, 100);
        time = click_pointer_with_rect(
            &ctx,
            &mut app,
            time,
            target,
            egui::PointerButton::Primary,
            TEST_SCREEN_RECT,
        );
        let release_snapshot =
            active_visible_window_debug(&ctx, &app).expect("visible window release snapshot");
        assert!(
            release_snapshot.primary_released,
            "expected release-frame snapshot, got {release_snapshot:?}"
        );
        assert!(
            release_snapshot.rect.contains(target),
            "expected click target inside release rect, target={target:?}, snapshot={release_snapshot:?}"
        );
        let _ = settle_frame(&ctx, &mut app, time);

        let clicked_line = line_index_for_active_cursor(&app).unwrap_or_else(|| {
            panic!("cursor after click; release snapshot: {release_snapshot:?}")
        });
        assert!(
            clicked_line >= 95,
            "expected visible-window click after scrolling to land near line 100, got {clicked_line}"
        );
    }

    #[test]
    #[ignore = "asserts old visible-window scroll behavior; replaced in Phase 6"]
    fn scrolled_wide_visible_window_click_places_cursor_in_scrolled_document_region() {
        let screen_rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(900.0, 320.0));
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.set_font_size(32.0);
        app.set_word_wrap(false);
        let long_line = "x".repeat(20_000);
        app.tabs_mut()[0].buffer.replace_text(
            (0..300)
                .map(|line| format!("line {line:03} {long_line}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let active_view_id = app.tabs()[0].active_view_id;

        let mut time = 0.0;
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);
        let row_height = app.tabs()[0]
            .view(active_view_id)
            .and_then(|view| view.latest_layout.as_ref())
            .and_then(|layout| Some(layout.row_top(1)? - layout.row_top(0)?))
            .expect("layout row height");
        {
            let view = app.tabs_mut()[0]
                .view_mut(active_view_id)
                .expect("active view");
            view.set_editor_pixel_offset(egui::vec2(0.0, 100.0 * row_height));
            view.editor_has_focus = false;
            view.cursor_range = None;
            view.pending_cursor_range = None;
            view.clear_cursor_reveal();
        }
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);

        let settled_offset =
            active_scroll_area_state(&ctx, &app).expect("scroll area state after settle");
        let visible_lines = active_view_visible_lines(&app).expect("visible lines after settle");
        assert!(
            settled_offset.offset.y >= 95.0 * row_height,
            "expected scrolled offset near line 100, got {settled_offset:?}"
        );
        assert!(
            visible_lines.start >= 95,
            "expected wide visible window near line 100 before click, got {visible_lines:?}"
        );
        let visible_window =
            active_visible_window_debug(&ctx, &app).expect("visible window snapshot after settle");
        assert!(
            visible_window.line_range.contains(&100),
            "expected release target line in visible window, got {visible_window:?}"
        );

        let target = visible_window_click_point(&visible_window, 100);
        time = click_pointer_with_rect(
            &ctx,
            &mut app,
            time,
            target,
            egui::PointerButton::Primary,
            screen_rect,
        );
        let release_snapshot =
            active_visible_window_debug(&ctx, &app).expect("visible window release snapshot");
        assert!(
            release_snapshot.primary_released,
            "expected release-frame snapshot, got {release_snapshot:?}"
        );
        assert!(
            release_snapshot.rect.contains(target),
            "expected click target inside release rect, target={target:?}, snapshot={release_snapshot:?}"
        );
        let _ = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);

        let clicked_line = line_index_for_active_cursor(&app).unwrap_or_else(|| {
            panic!("cursor after click; release snapshot: {release_snapshot:?}")
        });
        assert!(
            clicked_line >= 95,
            "expected wide visible-window click after scrolling to land near line 100, got {clicked_line}"
        );
    }

    #[test]
    #[ignore = "asserts old visible-window scroll behavior; replaced in Phase 6"]
    fn focused_wheel_scroll_updates_visible_window_and_click_mapping() {
        let screen_rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(900.0, 320.0));
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.set_font_size(32.0);
        app.set_word_wrap(false);
        let long_line = "x".repeat(20_000);
        app.tabs_mut()[0].buffer.replace_text(
            (0..300)
                .map(|line| format!("line {line:03} {long_line}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let active_view_id = app.tabs()[0].active_view_id;

        let mut time = 0.0;
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);
        let row_height = app.tabs()[0]
            .view(active_view_id)
            .and_then(|view| view.latest_layout.as_ref())
            .and_then(|layout| Some(layout.row_top(1)? - layout.row_top(0)?))
            .expect("layout row height");

        let focus_target = editor_point(screen_rect);
        time = click_pointer_with_rect(
            &ctx,
            &mut app,
            time,
            focus_target,
            egui::PointerButton::Primary,
            screen_rect,
        );
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);
        assert!(
            app.tabs()[0]
                .view(active_view_id)
                .is_some_and(|view| view.editor_has_focus && view.cursor_range.is_some()),
            "expected editor to hold focus before wheel scrolling"
        );

        let editor_target = editor_point(screen_rect);
        run_editor_frame_with_rect(
            &ctx,
            &mut app,
            time,
            vec![move_event(editor_target), mouse_wheel_event(egui::vec2(0.0, -100.0 * row_height))],
            screen_rect,
        );
        time += 1.0;

        let visible_lines = active_view_visible_lines(&app).expect("visible lines after wheel");
        let viewport_rect = active_scroll_area_state(&ctx, &app)
            .expect("scroll area state after wheel")
            .inner_rect;
        assert!(
            visible_lines.start > 30,
            "expected focused visible window to move beyond the top-of-file region after wheel, got {visible_lines:?}"
        );

        let visible_window =
            active_visible_window_debug(&ctx, &app).expect("visible window snapshot after wheel");
        let target_line = visible_window.line_range.start.saturating_add(2);
        assert!(
            target_line < visible_window.line_range.end,
            "expected a clickable target inside the wheel-scrolled focused window, got {visible_window:?}"
        );

        let target = egui::pos2(
            (viewport_rect.left() + 48.0).clamp(viewport_rect.left() + 4.0, viewport_rect.right() - 4.0),
            (viewport_rect.top() + (target_line - visible_lines.start) as f32 * row_height + row_height * 0.5)
                .clamp(viewport_rect.top() + 1.0, viewport_rect.bottom() - 1.0),
        );
        time = click_pointer_with_rect(
            &ctx,
            &mut app,
            time,
            target,
            egui::PointerButton::Primary,
            screen_rect,
        );
        let _ = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);

        let clicked_line = line_index_for_active_cursor(&app).expect("cursor after focused click");
        assert!(
            clicked_line >= visible_lines.start,
            "expected focused wheel-scrolled click to land in the current visible region {visible_lines:?}, got {clicked_line}"
        );
    }

    #[test]
    #[ignore = "asserts old visible-window scroll behavior; replaced in Phase 6"]
    fn focused_arrow_down_reveals_cursor_after_wheel_scroll() {
        let screen_rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(900.0, 320.0));
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.set_font_size(32.0);
        app.set_word_wrap(false);
        let long_line = "x".repeat(20_000);
        app.tabs_mut()[0].buffer.replace_text(
            (0..300)
                .map(|line| format!("line {line:03} {long_line}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let active_view_id = app.tabs()[0].active_view_id;

        let mut time = 0.0;
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);
        let row_height = app.tabs()[0]
            .view(active_view_id)
            .and_then(|view| view.latest_layout.as_ref())
            .and_then(|layout| Some(layout.row_top(1)? - layout.row_top(0)?))
            .expect("layout row height");

        time = click_pointer_with_rect(
            &ctx,
            &mut app,
            time,
            editor_point(screen_rect),
            egui::PointerButton::Primary,
            screen_rect,
        );
        time = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);

        run_editor_frame_with_rect(
            &ctx,
            &mut app,
            time,
            vec![
                move_event(editor_point(screen_rect)),
                mouse_wheel_event(egui::vec2(0.0, -100.0 * row_height)),
            ],
            screen_rect,
        );
        time += 1.0;

        let scrolled_lines = active_view_visible_lines(&app).expect("visible lines after wheel");
        assert!(
            scrolled_lines.start > 30,
            "expected wheel scroll to move away from the initial cursor region, got {scrolled_lines:?}"
        );

        run_editor_frame_with_rect(
            &ctx,
            &mut app,
            time,
            vec![key_event(egui::Key::ArrowDown)],
            screen_rect,
        );
        time += 1.0;
        let _ = settle_frame_with_rect(&ctx, &mut app, time, screen_rect);

        let cursor_line = line_index_for_active_cursor(&app).expect("cursor after ArrowDown");
        let visible_lines = active_view_visible_lines(&app).expect("visible lines after ArrowDown");
        assert_eq!(cursor_line, 2, "expected ArrowDown to move the cursor one line");
        assert!(
            visible_lines.contains(&cursor_line),
            "expected ArrowDown to reveal the off-screen cursor, cursor_line={cursor_line}, visible_lines={visible_lines:?}"
        );
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
}
