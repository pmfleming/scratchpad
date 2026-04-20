mod cursor;
mod editing;
mod highlighting;
mod types;

pub use highlighting::build_layouter;
pub use types::{
    CharCursor, CursorRange, EditOperation, EditorHighlightStyle, LayouterFn, OperationRecord,
    TextEditOptions,
};

use crate::app::domain::{
    BufferState, EditorViewState, RenderedLayout, RenderedTextWindow, SearchHighlightState,
};
use eframe::egui;
use std::sync::Arc;

const VISIBLE_ROW_OVERSCAN: usize = 2;

// ---------------------------------------------------------------------------
// Public rendering entry points
// ---------------------------------------------------------------------------

pub fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
) -> (bool, bool) {
    let text = buffer
        .document()
        .piece_tree()
        .extract_range(0..buffer.document().piece_tree().len_chars());
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

    let prev_cursor = view.cursor_range;
    handle_mouse_interaction(ui, &response, &galley, rect, view);

    let focused = response.has_focus() || response.gained_focus();

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

    (changed, focused)
}

pub fn render_editor_visible_text_window(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    options: TextEditOptions<'_>,
) -> Option<(bool, bool)> {
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
) -> bool {
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
    focused
}

// ---------------------------------------------------------------------------
// Private: mouse & keyboard handling
// ---------------------------------------------------------------------------

fn handle_mouse_interaction(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    view: &mut EditorViewState,
) {
    if response.hovered() {
        ui.output_mut(|o| o.mutable_text_under_cursor = true);
        ui.set_cursor_icon(egui::CursorIcon::Text);
    }

    let Some(pointer_pos) = response.interact_pointer_pos() else {
        return;
    };

    let cursor_at_pointer = galley.cursor_from_pos(pointer_pos - rect.min);
    let char_cursor = CharCursor {
        index: cursor_at_pointer.index,
        prefer_next_row: cursor_at_pointer.prefer_next_row,
    };

    if response.dragged() {
        // Extend selection from anchor
        if let Some(existing) = &view.cursor_range {
            view.cursor_range = Some(CursorRange {
                primary: char_cursor,
                secondary: existing.secondary,
            });
        }
    } else if response.is_pointer_button_down_on() {
        // Pointer pressed: set cursor and anchor for potential drag
        let modifiers = ui.input(|i| i.modifiers);
        if modifiers.shift {
            if let Some(existing) = &view.cursor_range {
                view.cursor_range = Some(CursorRange {
                    primary: char_cursor,
                    secondary: existing.secondary,
                });
            } else {
                view.cursor_range = Some(CursorRange::one(char_cursor));
            }
        } else {
            view.cursor_range = Some(CursorRange::one(char_cursor));
        }
    }

    // Request focus whenever pointer is actively on the widget
    if response.is_pointer_button_down_on() {
        response.request_focus();
    }
}

fn handle_keyboard_events(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    galley: &egui::Galley,
    total_chars: usize,
) -> bool {
    let events = ui.input(|i| i.events.clone());
    let cursor = view.cursor_range.unwrap_or_default();
    let mut changed = false;

    for event in &events {
        match event {
            egui::Event::Text(text_to_insert)
                if !text_to_insert.is_empty()
                    && text_to_insert != "\n"
                    && text_to_insert != "\r" =>
            {
                view.cursor_range =
                    Some(editing::apply_text_insert(buffer, &cursor, text_to_insert));
                changed = true;
            }

            egui::Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                ..
            } => {
                let line_ending = buffer.document().preferred_line_ending_str().to_owned();
                view.cursor_range = Some(editing::apply_text_insert(buffer, &cursor, &line_ending));
                changed = true;
            }

            egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.shift => {
                view.cursor_range = Some(editing::apply_text_insert(buffer, &cursor, "\t"));
                changed = true;
            }

            egui::Event::Key {
                key: egui::Key::Backspace,
                pressed: true,
                modifiers,
                ..
            } => {
                view.cursor_range = Some(editing::apply_backspace(buffer, &cursor, modifiers));
                changed = true;
            }

            egui::Event::Key {
                key: egui::Key::Delete,
                pressed: true,
                modifiers,
                ..
            } => {
                view.cursor_range = Some(editing::apply_delete(buffer, &cursor, modifiers));
                changed = true;
            }

            egui::Event::Key {
                key: egui::Key::Z,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command && !modifiers.shift => {
                if let Some(selection) = buffer.undo_last_text_operation_native() {
                    view.cursor_range = Some(selection);
                    changed = true;
                }
            }

            egui::Event::Key {
                key: egui::Key::Z | egui::Key::Y,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command && (event_key_is_y(event) || modifiers.shift) => {
                if let Some(selection) = buffer.redo_last_text_operation_native() {
                    view.cursor_range = Some(selection);
                    changed = true;
                }
            }

            egui::Event::Key {
                key: egui::Key::A,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command => {
                view.cursor_range = Some(CursorRange::two(0, total_chars));
            }

            egui::Event::Copy => {
                copy_selection(ui, buffer, &cursor);
            }

            egui::Event::Cut => {
                if !cursor.is_empty() {
                    let (new_cursor, selected) = editing::apply_cut(buffer, &cursor);
                    ui.copy_text(selected);
                    view.cursor_range = Some(new_cursor);
                    changed = true;
                }
            }

            egui::Event::Paste(text_to_paste) if !text_to_paste.is_empty() => {
                view.cursor_range =
                    Some(editing::apply_text_insert(buffer, &cursor, text_to_paste));
                changed = true;
            }

            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if let Some(new_cursor) = cursor::apply_cursor_movement(
                    &cursor,
                    *key,
                    modifiers,
                    galley,
                    total_chars,
                    buffer.document().piece_tree(),
                ) {
                    view.cursor_range = Some(new_cursor);
                }
            }

            egui::Event::Ime(egui::ImeEvent::Commit(commit_text))
                if !commit_text.is_empty() && commit_text != "\n" && commit_text != "\r" =>
            {
                view.cursor_range = Some(editing::apply_text_insert(buffer, &cursor, commit_text));
                changed = true;
            }

            _ => {}
        }
    }

    changed
}

fn event_key_is_y(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key: egui::Key::Y,
            ..
        }
    )
}

fn copy_selection(ui: &mut egui::Ui, buffer: &BufferState, cursor: &CursorRange) {
    if !cursor.is_empty() {
        let (start, end) = cursor.sorted_indices();
        let selected = buffer.document().piece_tree().extract_range(start..end);
        ui.copy_text(selected);
    }
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
) -> (bool, bool) {
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

    let text = visible_window.text.clone();
    let wrap_width = if options.word_wrap {
        ui.available_width()
    } else {
        f32::INFINITY
    };
    let galley = highlighting::build_galley(
        ui,
        &text,
        options,
        &SearchHighlightState::default(),
        window_selection,
        wrap_width,
    );

    let desired_height = visible_window.line_range.len().max(1) as f32 * row_height;
    let (rect, _response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), desired_height),
        egui::Sense::hover(),
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

    (false, false)
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
