mod cursor;
mod editing;
mod highlighting;
mod interactions;
mod layout;
mod painting;
#[cfg(test)]
mod tests;
mod types;
mod word_boundary;

pub use highlighting::build_layouter;
pub use types::{
    CharCursor, CursorRange, EditOperation, EditorHighlightStyle, LayouterFn, OperationRecord,
    TextEditOptions,
};

use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState};
use crate::app::ui::scrolling::DisplaySnapshot;
use eframe::egui;
use interactions::{
    KeyboardInputRequest, MouseInteractionRequest, handle_keyboard_events,
    handle_mouse_interaction, page_jump_rows, sync_view_cursor_before_render,
};
use layout::{
    allocate_editor_rect, build_editor_galley, editor_row_height, editor_viewport_height,
    galley_origin, total_editor_content_height,
};
use painting::{
    CursorPaintOutcome, EditorFrame, consume_cursor_reveal, paint_editor, publish_ime_output,
};
use std::sync::Arc;

fn editor_focus_lock_filter() -> egui::EventFilter {
    egui::EventFilter {
        horizontal_arrows: true,
        vertical_arrows: true,
        tab: true,
        escape: false,
    }
}

pub struct EditorWidgetOutcome {
    pub changed: bool,
    pub focused: bool,
    pub request_editor_focus: bool,
    pub response: egui::Response,
}

// ---------------------------------------------------------------------------
// Public rendering entry points
// ---------------------------------------------------------------------------

// This is the top-level editor-frame pipeline; keeping its ordered phases in
// one place is clearer than distributing the frame lifecycle across helpers.
#[allow(clippy::too_many_lines)]
pub fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> EditorWidgetOutcome {
    view.resolve_anchored_ranges(buffer);
    let total_chars = buffer.current_file_length().chars;
    let mut galley_context = build_editor_galley(ui, buffer, view, options, viewport);

    let row_height = editor_row_height(ui, options.editor_font_id);
    let viewport_height = editor_viewport_height(ui, viewport);
    let total_content_height = total_editor_content_height(
        buffer.line_count.max(1),
        row_height,
        &galley_context.galley,
        galley_context.logical_line_base,
        viewport_height,
    );
    let (rect, response) = allocate_editor_rect(
        ui,
        &galley_context.galley,
        view.id,
        options,
        total_content_height,
        viewport,
        galley_context.virtual_width,
    );
    let mut galley_pos = galley_origin(
        rect,
        galley_context.logical_line_base,
        row_height,
        galley_context.display_column_base,
    );
    request_editor_focus(ui, &response, options.request_focus);

    // The pre-input galley bakes the active cursor selection into its text
    // formats. If input changes the document or selection, rebuild before
    // paint so highlighted text color does not lag by a frame.
    let pre_active_selection = buffer.active_selection.clone();
    let pre_cursor_range = view.cursor_range;
    let input = process_editor_input(
        ui,
        buffer,
        view,
        EditorInputRequest {
            response: &response,
            galley: &galley_context.galley,
            rect,
            galley_pos,
            options,
            viewport,
            row_height,
            total_chars,
            char_offset_base: galley_context.char_offset_base,
            slice_chars: galley_context.slice_chars,
            display_map: galley_context.display_map.as_ref(),
        },
    );

    let mut document_revision = buffer.document_revision();
    if should_rebuild_galley_after_input(
        input.changed,
        pre_active_selection.as_ref(),
        buffer.active_selection.as_ref(),
        pre_cursor_range,
        view.cursor_range,
        galley_context.char_offset_base,
        galley_context.slice_chars,
    ) {
        document_revision = buffer.document_revision();
        galley_context = build_editor_galley(ui, buffer, view, options, viewport);
        galley_pos = galley_origin(
            rect,
            galley_context.logical_line_base,
            row_height,
            galley_context.display_column_base,
        );
        request_repaint_if_wrapped_content_height_changed(WrappedContentHeightRepaint {
            ui,
            changed: input.changed,
            word_wrap: options.word_wrap,
            previous_content_height: total_content_height,
            line_count: buffer.line_count.max(1),
            row_height,
            galley: &galley_context.galley,
            logical_line_base: galley_context.logical_line_base,
            viewport_height,
        });
    }

    let paint_outcome = if ui.is_rect_visible(rect) {
        paint_editor(
            ui,
            EditorFrame {
                galley: &galley_context.galley,
                galley_pos,
                rect,
                options,
                focused: input.focused,
                char_offset_base: galley_context.char_offset_base,
                slice_chars: galley_context.slice_chars,
                display_map: galley_context.display_map.as_ref(),
                tab_offsets: &galley_context.tab_offsets,
                active_selection: buffer.active_selection.clone(),
                cursor_range: view.cursor_range,
                cursor_reveal_mode: view.cursor_reveal_mode(),
                animate_cursor_transition: input.animate_cursor_transition,
                snap_cursor_animation: input.snap_cursor_animation,
                caret_animation: &mut view.caret_animation,
                ime_preedit: view.ime_preedit.as_ref(),
                replacement_preview: view.search_replacement_preview.as_ref(),
            },
        )
    } else {
        CursorPaintOutcome::default()
    };
    if let Some(intent) = paint_outcome.reveal_intent {
        view.request_intent(intent);
    }
    if let Some((rect, cursor_rect)) = paint_outcome.ime_geometry {
        publish_ime_output(ui, rect, cursor_rect, view);
    }
    consume_cursor_reveal(view, false, paint_outcome.reveal_attempted);
    sync_ime_output_focus(view, input.focused);

    store_latest_snapshot(
        view,
        &galley_context.galley,
        row_height,
        false,
        Some(document_revision),
        galley_context.char_offset_base,
        galley_context.logical_line_base,
    );

    view.editor_has_focus = input.focused;

    EditorWidgetOutcome {
        changed: input.changed,
        focused: input.focused,
        request_editor_focus: false,
        response,
    }
}

struct EditorInputOutcome {
    focused: bool,
    changed: bool,
    animate_cursor_transition: bool,
    snap_cursor_animation: bool,
}

struct EditorInputRequest<'a> {
    response: &'a egui::Response,
    galley: &'a egui::Galley,
    rect: egui::Rect,
    galley_pos: egui::Pos2,
    options: TextEditOptions<'a>,
    viewport: Option<egui::Rect>,
    row_height: f32,
    total_chars: usize,
    char_offset_base: usize,
    slice_chars: usize,
    display_map: Option<&'a layout::DisplayTextMap>,
}

fn process_editor_input(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    request: EditorInputRequest<'_>,
) -> EditorInputOutcome {
    let prev_cursor = view.cursor_range;
    let prev_cursor_line = prev_cursor.and_then(|cursor| primary_line_index(buffer, cursor));
    let editor_was_focused = view.editor_has_focus;
    handle_mouse_interaction(
        ui,
        view,
        MouseInteractionRequest {
            response: request.response,
            galley: request.galley,
            rect: request.rect,
            galley_pos: request.galley_pos,
            piece_tree: buffer.document().piece_tree(),
            char_offset_base: request.char_offset_base,
            display_map: request.display_map,
        },
    );
    let cursor_after_mouse = view.cursor_range;
    let suppress_cursor_reveal = request.response.dragged_by(egui::PointerButton::Primary);
    let pointer_interacted = request.response.clicked()
        || request.response.secondary_clicked()
        || request.response.middle_clicked()
        || request.response.dragged();
    let focused = request.response.has_focus()
        || request.response.gained_focus()
        || request.options.request_focus;
    sync_view_cursor_before_render(view, focused);
    let changed = handle_focused_keyboard_input(ui, buffer, view, &request, focused);
    request_cursor_reveal_after_input(
        buffer,
        view,
        prev_cursor,
        prev_cursor_line,
        request.options.word_wrap,
        changed,
        suppress_cursor_reveal,
    );
    if publish_active_selection(buffer, view, focused) {
        ui.ctx().request_repaint();
    }
    view.sync_cursor_anchors_from_ranges(buffer);

    let animate_cursor_transition = editor_was_focused
        && focused
        && !changed
        && cursor_after_mouse == prev_cursor
        && view.cursor_range != prev_cursor;
    let snap_cursor_animation = changed
        || pointer_interacted
        || !editor_was_focused
        || request.response.gained_focus()
        || request.options.request_focus
        || view.ime_preedit.is_some();
    EditorInputOutcome {
        focused,
        changed,
        animate_cursor_transition,
        snap_cursor_animation,
    }
}

fn request_cursor_reveal_after_input(
    buffer: &BufferState,
    view: &mut EditorViewState,
    prev_cursor: Option<CursorRange>,
    prev_cursor_line: Option<usize>,
    word_wrap: bool,
    changed: bool,
    suppress_reveal: bool,
) {
    if suppress_reveal {
        view.clear_cursor_reveal();
        return;
    }

    if view.cursor_range == prev_cursor {
        return;
    }

    if word_wrap
        || !changed
        || edit_moved_primary_cursor_to_new_line(buffer, view.cursor_range, prev_cursor_line)
    {
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    } else {
        view.request_cursor_reveal(CursorRevealMode::KeepHorizontalVisible);
    }
}

fn edit_moved_primary_cursor_to_new_line(
    buffer: &BufferState,
    cursor: Option<CursorRange>,
    prev_cursor_line: Option<usize>,
) -> bool {
    let Some(prev_line) = prev_cursor_line else {
        return true;
    };
    primary_line_index(buffer, cursor.unwrap_or_default()).is_none_or(|line| line != prev_line)
}

fn primary_line_index(buffer: &BufferState, cursor: CursorRange) -> Option<usize> {
    (cursor.primary.index <= buffer.current_file_length().chars).then(|| {
        buffer
            .document()
            .piece_tree()
            .line_index_at_offset(cursor.primary.index)
    })
}

fn handle_focused_keyboard_input(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    request: &EditorInputRequest<'_>,
    focused: bool,
) -> bool {
    if buffer.is_loading_preview {
        return false;
    }
    focused
        && handle_keyboard_events(
            ui,
            buffer,
            view,
            KeyboardInputRequest {
                galley: request.galley,
                page_jump_rows: page_jump_rows(request.viewport, request.row_height),
                total_chars: request.total_chars,
                char_offset_base: request.char_offset_base,
                slice_chars: request.slice_chars,
                display_map: request.display_map,
                indentation_style: request.options.indentation_style,
                indentation_width: request.options.indentation_width,
            },
        )
}

#[must_use]
pub fn select_all_cursor(total_chars: usize) -> CursorRange {
    CursorRange::two(0, total_chars)
}

#[must_use]
pub fn selected_text(buffer: &BufferState, cursor: CursorRange) -> Option<String> {
    let range = types::selection_char_range(&cursor)?;
    Some(buffer.document().piece_tree().extract_range(range))
}

pub fn cut_selected_text(
    buffer: &mut BufferState,
    cursor: CursorRange,
) -> Option<(CursorRange, String)> {
    (!buffer.is_loading_preview && !cursor.is_empty()).then(|| editing::apply_cut(buffer, &cursor))
}

pub fn delete_selected_text(buffer: &mut BufferState, cursor: CursorRange) -> Option<CursorRange> {
    (!buffer.is_loading_preview && !cursor.is_empty())
        .then(|| editing::apply_delete_selection(buffer, &cursor))
}

pub(super) fn store_latest_snapshot(
    view: &mut EditorViewState,
    galley: &Arc<egui::Galley>,
    row_height: f32,
    changed: bool,
    revision: Option<u64>,
    char_offset_base: usize,
    logical_line_base: usize,
) {
    if changed {
        view.latest_display_snapshot = None;
        view.latest_display_snapshot_revision = None;
    } else {
        let selection_range = view
            .cursor_range
            .as_ref()
            .and_then(types::selection_char_range);
        view.latest_display_snapshot = Some(DisplaySnapshot::from_galley_with_base_and_overlays(
            galley.as_ref(),
            row_height,
            char_offset_base,
            logical_line_base,
            selection_range,
            &view.search_highlights.ranges,
        ));
        view.latest_display_snapshot_revision = revision;
    }
}

pub(super) fn sync_ime_output_focus(view: &mut EditorViewState, focused: bool) {
    if !focused {
        view.clear_ime_output();
        view.ime_preedit = None;
    }
}

fn request_editor_focus(ui: &mut egui::Ui, response: &egui::Response, request_focus: bool) {
    if request_focus {
        response.request_focus();
    }
    if response.has_focus() {
        ui.memory_mut(|mem| {
            mem.set_focus_lock_filter(response.id, editor_focus_lock_filter());
        });
    }
}

fn publish_active_selection(
    buffer: &mut BufferState,
    view: &EditorViewState,
    focused: bool,
) -> bool {
    let previous = buffer.active_selection.clone();
    if focused {
        buffer.active_selection = view
            .cursor_range
            .as_ref()
            .and_then(types::selection_char_range);
    }
    previous != buffer.active_selection
}

fn should_rebuild_galley_after_input(
    changed: bool,
    pre_active_selection: Option<&std::ops::Range<usize>>,
    post_active_selection: Option<&std::ops::Range<usize>>,
    pre_cursor_range: Option<CursorRange>,
    post_cursor_range: Option<CursorRange>,
    char_offset_base: usize,
    slice_chars: usize,
) -> bool {
    changed
        || pre_active_selection != post_active_selection
        || (pre_cursor_range != post_cursor_range
            && !cursor_primary_in_slice(post_cursor_range, char_offset_base, slice_chars))
}

struct WrappedContentHeightRepaint<'a> {
    ui: &'a egui::Ui,
    changed: bool,
    word_wrap: bool,
    previous_content_height: f32,
    line_count: usize,
    row_height: f32,
    galley: &'a egui::Galley,
    logical_line_base: usize,
    viewport_height: f32,
}

fn request_repaint_if_wrapped_content_height_changed(input: WrappedContentHeightRepaint<'_>) {
    if !input.changed || !input.word_wrap {
        return;
    }

    let next_content_height = total_editor_content_height(
        input.line_count,
        input.row_height,
        input.galley,
        input.logical_line_base,
        input.viewport_height,
    );
    if (next_content_height - input.previous_content_height).abs() >= 1.0 {
        input.ui.ctx().request_repaint();
    }
}

fn cursor_primary_in_slice(
    cursor_range: Option<CursorRange>,
    char_offset_base: usize,
    slice_chars: usize,
) -> bool {
    let Some(cursor_range) = cursor_range else {
        return true;
    };
    let slice_end = char_offset_base.saturating_add(slice_chars);
    (char_offset_base..=slice_end).contains(&cursor_range.primary.index)
}
