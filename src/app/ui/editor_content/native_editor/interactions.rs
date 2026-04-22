use super::{CharCursor, CursorRange, cursor, editing, select_all_cursor, word_boundary};
use crate::app::domain::{BufferState, EditorViewState};
use eframe::egui;

const MULTI_CLICK_MAX_DELAY: f64 = 0.4;
const MULTI_CLICK_MAX_DISTANCE: f32 = 4.0;

#[derive(Clone, Default)]
struct ClickState {
    last_click_time: f64,
    last_click_pos: egui::Pos2,
    click_count: u32,
    was_primary_pointer_down: bool,
}

#[derive(Clone, Copy)]
struct WindowClickSelection {
    cursor_at_pointer: egui::text::CCursor,
    char_cursor: CharCursor,
    char_offset_base: usize,
}

pub(super) fn handle_mouse_interaction(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
) {
    if response.hovered() {
        ui.output_mut(|output| output.mutable_text_under_cursor = true);
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

    let click_id = response.id.with("click_state");
    let mut click_state: ClickState = ui
        .data_mut(|data| data.get_temp(click_id))
        .unwrap_or_default();

    let secondary_pointer_down = response.contains_pointer()
        && ui.input(|input| input.pointer.button_down(egui::PointerButton::Secondary));
    if secondary_pointer_down || response.secondary_clicked() {
        click_state.was_primary_pointer_down = false;
        ui.data_mut(|data| data.insert_temp(click_id, click_state));
        return;
    }

    let primary_pointer_down = response.contains_pointer()
        && ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary));
    let is_new_primary_press = primary_pointer_down && !click_state.was_primary_pointer_down;

    if primary_pointer_down && response.dragged() {
        extend_selection_to_cursor(view, char_cursor);
    } else if is_new_primary_press {
        update_click_count(ui, pointer_pos, &mut click_state);
        apply_click_selection(
            ui,
            view,
            piece_tree,
            galley,
            cursor_at_pointer,
            char_cursor,
            click_state.click_count,
        );
    }

    click_state.was_primary_pointer_down = primary_pointer_down;
    ui.data_mut(|data| data.insert_temp(click_id, click_state));

    if primary_pointer_down {
        response.request_focus();
    }
}

pub(super) fn handle_mouse_interaction_window(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    char_offset_base: usize,
) {
    if response.hovered() {
        ui.output_mut(|output| output.mutable_text_under_cursor = true);
        ui.set_cursor_icon(egui::CursorIcon::Text);
    }

    let Some(pointer_pos) = response.interact_pointer_pos() else {
        return;
    };

    let cursor_at_pointer = galley.cursor_from_pos(pointer_pos - rect.min);
    let char_cursor = CharCursor {
        index: char_offset_base + cursor_at_pointer.index,
        prefer_next_row: cursor_at_pointer.prefer_next_row,
    };

    let click_id = response.id.with("click_state");
    let mut click_state: ClickState = ui
        .data_mut(|data| data.get_temp(click_id))
        .unwrap_or_default();

    let secondary_pointer_down = response.contains_pointer()
        && ui.input(|input| input.pointer.button_down(egui::PointerButton::Secondary));
    if secondary_pointer_down || response.secondary_clicked() {
        click_state.was_primary_pointer_down = false;
        ui.data_mut(|data| data.insert_temp(click_id, click_state));
        return;
    }

    let primary_pointer_down = response.contains_pointer()
        && ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary));
    let is_new_primary_press = primary_pointer_down && !click_state.was_primary_pointer_down;

    if primary_pointer_down && response.dragged() {
        extend_selection_to_cursor(view, char_cursor);
    } else if is_new_primary_press {
        update_click_count(ui, pointer_pos, &mut click_state);
        apply_click_selection_window(
            ui,
            view,
            piece_tree,
            galley,
            click_state.click_count,
            WindowClickSelection {
                cursor_at_pointer,
                char_cursor,
                char_offset_base,
            },
        );
    }

    click_state.was_primary_pointer_down = primary_pointer_down;
    ui.data_mut(|data| data.insert_temp(click_id, click_state));

    if primary_pointer_down {
        response.request_focus();
    }
}

pub(super) fn handle_keyboard_events(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    galley: &egui::Galley,
    total_chars: usize,
) -> bool {
    let events = ui.input(|input| input.events.clone());
    let mut changed = false;

    for event in &events {
        let cursor = view.cursor_range.unwrap_or_default();
        let text_changed = handle_text_event(event, buffer, view, &cursor);
        changed |= text_changed;
        if text_changed {
            continue;
        }

        if handle_key_event(event, buffer, view, &cursor, galley, total_chars) {
            changed = true;
            continue;
        }

        changed |= handle_clipboard_event(ui, event, buffer, view, &cursor);
    }

    changed
}

pub(super) fn handle_keyboard_events_unwrapped(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    total_chars: usize,
) -> bool {
    let events = ui.input(|input| input.events.clone());
    let mut changed = false;

    for event in &events {
        let cursor = view.cursor_range.unwrap_or_default();
        let text_changed = handle_text_event(event, buffer, view, &cursor);
        changed |= text_changed;
        if text_changed {
            continue;
        }

        if handle_key_event_unwrapped(event, buffer, view, &cursor, total_chars) {
            changed = true;
            continue;
        }

        changed |= handle_clipboard_event(ui, event, buffer, view, &cursor);
    }

    changed
}

pub(super) fn sync_view_cursor_before_render(view: &mut EditorViewState, focused: bool) {
    if let Some(cursor_range) = view.pending_cursor_range.take() {
        view.cursor_range = Some(cursor_range);
        view.scroll_to_cursor = true;
    } else if focused && view.cursor_range.is_none() {
        view.cursor_range = Some(CursorRange::one(CharCursor::new(0)));
        view.scroll_to_cursor = true;
    }
}

fn extend_selection_to_cursor(view: &mut EditorViewState, char_cursor: CharCursor) {
    if let Some(existing) = &view.cursor_range {
        view.cursor_range = Some(CursorRange {
            primary: char_cursor,
            secondary: existing.secondary,
        });
    }
}

fn update_click_count(ui: &egui::Ui, pointer_pos: egui::Pos2, click_state: &mut ClickState) {
    let now = ui.input(|input| input.time);
    let is_repeat = (now - click_state.last_click_time) < MULTI_CLICK_MAX_DELAY
        && (pointer_pos - click_state.last_click_pos).length() < MULTI_CLICK_MAX_DISTANCE;

    click_state.click_count = if is_repeat {
        click_state.click_count + 1
    } else {
        1
    };
    click_state.last_click_time = now;
    click_state.last_click_pos = pointer_pos;
}

fn apply_click_selection(
    ui: &egui::Ui,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    galley: &egui::Galley,
    cursor_at_pointer: egui::text::CCursor,
    char_cursor: CharCursor,
    click_count: u32,
) {
    match click_count {
        2 => {
            let start = word_boundary::word_start(piece_tree, char_cursor.index);
            let end = word_boundary::word_end(piece_tree, char_cursor.index);
            view.cursor_range = Some(CursorRange::two(start, end));
        }
        n if n >= 3 => {
            let row_start = galley.cursor_begin_of_row(&cursor_at_pointer);
            let row_end = galley.cursor_end_of_row(&cursor_at_pointer);
            view.cursor_range = Some(CursorRange {
                primary: CharCursor {
                    index: row_end.index,
                    prefer_next_row: row_end.prefer_next_row,
                },
                secondary: CharCursor {
                    index: row_start.index,
                    prefer_next_row: row_start.prefer_next_row,
                },
            });
        }
        _ => apply_single_click(ui, view, char_cursor),
    }
}

fn apply_click_selection_window(
    ui: &egui::Ui,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    galley: &egui::Galley,
    click_count: u32,
    selection: WindowClickSelection,
) {
    match click_count {
        2 => {
            let start = word_boundary::word_start(piece_tree, selection.char_cursor.index);
            let end = word_boundary::word_end(piece_tree, selection.char_cursor.index);
            view.cursor_range = Some(CursorRange::two(start, end));
        }
        n if n >= 3 => {
            let row_start = galley.cursor_begin_of_row(&selection.cursor_at_pointer);
            let row_end = galley.cursor_end_of_row(&selection.cursor_at_pointer);
            view.cursor_range = Some(CursorRange {
                primary: CharCursor {
                    index: selection.char_offset_base + row_end.index,
                    prefer_next_row: row_end.prefer_next_row,
                },
                secondary: CharCursor {
                    index: selection.char_offset_base + row_start.index,
                    prefer_next_row: row_start.prefer_next_row,
                },
            });
        }
        _ => apply_single_click(ui, view, selection.char_cursor),
    }
}

fn apply_single_click(ui: &egui::Ui, view: &mut EditorViewState, char_cursor: CharCursor) {
    let shift = ui.input(|input| input.modifiers.shift);
    if shift {
        extend_selection_to_cursor(view, char_cursor);
        if view.cursor_range.is_none() {
            view.cursor_range = Some(CursorRange::one(char_cursor));
        }
    } else {
        view.cursor_range = Some(CursorRange::one(char_cursor));
    }
}

fn handle_text_event(
    event: &egui::Event,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
) -> bool {
    match event {
        egui::Event::Text(text_to_insert)
            if !text_to_insert.is_empty() && text_to_insert != "\n" && text_to_insert != "\r" =>
        {
            view.cursor_range = Some(editing::apply_text_insert(buffer, cursor, text_to_insert));
            true
        }
        egui::Event::Ime(egui::ImeEvent::Commit(commit_text))
            if !commit_text.is_empty() && commit_text != "\n" && commit_text != "\r" =>
        {
            view.cursor_range = Some(editing::apply_text_insert(buffer, cursor, commit_text));
            true
        }
        _ => false,
    }
}

fn handle_key_event(
    event: &egui::Event,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    galley: &egui::Galley,
    total_chars: usize,
) -> bool {
    match event {
        egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            ..
        } => {
            let line_ending = buffer.document().preferred_line_ending_str().to_owned();
            insert_text(buffer, view, cursor, &line_ending)
        }

        egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            modifiers,
            ..
        } if !modifiers.shift => insert_text(buffer, view, cursor, "\t"),

        egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            modifiers,
            ..
        } if modifiers.shift => editing::apply_outdent(buffer, cursor)
            .map(|new_cursor| {
                view.cursor_range = Some(new_cursor);
            })
            .is_some(),

        egui::Event::Key {
            key: egui::Key::Backspace,
            pressed: true,
            modifiers,
            ..
        } => {
            view.cursor_range = Some(editing::apply_backspace(buffer, cursor, modifiers));
            true
        }

        egui::Event::Key {
            key: egui::Key::Delete,
            pressed: true,
            modifiers,
            ..
        } => {
            view.cursor_range = Some(editing::apply_delete(buffer, cursor, modifiers));
            true
        }

        egui::Event::Key {
            key: egui::Key::Z,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command && !modifiers.shift => {
            apply_history(view, buffer.undo_last_text_operation())
        }

        egui::Event::Key {
            key: egui::Key::Z | egui::Key::Y,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command && (event_key_is_y(event) || modifiers.shift) => {
            apply_history(view, buffer.redo_last_text_operation())
        }

        egui::Event::Key {
            key: egui::Key::A,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command => {
            view.cursor_range = Some(select_all_cursor(total_chars));
            false
        }

        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => cursor::apply_cursor_movement(
            cursor,
            *key,
            modifiers,
            galley,
            total_chars,
            buffer.document().piece_tree(),
        )
        .map(|new_cursor| {
            view.cursor_range = Some(new_cursor);
        })
        .is_some(),

        _ => false,
    }
}

fn handle_key_event_unwrapped(
    event: &egui::Event,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    total_chars: usize,
) -> bool {
    match event {
        egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            ..
        } => {
            let line_ending = buffer.document().preferred_line_ending_str().to_owned();
            insert_text(buffer, view, cursor, &line_ending)
        }

        egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            modifiers,
            ..
        } if !modifiers.shift => insert_text(buffer, view, cursor, "\t"),

        egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            modifiers,
            ..
        } if modifiers.shift => editing::apply_outdent(buffer, cursor)
            .map(|new_cursor| {
                view.cursor_range = Some(new_cursor);
            })
            .is_some(),

        egui::Event::Key {
            key: egui::Key::Backspace,
            pressed: true,
            modifiers,
            ..
        } => {
            view.cursor_range = Some(editing::apply_backspace(buffer, cursor, modifiers));
            true
        }

        egui::Event::Key {
            key: egui::Key::Delete,
            pressed: true,
            modifiers,
            ..
        } => {
            view.cursor_range = Some(editing::apply_delete(buffer, cursor, modifiers));
            true
        }

        egui::Event::Key {
            key: egui::Key::Z,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command && !modifiers.shift => {
            apply_history(view, buffer.undo_last_text_operation())
        }

        egui::Event::Key {
            key: egui::Key::Z | egui::Key::Y,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command && (event_key_is_y(event) || modifiers.shift) => {
            apply_history(view, buffer.redo_last_text_operation())
        }

        egui::Event::Key {
            key: egui::Key::A,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command => {
            view.cursor_range = Some(select_all_cursor(total_chars));
            false
        }

        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => cursor::apply_cursor_movement_unwrapped(
            cursor,
            *key,
            modifiers,
            total_chars,
            buffer.document().piece_tree(),
        )
        .map(|new_cursor| {
            view.cursor_range = Some(new_cursor);
        })
        .is_some(),

        _ => false,
    }
}

fn handle_clipboard_event(
    ui: &mut egui::Ui,
    event: &egui::Event,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
) -> bool {
    match event {
        egui::Event::Copy => {
            copy_selection(ui, buffer, cursor);
            false
        }
        egui::Event::Cut if !cursor.is_empty() => {
            let (new_cursor, selected) = editing::apply_cut(buffer, cursor);
            ui.copy_text(selected);
            view.cursor_range = Some(new_cursor);
            true
        }
        egui::Event::Paste(text_to_paste) if !text_to_paste.is_empty() => {
            view.cursor_range = Some(editing::apply_text_insert(buffer, cursor, text_to_paste));
            true
        }
        _ => false,
    }
}

fn insert_text(
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    text: &str,
) -> bool {
    view.cursor_range = Some(editing::apply_text_insert(buffer, cursor, text));
    true
}

fn apply_history(view: &mut EditorViewState, selection: Option<CursorRange>) -> bool {
    if let Some(selection) = selection {
        view.cursor_range = Some(selection);
        true
    } else {
        false
    }
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
        ui.copy_text(buffer.document().piece_tree().extract_range(start..end));
    }
}
