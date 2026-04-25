use super::super::{CursorRange, cursor, editing, select_all_cursor};
use crate::app::domain::{BufferState, EditorViewState};
use eframe::egui;

#[derive(Clone, Copy)]
struct PressedKeyEvent {
    key: egui::Key,
    modifiers: egui::Modifiers,
}

pub(super) fn handle_keyboard_events(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    galley: &egui::Galley,
    page_jump_rows: usize,
    total_chars: usize,
) -> bool {
    handle_keyboard_events_with(ui, buffer, view, |key_event, buffer, cursor| {
        cursor::apply_cursor_movement(
            cursor,
            key_event.key,
            &key_event.modifiers,
            galley,
            page_jump_rows,
            total_chars,
            buffer.document().piece_tree(),
        )
    })
}

pub(super) fn handle_keyboard_events_unwrapped(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    page_jump_rows: usize,
    total_chars: usize,
) -> bool {
    handle_keyboard_events_with(ui, buffer, view, |key_event, buffer, cursor| {
        cursor::apply_cursor_movement_unwrapped(
            cursor,
            key_event.key,
            &key_event.modifiers,
            page_jump_rows,
            total_chars,
            buffer.document().piece_tree(),
        )
    })
}

fn handle_keyboard_events_with(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    mut handle_movement_event: impl FnMut(
        PressedKeyEvent,
        &mut BufferState,
        &CursorRange,
    ) -> Option<CursorRange>,
) -> bool {
    let events = ui.input(|input| input.events.clone());
    let total_chars = buffer.document().piece_tree().len_chars();
    let mut changed = false;

    for event in &events {
        let cursor = view.cursor_range.unwrap_or_default();
        let text_changed = handle_text_event(event, buffer, view, &cursor);
        changed |= text_changed;
        if text_changed {
            continue;
        }

        if let Some(key_event) = pressed_key_event(event) {
            if let Some(handled) =
                handle_non_movement_key_event(key_event, buffer, view, &cursor, total_chars)
            {
                changed |= handled;
                continue;
            }

            if apply_cursor_update(view, handle_movement_event(key_event, buffer, &cursor)) {
                changed = true;
                continue;
            }
        }

        changed |= handle_clipboard_event(ui, event, buffer, view, &cursor);
    }

    changed
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

fn handle_non_movement_key_event(
    key_event: PressedKeyEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    total_chars: usize,
) -> Option<bool> {
    match key_event.key {
        egui::Key::Enter => {
            let line_ending = buffer.document().preferred_line_ending_str().to_owned();
            Some(insert_text(buffer, view, cursor, &line_ending))
        }
        egui::Key::Tab if !key_event.modifiers.shift => {
            Some(insert_text(buffer, view, cursor, "\t"))
        }
        egui::Key::Tab => Some(apply_cursor_update(
            view,
            editing::apply_outdent(buffer, cursor),
        )),
        egui::Key::Backspace => {
            view.cursor_range = Some(editing::apply_backspace(
                buffer,
                cursor,
                &key_event.modifiers,
            ));
            Some(true)
        }
        egui::Key::Delete => {
            view.cursor_range = Some(editing::apply_delete(buffer, cursor, &key_event.modifiers));
            Some(true)
        }
        egui::Key::Z if is_undo_shortcut(key_event.modifiers) => {
            Some(apply_history(view, buffer.undo_last_text_operation()))
        }
        egui::Key::Z | egui::Key::Y if is_redo_shortcut(key_event) => {
            Some(apply_history(view, buffer.redo_last_text_operation()))
        }
        egui::Key::A if key_event.modifiers.command => {
            view.cursor_range = Some(select_all_cursor(total_chars));
            Some(false)
        }
        _ => None,
    }
}

fn pressed_key_event(event: &egui::Event) -> Option<PressedKeyEvent> {
    match event {
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => Some(PressedKeyEvent {
            key: *key,
            modifiers: *modifiers,
        }),
        _ => None,
    }
}

fn is_undo_shortcut(modifiers: egui::Modifiers) -> bool {
    modifiers.command && !modifiers.shift
}

fn is_redo_shortcut(key_event: PressedKeyEvent) -> bool {
    key_event.modifiers.command && (key_event.key == egui::Key::Y || key_event.modifiers.shift)
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

fn copy_selection(ui: &mut egui::Ui, buffer: &BufferState, cursor: &CursorRange) {
    if !cursor.is_empty() {
        let (start, end) = cursor.sorted_indices();
        ui.copy_text(buffer.document().piece_tree().extract_range(start..end));
    }
}

fn apply_cursor_update(view: &mut EditorViewState, next_cursor: Option<CursorRange>) -> bool {
    next_cursor
        .map(|new_cursor| {
            view.cursor_range = Some(new_cursor);
        })
        .is_some()
}
