mod context_menu;

use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{ViewId, WorkspaceTab};
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme::*;
use crate::app::ui::autoscroll::{AutoScrollAxis, AutoScrollConfig, edge_auto_scroll_delta};
use crate::app::ui::callout;
use crate::app::ui::editor_content::{
    self, EditorContentOutcome, EditorContentStyle, EditorHighlightStyle, TextEditOptions,
};
use crate::app::ui::scrolling;
use crate::app::ui::scrolling::{DisplaySnapshot, ScrollAnchor};
use crate::app::ui::tab_drag;
use crate::app::ui::tile_header::{
    self, SplitPreviewOverlay, TileAction, TileHeaderRequest, TileHeaderState,
};
use crate::app::ui::widget_ids;
use eframe::egui;

const EDITOR_SELECTION_AUTOSCROLL_EDGE_ROWS: f32 = 2.0;
const EDITOR_SELECTION_AUTOSCROLL_MAX_STEP: f32 = 10.0;
const EDITOR_SELECTION_AUTOSCROLL_CROSS_AXIS_MARGIN: f32 = 12.0;

fn editor_selection_autoscroll_config(row_height: f32) -> AutoScrollConfig {
    AutoScrollConfig {
        edge_extent: (EDITOR_SELECTION_AUTOSCROLL_EDGE_ROWS * row_height).max(1.0),
        max_step: EDITOR_SELECTION_AUTOSCROLL_MAX_STEP,
        cross_axis_margin: EDITOR_SELECTION_AUTOSCROLL_CROSS_AXIS_MARGIN,
    }
}

struct TileBodyOutcome {
    changed: bool,
    focused: bool,
    interaction_response: Option<egui::Response>,
}

#[derive(Clone, Copy)]
pub(super) struct TileRenderRequest {
    pub(super) tab_index: usize,
    pub(super) view_id: ViewId,
    pub(super) rect: egui::Rect,
    pub(super) is_active: bool,
    pub(super) can_close: bool,
}

pub(super) struct TileRenderState<'a> {
    pub(super) actions: &'a mut Vec<TileAction>,
    pub(super) any_editor_changed: &'a mut bool,
    pub(super) preview_overlay: &'a mut Option<SplitPreviewOverlay>,
}

struct TileScrollRequest<'a> {
    view_id: ViewId,
    scroll_bar_visibility: egui::scroll_area::ScrollBarVisibility,
    content_style: EditorContentStyle<'a>,
}

pub(super) fn render_tile(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
    state: &mut TileRenderState<'_>,
) {
    ui.scope_builder(tile_ui_builder(request.rect), |ui| {
        let tile_response = handle_tile_click(ui, app, request, state.actions);
        paint_tile_frame(
            ui,
            request.rect,
            request.is_active,
            app.editor_background_color(),
        );

        let body_outcome = render_tile_body(ui, app, request);
        let context_menu_response = body_outcome
            .interaction_response
            .as_ref()
            .unwrap_or(&tile_response);
        *state.any_editor_changed |= body_outcome.changed;
        apply_tile_body_focus(
            body_outcome.focused,
            request.is_active,
            request.view_id,
            state.actions,
        );
        tile_header::render_tile_header(
            ui,
            app,
            TileHeaderRequest {
                tab_index: request.tab_index,
                view_id: request.view_id,
                tile_rect: request.rect,
                can_close: request.can_close,
            },
            &mut TileHeaderState {
                actions: state.actions,
                preview_overlay: state.preview_overlay,
            },
        );
        context_menu::attach_editor_context_menu(
            context_menu_response,
            ui,
            app,
            request,
            state.actions,
        );
    });
}

fn render_tile_body(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
) -> TileBodyOutcome {
    ui.scope_builder(tile_ui_builder(request.rect), |ui| {
        let request_focus = app.should_focus_view(request.view_id);
        let editor_font_id = editor_font_id(app.font_size());
        let content_style =
            editor_content_style(app, request.is_active, request_focus, &editor_font_id);
        let tab = &mut app.tabs_mut()[request.tab_index];
        let Some(_buffer) = tab.buffer_for_view(request.view_id) else {
            return TileBodyOutcome {
                changed: false,
                focused: false,
                interaction_response: None,
            };
        };
        let previous_snapshot = take_previous_snapshot(tab, request.view_id);
        let outcome = show_editor_scroll_area(
            ui,
            tab,
            TileScrollRequest {
                view_id: request.view_id,
                scroll_bar_visibility: editor_scroll_bar_visibility(ui.ctx()),
                content_style: EditorContentStyle {
                    previous_snapshot: previous_snapshot.as_ref(),
                    ..content_style
                },
            },
        );
        restore_previous_snapshot_if_needed(tab, request.view_id, previous_snapshot);
        apply_tile_focus_request(
            app,
            request.view_id,
            request_focus,
            outcome.request_editor_focus,
        );

        TileBodyOutcome {
            changed: outcome.changed,
            focused: outcome.focused,
            interaction_response: outcome.interaction_response,
        }
    })
    .inner
}

fn tile_ui_builder(rect: egui::Rect) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .max_rect(rect)
        .layout(egui::Layout::top_down(egui::Align::Min))
}

fn handle_tile_click(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
    actions: &mut Vec<TileAction>,
) -> egui::Response {
    let tile_response = ui.interact(
        request.rect,
        widget_ids::local(ui, ("tile", request.tab_index, request.view_id)),
        egui::Sense::click(),
    );
    context_menu::activate_inactive_tile_on_secondary_click(app, &tile_response, request);
    if tile_response.clicked() {
        actions.push(TileAction::Activate(request.view_id));
    }
    tile_response
}

fn paint_tile_frame(
    ui: &egui::Ui,
    rect: egui::Rect,
    is_active: bool,
    background_color: egui::Color32,
) {
    let bg = if is_active {
        header_bg(ui)
    } else {
        background_color
    };
    let border_color = border(ui).gamma_multiply(0.0);

    ui.painter().rect_filled(rect, 4.0, bg);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Inside,
    );
}

fn apply_tile_body_focus(
    body_focused: bool,
    is_active: bool,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) {
    if body_focused && !is_active {
        actions.push(TileAction::Activate(view_id));
    }
}

fn editor_font_id(font_size: f32) -> egui::FontId {
    egui::FontId::new(font_size, egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()))
}

fn editor_content_style<'a>(
    app: &ScratchpadApp,
    _is_active: bool,
    request_focus: bool,
    editor_font_id: &'a egui::FontId,
) -> EditorContentStyle<'a> {
    EditorContentStyle {
        editor_gutter: app.editor_gutter(),
        viewport: None,
        previous_snapshot: None,
        text_edit: TextEditOptions::new(
            request_focus,
            app.word_wrap(),
            editor_font_id,
            app.editor_text_color(),
            EditorHighlightStyle::new(
                app.editor_text_highlight_color(),
                app.editor_text_highlight_text_color(),
            ),
        ),
        background_color: app.editor_background_color(),
    }
}

fn apply_tile_focus_request(
    app: &mut ScratchpadApp,
    view_id: ViewId,
    request_focus: bool,
    request_editor_focus: bool,
) {
    if request_focus {
        app.consume_focus_request(view_id);
    } else if request_editor_focus {
        app.request_focus_for_view(view_id);
    }
}

fn editor_scroll_bar_visibility(ctx: &egui::Context) -> egui::scroll_area::ScrollBarVisibility {
    if tab_drag::is_drag_active_for_context(ctx) {
        egui::scroll_area::ScrollBarVisibility::AlwaysHidden
    } else {
        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
    }
}

fn take_previous_snapshot(tab: &mut WorkspaceTab, view_id: ViewId) -> Option<DisplaySnapshot> {
    let current_revision = tab
        .buffer_for_view(view_id)
        .map(|buffer| buffer.document_revision());
    tab.view_mut(view_id).and_then(|view| {
        if view.latest_display_snapshot_revision == current_revision {
            view.latest_display_snapshot.take()
        } else {
            view.latest_display_snapshot = None;
            view.latest_display_snapshot_revision = None;
            None
        }
    })
}

fn editor_scroll_id(view_id: ViewId) -> egui::Id {
    egui::Id::new(("editor_scroll", view_id))
}

fn show_editor_scroll_area(
    ui: &mut egui::Ui,
    tab: &mut WorkspaceTab,
    request: TileScrollRequest<'_>,
) -> EditorContentOutcome {
    let frame = prepare_editor_scroll_frame(ui, tab, request.view_id, &request.content_style);
    let output = scrolling::ScrollArea::new(frame.scroll_id)
        .source(local_scroll_source(request.scroll_bar_visibility))
        .scrollbar_x(scrollbar_policy_from_egui(request.scroll_bar_visibility))
        .scrollbar_y(scrollbar_policy_from_egui(request.scroll_bar_visibility))
        .min_content_size(egui::vec2(0.0, frame.virtual_content_height))
        .show_viewport(ui, |ui, _offset, viewport| {
            let mut content_style = request.content_style;
            content_style.viewport = Some(viewport);
            tab.buffer_and_view_mut(request.view_id)
                .map(|(buffer, view)| {
                    editor_content::render_editor_content(
                        ui,
                        buffer,
                        view,
                        request.view_id,
                        content_style,
                    )
                })
                .unwrap_or_else(missing_editor_content_outcome)
        });

    let content_size =
        editor_scroll_content_size(output.content_size, frame.virtual_content_height);
    apply_selection_edge_autoscroll_intent(
        ui,
        tab,
        request.view_id,
        output.inner.interaction_response.as_ref(),
        output.inner_rect,
        frame.row_height,
    );
    let drag_requested_scroll_offset = requested_scroll_offset_for_pointer_drag(
        ui,
        frame.scroll_offset,
        output.inner.interaction_response.as_ref(),
        content_size,
        output.inner_rect.size(),
        output.inner_rect,
    );
    finish_editor_scroll_frame(
        tab,
        request.view_id,
        &frame,
        &output,
        content_size,
        drag_requested_scroll_offset,
    );
    output.inner
}

struct EditorScrollFrame<'a> {
    scroll_id: egui::Id,
    previous_snapshot: Option<&'a DisplaySnapshot>,
    scroll_offset: egui::Vec2,
    wheel_requested_scroll_offset: Option<egui::Vec2>,
    row_height: f32,
    virtual_content_height: f32,
}

fn prepare_editor_scroll_frame<'a>(
    ui: &egui::Ui,
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    content_style: &EditorContentStyle<'a>,
) -> EditorScrollFrame<'a> {
    let scroll_id = editor_scroll_id(view_id);
    let previous_snapshot = content_style.previous_snapshot;
    recover_unresolved_piece_anchor(ui, tab, view_id, scroll_id, previous_snapshot);
    if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
        drain_pending_scroll_intents(view, buffer, previous_snapshot);
    }
    let scroll_offset = resolved_scroll_offset_for_view(tab, view_id, previous_snapshot);
    let wheel_requested_scroll_offset =
        requested_scroll_offset_for_pointer_wheel(ui, scroll_offset);
    if wheel_requested_scroll_offset.is_some()
        && let Some(view) = tab.view_mut(view_id)
    {
        view.clear_cursor_reveal();
    }
    sync_local_scroll_state(
        ui,
        scroll_id,
        wheel_requested_scroll_offset.unwrap_or(scroll_offset),
    );
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(content_style.text_edit.editor_font_id));
    let virtual_content_height = virtual_editor_content_height(
        tab,
        view_id,
        row_height.max(content_style.text_edit.editor_font_id.size),
    );
    EditorScrollFrame {
        scroll_id,
        previous_snapshot,
        scroll_offset,
        wheel_requested_scroll_offset,
        row_height,
        virtual_content_height,
    }
}

fn resolved_scroll_offset_for_view(
    tab: &WorkspaceTab,
    view_id: ViewId,
    previous_snapshot: Option<&DisplaySnapshot>,
) -> egui::Vec2 {
    tab.view(view_id)
        .and_then(|view| {
            tab.buffer_for_view(view_id)
                .map(|buffer| editor_pixel_offset_resolved(view, buffer, previous_snapshot))
        })
        .unwrap_or_default()
}

fn recover_unresolved_piece_anchor(
    ui: &egui::Ui,
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    scroll_id: egui::Id,
    snapshot_fallback: Option<&DisplaySnapshot>,
) {
    let preserved_offset = scrolling::ScrollState::load(ui, scroll_id).offset;
    let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) else {
        return;
    };
    let ScrollAnchor::Piece { anchor, .. } = view.scroll.anchor() else {
        return;
    };
    let snapshot = view.latest_display_snapshot.as_ref().or(snapshot_fallback);
    let resolved_char_offset = buffer.document().piece_tree().anchor_position(anchor);
    let unresolved = match resolved_char_offset {
        Some(char_offset) => snapshot
            .is_some_and(|snapshot| snapshot.row_for_char_offset(char_offset as u32).is_none()),
        None => true,
    };
    if !unresolved {
        return;
    }

    let tracked_anchor = view.take_piece_anchor_for_release();
    if tracked_anchor.is_none() {
        view.scroll.replace_anchor(ScrollAnchor::TOP);
    }
    view.set_editor_pixel_offset(preserved_offset);
    if let Some(anchor) = tracked_anchor {
        buffer
            .document_mut()
            .piece_tree_mut()
            .release_anchor(anchor);
    }
}

fn sync_local_scroll_state(ui: &egui::Ui, scroll_id: egui::Id, offset: egui::Vec2) {
    sync_editor_scroll_state(ui, scroll_id, offset);
    let mut local_state = scrolling::ScrollState::load(ui, scroll_id);
    local_state.offset = offset;
    local_state.store(ui, scroll_id);
}

fn virtual_editor_content_height(
    tab: &WorkspaceTab,
    view_id: ViewId,
    virtual_row_height: f32,
) -> f32 {
    tab.buffer_for_view(view_id)
        .map(|buffer| buffer.line_count.max(1) as f32 * virtual_row_height)
        .unwrap_or_default()
}

fn finish_editor_scroll_frame(
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    frame: &EditorScrollFrame<'_>,
    output: &scrolling::ScrollAreaOutput<EditorContentOutcome>,
    content_size: egui::Vec2,
    drag_requested_scroll_offset: Option<egui::Vec2>,
) {
    if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
        publish_scroll_manager_metrics(view, output.inner_rect, frame.row_height, content_size);
        drain_pending_scroll_intents(view, buffer, frame.previous_snapshot);
        let scrollbar_requested_scroll_offset = output.did_scroll.then_some(output.state.offset);
        if let Some(offset) = resolve_editor_scroll_offset_override(
            content_size,
            output.inner_rect.size(),
            frame.wheel_requested_scroll_offset,
            drag_requested_scroll_offset,
            scrollbar_requested_scroll_offset,
        ) {
            view.set_editor_pixel_offset_resolved(buffer, offset);
        }
        view.upgrade_scroll_anchor_to_piece(buffer);
    }
}

/// Publish the latest viewport rect, row height, and content extent to the
/// per-view `ScrollManager` so subsequent `ScrollIntent::Pages` / `Reveal`
/// resolution operates on real geometry rather than zeros.
fn publish_scroll_manager_metrics(
    view: &mut crate::app::domain::EditorViewState,
    viewport_rect: egui::Rect,
    row_height: f32,
    content_size: egui::Vec2,
) {
    let visible_rows = if row_height > 0.0 {
        (viewport_rect.height() / row_height).ceil().max(1.0) as u32
    } else {
        1
    };
    view.scroll.set_metrics(scrolling::ViewportMetrics {
        viewport_rect,
        row_height,
        column_width: row_height * 0.5,
        visible_rows,
        visible_columns: 0,
    });
    let display_rows = if row_height > 0.0 {
        (content_size.y / row_height).ceil().max(0.0) as u32
    } else {
        0
    };
    view.scroll.set_extent(scrolling::ContentExtent {
        display_rows,
        height: content_size.y,
        max_line_width: content_size.x,
    });
}

fn editor_pixel_offset_resolved(
    view: &crate::app::domain::EditorViewState,
    buffer: &crate::app::domain::BufferState,
    snapshot_fallback: Option<&DisplaySnapshot>,
) -> egui::Vec2 {
    let metrics = view.scroll.metrics();
    let snapshot = view.latest_display_snapshot.as_ref().or(snapshot_fallback);
    let resolve = |id| buffer.document().piece_tree().anchor_position(id);
    let anchor_to_row = scrolling::display_aware_anchor_to_row(snapshot, resolve);
    let row = anchor_to_row(view.scroll.anchor());
    let y = row * metrics.row_height.max(0.0);
    egui::vec2(view.scroll.horizontal_px(), y)
}

/// Drain any `ScrollIntent`s queued on the view through the per-view
/// `ScrollManager`. This is the renderer-side half of Phase 4 wiring: input
/// emitters push intents (search jumps, programmatic scrolls, future page/line
/// nav), and the renderer consumes them once per frame before deriving the
/// pixel offset that drives the egui-style `ScrollArea`.
fn drain_pending_scroll_intents(
    view: &mut crate::app::domain::EditorViewState,
    buffer: &crate::app::domain::BufferState,
    snapshot_fallback: Option<&DisplaySnapshot>,
) {
    if view.pending_intents.is_empty() {
        return;
    }
    let intents = std::mem::take(&mut view.pending_intents);
    let snapshot = view
        .latest_display_snapshot
        .as_ref()
        .or(snapshot_fallback)
        .cloned();
    let resolve = |id| buffer.document().piece_tree().anchor_position(id);
    let anchor_to_row = scrolling::display_aware_anchor_to_row(snapshot.as_ref(), resolve);
    for intent in intents {
        view.scroll
            .apply_intent(intent, &anchor_to_row, scrolling::naive_row_to_anchor);
    }
}

fn sync_editor_scroll_state(ui: &egui::Ui, scroll_id: egui::Id, offset: egui::Vec2) {
    let persistent_id = ui.make_persistent_id(scroll_id);
    let mut state = egui::scroll_area::State::load(ui.ctx(), persistent_id).unwrap_or_default();
    if state.offset != offset {
        state.offset = offset;
        state.store(ui.ctx(), persistent_id);
    }
}

fn local_scroll_source(
    _egui_vis: egui::scroll_area::ScrollBarVisibility,
) -> scrolling::ScrollSource {
    // Editor handles its own pointer wheel + drag (selection edges, cursor
    // reveal suppression). Scrollbar drag and programmatic targets go through
    // the local container.
    scrolling::ScrollSource {
        scroll_bar: true,
        mouse_wheel: false,
        drag: false,
        programmatic: true,
    }
}

fn scrollbar_policy_from_egui(
    vis: egui::scroll_area::ScrollBarVisibility,
) -> scrolling::ScrollbarPolicy {
    use egui::scroll_area::ScrollBarVisibility;
    match vis {
        ScrollBarVisibility::AlwaysVisible => scrolling::ScrollbarPolicy::AlwaysVisible,
        ScrollBarVisibility::AlwaysHidden => scrolling::ScrollbarPolicy::Hidden,
        ScrollBarVisibility::VisibleWhenNeeded => scrolling::ScrollbarPolicy::VisibleWhenNeeded,
    }
}

fn resolve_editor_scroll_offset_override(
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
    wheel_requested_scroll_offset: Option<egui::Vec2>,
    drag_requested_scroll_offset: Option<egui::Vec2>,
    scrollbar_requested_scroll_offset: Option<egui::Vec2>,
) -> Option<egui::Vec2> {
    drag_requested_scroll_offset
        .or(scrollbar_requested_scroll_offset)
        .or(wheel_requested_scroll_offset)
        .map(|offset| clamp_scroll_offset(offset, content_size, viewport_size))
}

fn editor_scroll_content_size(content_size: egui::Vec2, virtual_content_height: f32) -> egui::Vec2 {
    egui::vec2(
        content_size.x,
        content_size.y.max(virtual_content_height.max(0.0)),
    )
}

fn requested_scroll_offset_for_pointer_drag(
    ui: &egui::Ui,
    current_offset: egui::Vec2,
    interaction_response: Option<&egui::Response>,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
    inner_rect: egui::Rect,
) -> Option<egui::Vec2> {
    if !pointer_over_rect(ui, inner_rect)
        || !ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
        || interaction_response
            .is_some_and(|response| response.dragged_by(egui::PointerButton::Primary))
    {
        return None;
    }

    scroll_offset_from_drag_delta(
        current_offset,
        ui.input(|input| input.pointer.delta()),
        content_size,
        viewport_size,
    )
}

fn apply_selection_edge_autoscroll_intent(
    ui: &egui::Ui,
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    interaction_response: Option<&egui::Response>,
    inner_rect: egui::Rect,
    row_height: f32,
) {
    let Some(delta) =
        selection_edge_autoscroll_delta(ui, interaction_response, inner_rect, row_height)
    else {
        return;
    };
    if delta == egui::Vec2::ZERO {
        clear_edge_autoscroll(tab, view_id);
        return;
    }
    apply_edge_autoscroll_delta(tab, view_id, delta);
}

fn selection_edge_autoscroll_delta(
    ui: &egui::Ui,
    interaction_response: Option<&egui::Response>,
    inner_rect: egui::Rect,
    row_height: f32,
) -> Option<egui::Vec2> {
    let is_drag_selecting = ui
        .input(|input| input.pointer.button_down(egui::PointerButton::Primary))
        && interaction_response
            .is_some_and(|response| response.dragged_by(egui::PointerButton::Primary));
    let pointer_pos = ui.input(|input| input.pointer.latest_pos())?;
    is_drag_selecting.then(|| selection_edge_drag_delta(inner_rect, pointer_pos, row_height))
}

fn clear_edge_autoscroll(tab: &mut WorkspaceTab, view_id: ViewId) {
    if let Some(view) = tab.view_mut(view_id) {
        view.scroll.clear_edge_autoscroll();
    }
}

fn apply_edge_autoscroll_delta(tab: &mut WorkspaceTab, view_id: ViewId, delta: egui::Vec2) {
    let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) else {
        return;
    };
    let snapshot = view.latest_display_snapshot.clone();
    let resolve = |id| buffer.document().piece_tree().anchor_position(id);
    let anchor_to_row = scrolling::display_aware_anchor_to_row(snapshot.as_ref(), resolve);
    apply_edge_autoscroll_axis(view, scrolling::Axis::X, delta.x, &anchor_to_row);
    apply_edge_autoscroll_axis(view, scrolling::Axis::Y, delta.y, &anchor_to_row);
    view.scroll
        .tick_edge_autoscroll(1.0, &anchor_to_row, scrolling::naive_row_to_anchor);
    view.scroll.clear_edge_autoscroll();
}

fn apply_edge_autoscroll_axis(
    view: &mut crate::app::domain::EditorViewState,
    axis: scrolling::Axis,
    velocity: f32,
    anchor_to_row: &impl Fn(scrolling::ScrollAnchor) -> f32,
) {
    view.scroll.apply_intent(
        scrolling::ScrollIntent::EdgeAutoscroll { axis, velocity },
        anchor_to_row,
        scrolling::naive_row_to_anchor,
    );
}

fn requested_scroll_offset_for_pointer_wheel(
    ui: &egui::Ui,
    current_offset: egui::Vec2,
) -> Option<egui::Vec2> {
    if callout::scroll_blocker_hovered(ui.ctx()) {
        return None;
    }
    if !pointer_over_rect(ui, ui.max_rect()) {
        return None;
    }

    scroll_offset_from_wheel_delta(current_offset, ui.input(|input| input.smooth_scroll_delta))
}

fn pointer_over_rect(ui: &egui::Ui, rect: egui::Rect) -> bool {
    ui.input(|input| {
        input
            .pointer
            .hover_pos()
            .is_some_and(|pos| rect.contains(pos))
    })
}

fn scroll_offset_from_wheel_delta(
    current_offset: egui::Vec2,
    scroll_delta: egui::Vec2,
) -> Option<egui::Vec2> {
    let desired = egui::vec2(
        (current_offset.x - scroll_delta.x).max(0.0),
        (current_offset.y - scroll_delta.y).max(0.0),
    );
    (desired != current_offset).then_some(desired)
}

fn scroll_offset_from_drag_delta(
    current_offset: egui::Vec2,
    drag_delta: egui::Vec2,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
) -> Option<egui::Vec2> {
    if drag_delta == egui::Vec2::ZERO {
        return None;
    }

    let desired = clamp_scroll_offset(current_offset - drag_delta, content_size, viewport_size);
    (desired != current_offset).then_some(desired)
}

fn selection_edge_drag_delta(
    viewport_rect: egui::Rect,
    pointer_pos: egui::Pos2,
    row_height: f32,
) -> egui::Vec2 {
    let config = editor_selection_autoscroll_config(row_height);
    egui::vec2(
        edge_auto_scroll_delta(
            viewport_rect,
            pointer_pos,
            AutoScrollAxis::Horizontal,
            config,
        ),
        edge_auto_scroll_delta(viewport_rect, pointer_pos, AutoScrollAxis::Vertical, config),
    )
}

fn clamp_scroll_offset(
    offset: egui::Vec2,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
) -> egui::Vec2 {
    let max_offset = max_scroll_offset(content_size, viewport_size);
    egui::vec2(
        offset.x.clamp(0.0, max_offset.x),
        offset.y.clamp(0.0, max_offset.y),
    )
}

fn max_scroll_offset(content_size: egui::Vec2, viewport_size: egui::Vec2) -> egui::Vec2 {
    egui::vec2(
        (content_size.x - viewport_size.x).max(0.0),
        (content_size.y - viewport_size.y).max(0.0),
    )
}

fn restore_previous_snapshot_if_needed(
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    previous_snapshot: Option<DisplaySnapshot>,
) {
    if tab
        .view(view_id)
        .is_some_and(|view| view.latest_display_snapshot.is_none())
        && let Some(view) = tab.view_mut(view_id)
    {
        view.latest_display_snapshot = previous_snapshot;
    }
}

fn missing_editor_content_outcome() -> EditorContentOutcome {
    EditorContentOutcome {
        changed: false,
        focused: false,
        request_editor_focus: false,
        interaction_response: None,
    }
}

#[cfg(test)]
mod tests;
