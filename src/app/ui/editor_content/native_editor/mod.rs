mod cursor;
mod editing;
mod highlighting;
mod interactions;
mod layout;
mod painting;
mod types;
mod word_boundary;

pub use highlighting::build_layouter;
pub use types::{
    CharCursor, CursorRange, EditOperation, EditorHighlightStyle, LayouterFn, OperationRecord,
    TextEditOptions,
};

use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState};
use crate::app::ui::scrolling::DisplaySnapshot;
use crate::app::ui::widget_ids;
use eframe::egui;
use interactions::{
    handle_keyboard_events, handle_mouse_interaction, page_jump_rows,
    sync_view_cursor_before_render,
};
use layout::{
    allocate_editor_rect, build_editor_galley, editor_desired_width, editor_row_height,
    editor_viewport_height, galley_origin, total_editor_content_height,
};
use painting::{CursorPaintOutcome, consume_cursor_reveal, paint_editor, paint_galley};
use std::sync::Arc;

const EDITOR_FOCUS_LOCK_FILTER: egui::EventFilter = egui::EventFilter {
    horizontal_arrows: true,
    vertical_arrows: true,
    tab: false,
    escape: false,
};

pub struct EditorWidgetOutcome {
    pub changed: bool,
    pub focused: bool,
    pub request_editor_focus: bool,
    pub response: egui::Response,
}

// ---------------------------------------------------------------------------
// Public rendering entry points
// ---------------------------------------------------------------------------

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
        viewport_height,
    );
    let (rect, response) = allocate_editor_rect(
        ui,
        &galley_context.galley,
        view.id,
        options,
        total_content_height,
        viewport,
    );
    let mut galley_pos = galley_origin(rect, galley_context.logical_line_base, row_height);
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
        },
    );

    let mut document_revision = buffer.document_revision();
    if should_rebuild_galley_after_input(
        input.changed,
        pre_active_selection.as_ref(),
        buffer.active_selection.as_ref(),
        pre_cursor_range,
        view.cursor_range,
    ) {
        document_revision = buffer.document_revision();
        galley_context = build_editor_galley(ui, buffer, view, options, viewport);
        galley_pos = galley_origin(rect, galley_context.logical_line_base, row_height);
    }

    let paint_outcome = if ui.is_rect_visible(rect) {
        paint_editor(
            ui,
            &galley_context.galley,
            galley_pos,
            rect,
            view,
            options,
            input.focused,
            false,
            galley_context.char_offset_base,
            galley_context.slice_chars,
        )
    } else {
        CursorPaintOutcome::default()
    };
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
}

fn process_editor_input(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    request: EditorInputRequest<'_>,
) -> EditorInputOutcome {
    let prev_cursor = view.cursor_range;
    let prev_cursor_line = prev_cursor.and_then(|cursor| primary_line_index(buffer, cursor));
    handle_mouse_interaction(
        ui,
        request.response,
        request.galley,
        request.rect,
        request.galley_pos,
        view,
        buffer.document().piece_tree(),
        request.char_offset_base,
    );
    let suppress_cursor_reveal = request.response.dragged_by(egui::PointerButton::Primary);
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
        changed,
        suppress_cursor_reveal,
    );
    publish_active_selection(buffer, view, focused);
    view.sync_cursor_anchors_from_ranges(buffer);
    EditorInputOutcome { focused, changed }
}

fn request_cursor_reveal_after_input(
    buffer: &BufferState,
    view: &mut EditorViewState,
    prev_cursor: Option<CursorRange>,
    prev_cursor_line: Option<usize>,
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

    if !changed
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
    focused
        && handle_keyboard_events(
            ui,
            buffer,
            view,
            request.galley,
            page_jump_rows(request.viewport, request.row_height),
            request.total_chars,
            request.char_offset_base,
            request.slice_chars,
        )
}

pub fn render_read_only_text_edit(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    text: String,
    desired_rows: usize,
    options: TextEditOptions<'_>,
) -> EditorWidgetOutcome {
    let selection_range = view
        .cursor_range
        .as_ref()
        .and_then(types::selection_char_range);

    let wrap_width = if options.word_wrap {
        ui.available_width()
    } else {
        f32::INFINITY
    };
    let galley = highlighting::build_galley(
        ui,
        &text,
        options,
        &view.search_highlights,
        selection_range,
        wrap_width,
    );

    let row_height = editor_row_height(ui, options.editor_font_id);
    let desired_height = desired_rows.max(1) as f32 * row_height;
    let size = egui::vec2(
        editor_desired_width(ui, &galley, options.word_wrap, None),
        desired_height,
    );
    let response = widget_ids::allocate_exact_rect_interact(
        ui,
        size,
        ("native_editor.empty", view.id),
        egui::Sense::click(),
        "native_editor.empty",
    );
    let rect = response.rect;

    if ui.is_rect_visible(rect) {
        paint_galley(ui, &galley, rect.min, options.text_color);
    }

    let focused = response.has_focus() || response.gained_focus();
    sync_ime_output_focus(view, focused);
    store_latest_snapshot(view, &galley, row_height, false, None, 0, 0);
    view.cursor_range = None;
    view.editor_has_focus = focused;
    EditorWidgetOutcome {
        changed: false,
        focused,
        request_editor_focus: false,
        response,
    }
}

pub fn select_all_cursor(total_chars: usize) -> CursorRange {
    CursorRange::two(0, total_chars)
}

pub fn selected_text(buffer: &BufferState, cursor: CursorRange) -> Option<String> {
    let range = types::selection_char_range(&cursor)?;
    Some(buffer.document().piece_tree().extract_range(range))
}

pub fn cut_selected_text(
    buffer: &mut BufferState,
    cursor: CursorRange,
) -> Option<(CursorRange, String)> {
    (!cursor.is_empty()).then(|| editing::apply_cut(buffer, &cursor))
}

pub fn delete_selected_text(buffer: &mut BufferState, cursor: CursorRange) -> Option<CursorRange> {
    (!cursor.is_empty()).then(|| editing::apply_delete_selection(buffer, &cursor))
}

fn store_latest_snapshot(
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

fn sync_ime_output_focus(view: &mut EditorViewState, focused: bool) {
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
        ui.memory_mut(|mem| mem.set_focus_lock_filter(response.id, EDITOR_FOCUS_LOCK_FILTER));
    }
}

fn publish_active_selection(buffer: &mut BufferState, view: &EditorViewState, focused: bool) {
    if focused {
        buffer.active_selection = view
            .cursor_range
            .as_ref()
            .and_then(types::selection_char_range);
    }
}

fn should_rebuild_galley_after_input(
    changed: bool,
    pre_active_selection: Option<&std::ops::Range<usize>>,
    post_active_selection: Option<&std::ops::Range<usize>>,
    pre_cursor_range: Option<CursorRange>,
    post_cursor_range: Option<CursorRange>,
) -> bool {
    changed
        || pre_active_selection != post_active_selection
        || pre_cursor_range != post_cursor_range
}

#[cfg(test)]
mod tests {
    use super::{CharCursor, CursorRange, should_rebuild_galley_after_input};

    #[test]
    fn cursor_only_movement_rebuilds_galley_for_reveal() {
        let before = Some(CursorRange::one(CharCursor::new(5)));
        let after = Some(CursorRange::one(CharCursor::new(500)));

        assert!(should_rebuild_galley_after_input(
            false, None, None, before, after
        ));
    }

    #[test]
    fn unchanged_input_keeps_existing_galley() {
        let cursor = Some(CursorRange::one(CharCursor::new(5)));

        assert!(!should_rebuild_galley_after_input(
            false, None, None, cursor, cursor
        ));
    }
}
