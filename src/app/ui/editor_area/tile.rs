mod autoscroll;
mod chrome;
mod context_menu;
mod scroll_frame;
mod scroll_input;
mod style;

use self::autoscroll::apply_selection_edge_autoscroll_intent;
use self::chrome::{apply_tile_body_focus, handle_tile_click, paint_tile_frame};
use self::scroll_frame::{
    drain_pending_scroll_intents, editor_scroll_content_size, publish_scroll_manager_metrics,
    recover_unresolved_piece_anchor, resolved_scroll_offset_for_view, sync_local_scroll_state,
    sync_programmatic_scroll_offset, virtual_editor_content_height,
};
use self::scroll_input::{
    local_scroll_source, resolve_editor_scroll_offset_override, scrollbar_policy_from_egui,
};
use self::style::{editor_content_style, editor_font_id};
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{EditorViewState, SplitPath, ViewId, WorkspaceTab};
use crate::app::ui::editor_content::{self, EditorContentOutcome, EditorContentStyle};
use crate::app::ui::scrolling;
use crate::app::ui::scrolling::DisplaySnapshot;
use crate::app::ui::tab_drag;
use crate::app::ui::tile_header::{
    self, SplitPreviewOverlay, TileAction, TileHeaderRequest, TileHeaderState,
};
use crate::app::ui::widget_ids;
use eframe::egui;

struct TileBodyOutcome {
    changed: bool,
    focused: bool,
    interaction_response: Option<egui::Response>,
}

#[derive(Clone)]
pub(super) struct TileRenderRequest {
    pub(super) tab_index: usize,
    pub(super) view_id: ViewId,
    pub(super) pane_path: SplitPath,
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
    pane_path: &'a SplitPath,
    scroll_bar_visibility: egui::scroll_area::ScrollBarVisibility,
    content_style: EditorContentStyle<'a>,
}

pub(super) fn render_tile(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
    state: &mut TileRenderState<'_>,
) {
    widget_ids::rect_scope_with_layout(
        ui,
        request.rect,
        "tile.root",
        egui::Layout::top_down(egui::Align::Min),
        |ui| render_tile_contents(ui, app, request, state),
    );
}

fn render_tile_contents(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
    state: &mut TileRenderState<'_>,
) {
    let tile_response = handle_tile_click(ui, app, &request, state.actions);
    paint_tile_frame(
        ui,
        request.rect,
        request.is_active,
        app.state.app_settings.editor_background_color(),
    );

    let body_outcome = render_tile_body(ui, app, &request);
    *state.any_editor_changed |= body_outcome.changed;
    apply_tile_body_focus(
        body_outcome.focused,
        request.is_active,
        request.view_id,
        state.actions,
    );
    render_tile_header(ui, app, &request, state);
    if let Some(editor_response) = body_outcome.interaction_response.as_ref() {
        context_menu::attach_editor_context_menu(editor_response, ui, app, &request, state.actions);
    }
    if should_attach_tile_context_menu(&tile_response, body_outcome.interaction_response.as_ref()) {
        context_menu::attach_editor_context_menu(&tile_response, ui, app, &request, state.actions);
    }
}

fn should_attach_tile_context_menu(
    tile_response: &egui::Response,
    editor_response: Option<&egui::Response>,
) -> bool {
    let Some(editor_response) = editor_response else {
        return true;
    };
    tile_response
        .ctx
        .input(|input| input.pointer.interact_pos())
        .is_none_or(|pos| !editor_response.rect.contains(pos))
}

fn render_tile_header(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: &TileRenderRequest,
    state: &mut TileRenderState<'_>,
) {
    tile_header::render_tile_header(
        ui,
        app,
        TileHeaderRequest {
            tab_index: request.tab_index,
            view_id: request.view_id,
            pane_path: request.pane_path.clone(),
            tile_rect: request.rect,
            can_close: request.can_close,
        },
        &mut TileHeaderState {
            actions: state.actions,
            preview_overlay: state.preview_overlay,
        },
    );
}

fn render_tile_body(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: &TileRenderRequest,
) -> TileBodyOutcome {
    widget_ids::rect_scope_with_layout(
        ui,
        request.rect,
        "tile.body",
        egui::Layout::top_down(egui::Align::Min),
        |ui| render_tile_body_contents(ui, app, request),
    )
    .inner
}

fn render_tile_body_contents(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: &TileRenderRequest,
) -> TileBodyOutcome {
    let request_focus = app.should_focus_view(request.view_id);
    let editor_font_id = editor_font_id(app.state.app_settings.font_size());
    let mut content_style =
        editor_content_style(app, request.is_active, request_focus, &editor_font_id);
    content_style.text_edit.right_to_left_reading_order = app
        .tab_manager
        .tabs
        .as_slice()
        .get(request.tab_index)
        .and_then(|tab| tab.buffer_for_view(request.view_id))
        .is_some_and(|buffer| buffer.right_to_left_reading_order);
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[request.tab_index];
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
            pane_path: &request.pane_path,
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
    widget_ids::root_id(("editor_scroll", view_id))
}

fn show_editor_scroll_area(
    ui: &mut egui::Ui,
    tab: &mut WorkspaceTab,
    request: TileScrollRequest<'_>,
) -> EditorContentOutcome {
    let frame = prepare_editor_scroll_frame(ui, tab, request.view_id, &request.content_style);
    let word_wrap = request.content_style.text_edit.word_wrap;
    let scrollbar_x = if word_wrap {
        scrolling::ScrollbarPolicy::Hidden
    } else {
        scrollbar_policy_from_egui(request.scroll_bar_visibility)
    };
    let output = scrolling::ScrollArea::new(frame.scroll_id)
        .interaction_id(widget_ids::root_id((
            "editor_scroll.interaction",
            request.pane_path,
        )))
        .source(local_scroll_source(request.scroll_bar_visibility))
        .scrollbar_x(scrollbar_x)
        .scrollbar_y(scrollbar_policy_from_egui(request.scroll_bar_visibility))
        .min_content_size(egui::vec2(0.0, frame.virtual_content_height))
        // Match the no-overscroll clamp applied in `resolve_editor_scroll_offset_override`.
        // Otherwise the scrollbar's track is sized for one extra viewport-height of
        // overscroll while the offset is clamped to `content - viewport`, so the thumb
        // pins at ~93% of the track and the per-frame clamp tug-of-war flickers.
        .eof_overscroll(false)
        .show_viewport(ui, |ui, _offset, viewport| {
            let mut content_style = request.content_style;
            content_style.viewport = Some(viewport);
            tab.buffer_and_view_mut(request.view_id)
                .map(|(buffer, view)| {
                    editor_content::render_editor_content(ui, buffer, view, content_style)
                })
                .unwrap_or_else(missing_editor_content_outcome)
        });

    let content_size = editor_scroll_content_size(
        output.content_size,
        frame.virtual_content_height,
        word_wrap.then_some(output.inner_rect.width()),
    );
    if selection_drag_active(
        ui,
        output.inner.interaction_response.as_ref(),
        output.inner_rect,
    ) {
        suppress_selection_drag_reveals(tab, request.view_id);
    }
    finish_editor_scroll_frame(ui, tab, request.view_id, &frame, &output, content_size);
    apply_selection_edge_autoscroll_intent(
        ui,
        tab,
        request.view_id,
        frame.scroll_id,
        output.inner.interaction_response.as_ref(),
        output.inner_rect,
        frame.row_height,
    );
    output.inner
}

struct EditorScrollFrame<'a> {
    scroll_id: egui::Id,
    previous_snapshot: Option<&'a DisplaySnapshot>,
    layout_requested_scroll_offset: Option<egui::Vec2>,
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
        drain_pending_scroll_intents(view, buffer, previous_snapshot, None);
    }
    let layout_requested_scroll_offset =
        layout_resize_scroll_offset(ui, tab, view_id, scroll_id, content_style);
    let scroll_offset = layout_requested_scroll_offset
        .unwrap_or_else(|| resolved_scroll_offset_for_view(tab, view_id, previous_snapshot));
    let scroll_offset = if content_style.text_edit.word_wrap {
        egui::vec2(0.0, scroll_offset.y)
    } else {
        scroll_offset
    };
    sync_local_scroll_state(ui, scroll_id, scroll_offset);
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(content_style.text_edit.editor_font_id));
    let viewport_height = ui.available_rect_before_wrap().height().max(0.0);
    let virtual_content_height = virtual_editor_content_height(
        tab,
        view_id,
        row_height.max(content_style.text_edit.editor_font_id.size),
        viewport_height,
        previous_snapshot,
    );
    EditorScrollFrame {
        scroll_id,
        previous_snapshot,
        layout_requested_scroll_offset,
        row_height,
        virtual_content_height,
    }
}

fn layout_resize_scroll_offset(
    ui: &egui::Ui,
    tab: &WorkspaceTab,
    view_id: ViewId,
    scroll_id: egui::Id,
    content_style: &EditorContentStyle<'_>,
) -> Option<egui::Vec2> {
    if !content_style.text_edit.word_wrap {
        return None;
    }

    let previous_width = tab
        .view(view_id)
        .map(|view| view.scroll.metrics().viewport_rect.width())
        .filter(|width| width.is_finite() && *width > 0.0)?;
    let current_width = ui.available_rect_before_wrap().width();
    if !current_width.is_finite() || (previous_width - current_width).abs() < 1.0 {
        return None;
    }

    Some(scrolling::ScrollState::load(ui, scroll_id).offset)
}

fn finish_editor_scroll_frame(
    ui: &egui::Ui,
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    frame: &EditorScrollFrame<'_>,
    output: &scrolling::ScrollAreaOutput<EditorContentOutcome>,
    content_size: egui::Vec2,
) {
    if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
        publish_scroll_manager_metrics(view, output.inner_rect, frame.row_height, content_size);
        let had_pending_intents = !view.pending_intents.is_empty();
        if output.did_scroll {
            view.clear_cursor_reveal();
        }
        drain_pending_scroll_intents(
            view,
            buffer,
            frame.previous_snapshot,
            Some(output.state.offset),
        );
        if had_pending_intents {
            sync_programmatic_scroll_offset(ui, view, buffer, frame.scroll_id, output.inner_rect);
        } else {
            let scrollbar_requested_scroll_offset =
                output.did_scroll.then_some(output.state.offset);
            if let Some(offset) = resolve_editor_scroll_offset_override(
                content_size,
                output.inner_rect.size(),
                frame.layout_requested_scroll_offset,
                None,
                scrollbar_requested_scroll_offset,
            ) {
                view.set_editor_pixel_offset_resolved(buffer, offset);
            }
        }
        view.upgrade_scroll_anchor_to_piece(buffer);
    }
}

fn selection_drag_active(
    ui: &egui::Ui,
    interaction_response: Option<&egui::Response>,
    inner_rect: egui::Rect,
) -> bool {
    let primary_down = ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary));
    let pointer_near_editor = ui
        .input(|input| input.pointer.latest_pos())
        .is_some_and(|pos| inner_rect.expand(32.0).contains(pos));
    primary_down
        && pointer_near_editor
        && interaction_response
            .is_some_and(|response| response.dragged_by(egui::PointerButton::Primary))
}

fn suppress_selection_drag_reveals(tab: &mut WorkspaceTab, view_id: ViewId) {
    if let Some(view) = tab.view_mut(view_id) {
        suppress_view_reveals_for_selection_drag(view);
    }
}

fn suppress_view_reveals_for_selection_drag(view: &mut EditorViewState) {
    view.clear_cursor_reveal();
    view.pending_intents
        .retain(|intent| !matches!(intent, scrolling::ScrollIntent::Reveal { .. }));
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
