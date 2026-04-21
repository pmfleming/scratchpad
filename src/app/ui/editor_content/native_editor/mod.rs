mod cursor;
mod editing;
mod highlighting;
mod interactions;
mod types;
mod word_boundary;

pub use highlighting::build_layouter;
pub use types::{
    CharCursor, CursorRange, EditOperation, EditorHighlightStyle, LayouterFn, OperationRecord,
    TextEditOptions,
};

use crate::app::domain::{
    BufferState, EditorViewState, RenderedLayout, RenderedTextWindow, SearchHighlightState,
};
use eframe::egui;
use interactions::{
    handle_keyboard_events, handle_mouse_interaction, sync_view_cursor_before_render,
};
use std::sync::Arc;

const VISIBLE_ROW_OVERSCAN: usize = 2;
const EDITOR_FOCUS_LOCK_FILTER: egui::EventFilter = egui::EventFilter {
    horizontal_arrows: true,
    vertical_arrows: true,
    tab: false,
    escape: false,
};

pub struct EditorWidgetOutcome {
    pub changed: bool,
    pub focused: bool,
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
) -> EditorWidgetOutcome {
    let text = buffer.document().extract_text();
    let total_chars = buffer.document().piece_tree().len_chars();

    let selection_range = buffer.active_selection.clone();

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

    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let desired_rows = buffer.line_count.max(1);
    let visible_height = ui.available_height();
    let bottom_padding = visible_height * 0.5;
    let desired_height = desired_rows as f32 * row_height + bottom_padding;
    let desired_width = wrap_width;

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            desired_width.min(ui.available_width()),
            desired_height.max(ui.available_height()),
        ),
        egui::Sense::click_and_drag(),
    );

    if options.request_focus {
        response.request_focus();
    }

    if response.has_focus() {
        ui.memory_mut(|mem| mem.set_focus_lock_filter(response.id, EDITOR_FOCUS_LOCK_FILTER));
    }

    let prev_cursor = view.cursor_range;
    handle_mouse_interaction(
        ui,
        &response,
        &galley,
        rect,
        view,
        buffer.document().piece_tree(),
    );

    let focused = response.has_focus() || response.gained_focus() || options.request_focus;
    sync_view_cursor_before_render(view, focused);

    let changed = if focused {
        handle_keyboard_events(ui, buffer, view, &galley, total_chars)
    } else {
        false
    };

    if view.cursor_range != prev_cursor {
        view.scroll_to_cursor = true;
    }

    // Publish active view's selection to the buffer so all views can show it
    if focused {
        buffer.active_selection = view
            .cursor_range
            .as_ref()
            .and_then(types::selection_char_range);
    }

    let galley_pos = rect.min;
    if ui.is_rect_visible(rect) {
        paint_editor(
            ui, &galley, galley_pos, rect, view, options, focused, changed,
        );
    }

    // Consume scroll flag once the galley is fresh (scroll was applied)
    if !changed {
        view.scroll_to_cursor = false;
    }

    update_visible_layout(&galley, galley_pos, rect, buffer, view);

    if changed {
        buffer.refresh_text_metadata();
    }

    EditorWidgetOutcome {
        changed,
        focused,
        response,
    }
}

pub fn render_editor_visible_text_window(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    options: TextEditOptions<'_>,
) -> Option<EditorWidgetOutcome> {
    if options.word_wrap || options.request_focus {
        return None;
    }

    let visible_lines = previous_layout?.visible_line_range();
    if visible_lines.is_empty() {
        return None;
    }

    let visible_window = buffer.visible_line_window(visible_lines);
    Some(render_visible_text_window(
        ui,
        view,
        visible_window,
        options,
        buffer.line_count,
        buffer.active_selection.as_ref(),
    ))
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

    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let desired_height = desired_rows.max(1) as f32 * row_height;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), desired_height),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        paint_galley(ui, &galley, rect.min, options.text_color);
    }

    let focused = response.has_focus() || response.gained_focus();
    view.latest_layout = Some(RenderedLayout::from_galley(galley));
    view.cursor_range = None;
    EditorWidgetOutcome {
        changed: false,
        focused,
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

// ---------------------------------------------------------------------------
// Private: painting
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn paint_editor(
    ui: &mut egui::Ui,
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    view: &EditorViewState,
    options: TextEditOptions<'_>,
    focused: bool,
    changed: bool,
) {
    let painter = ui.painter_at(rect.expand(1.0));

    // Paint galley — selection highlight is already baked into the LayoutJob
    paint_galley(ui, galley, galley_pos, options.text_color);

    if !focused {
        return;
    }

    if let Some(cursor_range) = &view.cursor_range
        && !changed
    {
        // Paint cursor (skip when changed — galley is stale, next frame corrects it)
        let cursor_rect = cursor_rect_at(galley, galley_pos, cursor_range.primary);
        let stroke = ui.visuals().text_cursor.stroke;
        painter.line_segment(
            [cursor_rect.center_top(), cursor_rect.center_bottom()],
            (stroke.width, stroke.color),
        );

        // Scroll to cursor only when it moved
        if view.scroll_to_cursor {
            ui.scroll_to_rect(cursor_rect, None);
        }

        // IME output
        let to_global = ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default();
        ui.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                rect: to_global * rect,
                cursor_rect: to_global * cursor_rect,
            });
        });
    }
}

fn paint_galley(
    ui: &egui::Ui,
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    text_color: egui::Color32,
) {
    let offset = galley_pos - egui::vec2(galley.rect.left(), 0.0);
    ui.painter().galley(offset, galley.clone(), text_color);
}

fn cursor_rect_at(galley: &egui::Galley, galley_pos: egui::Pos2, cursor: CharCursor) -> egui::Rect {
    galley
        .pos_from_cursor(cursor.to_egui_ccursor())
        .expand(1.5)
        .translate(galley_pos.to_vec2())
}

// ---------------------------------------------------------------------------
// Private: visible text window
// ---------------------------------------------------------------------------

fn render_visible_text_window(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    mut visible_window: RenderedTextWindow,
    options: TextEditOptions<'_>,
    total_line_count: usize,
    active_selection: Option<&std::ops::Range<usize>>,
) -> EditorWidgetOutcome {
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let top_padding_lines = visible_window.layout_row_offset;
    let bottom_padding_lines = total_line_count.saturating_sub(visible_window.line_range.end);

    if top_padding_lines > 0 {
        ui.add_space(row_height * top_padding_lines as f32);
    }

    // Map buffer-level selection into window-local char offsets
    let window_selection = active_selection.and_then(|sel| {
        let win = &visible_window.char_range;
        let start = sel.start.max(win.start).saturating_sub(win.start);
        let end = sel.end.min(win.end).saturating_sub(win.start);
        (start < end).then_some(start..end)
    });

    let wrap_width = if options.word_wrap {
        ui.available_width()
    } else {
        f32::INFINITY
    };
    let galley = highlighting::build_galley(
        ui,
        &visible_window.text,
        options,
        &SearchHighlightState::default(),
        window_selection,
        wrap_width,
    );

    let desired_height = visible_window.line_range.len().max(1) as f32 * row_height;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), desired_height),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        paint_galley(ui, &galley, rect.min, options.text_color);
    }

    let mut latest_layout = Some(RenderedLayout::from_galley(galley));
    if let Some(layout) = latest_layout.as_mut() {
        layout.offset_line_numbers(visible_window.line_range.start);
        visible_window.row_range = 0..layout.row_count();
        layout.set_visible_text(visible_window);
    }
    view.latest_layout = latest_layout;

    if bottom_padding_lines > 0 {
        ui.add_space(row_height * bottom_padding_lines as f32);
    }

    EditorWidgetOutcome {
        changed: false,
        focused: false,
        response,
    }
}

// ---------------------------------------------------------------------------
// Private: layout helpers
// ---------------------------------------------------------------------------

fn update_visible_layout(
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
) {
    let visible_row_range = visible_row_range_for_galley(galley, galley_pos, rect);

    let mut latest_layout = Some(RenderedLayout::from_galley(galley.clone()));
    if let (Some(layout), Some(visible_row_range)) = (latest_layout.as_mut(), visible_row_range)
        && let Some(char_range) = layout.char_range_for_rows(visible_row_range.clone())
    {
        let visible_text =
            buffer.visible_text_window(visible_row_range, char_range, layout.row_count());
        layout.set_visible_text(visible_text);
    }
    view.latest_layout = latest_layout;
}

fn visible_row_range_for_galley(
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    clip_rect: egui::Rect,
) -> Option<std::ops::Range<usize>> {
    let first_visible = galley
        .rows
        .iter()
        .position(|row| galley_pos.y + row.max_y() >= clip_rect.top())?;
    let last_visible = galley
        .rows
        .iter()
        .rposition(|row| galley_pos.y + row.min_y() <= clip_rect.bottom())
        .unwrap_or(first_visible);
    let start = first_visible.saturating_sub(VISIBLE_ROW_OVERSCAN);
    let end = (last_visible + 1 + VISIBLE_ROW_OVERSCAN).min(galley.rows.len());
    Some(start..end)
}

#[cfg(test)]
mod tests {
    use super::{CharCursor, CursorRange, sync_view_cursor_before_render};
    use crate::app::domain::EditorViewState;

    #[test]
    fn focused_editor_without_cursor_starts_at_document_beginning() {
        let mut view = EditorViewState::new(1, false);

        sync_view_cursor_before_render(&mut view, true);

        assert_eq!(
            view.cursor_range,
            Some(CursorRange::one(CharCursor::new(0)))
        );
        assert!(view.scroll_to_cursor);
    }

    #[test]
    fn pending_cursor_range_overrides_missing_native_editor_cursor() {
        let mut view = EditorViewState::new(1, false);
        let pending = CursorRange::one(CharCursor::new(7));
        view.pending_cursor_range = Some(pending);

        sync_view_cursor_before_render(&mut view, true);

        assert_eq!(view.cursor_range, Some(pending));
        assert_eq!(view.pending_cursor_range, None);
        assert!(view.scroll_to_cursor);
    }
}
