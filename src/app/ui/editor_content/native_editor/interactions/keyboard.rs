use super::super::{CursorRange, cursor, editing, select_all_cursor};
use crate::app::domain::{BufferState, EditorViewState};
use eframe::egui;

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
    if let Some(handled) = handle_non_movement_key_event(event, buffer, view, cursor, total_chars) {
        return handled;
    }

    match event {
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => apply_cursor_update(
            view,
            cursor::apply_cursor_movement(
                cursor,
                *key,
                modifiers,
                galley,
                total_chars,
                buffer.document().piece_tree(),
            ),
        ),
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
    if let Some(handled) = handle_non_movement_key_event(event, buffer, view, cursor, total_chars) {
        return handled;
    }

    match event {
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => apply_cursor_update(
            view,
            cursor::apply_cursor_movement_unwrapped(
                cursor,
                *key,
                modifiers,
                total_chars,
                buffer.document().piece_tree(),
            ),
        ),
        _ => false,
    }
}

fn handle_non_movement_key_event(
    event: &egui::Event,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    total_chars: usize,
) -> Option<bool> {
    match event {
        egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            ..
        } => {
            let line_ending = buffer.document().preferred_line_ending_str().to_owned();
            Some(insert_text(buffer, view, cursor, &line_ending))
        }
        egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            modifiers,
            ..
        } if !modifiers.shift => Some(insert_text(buffer, view, cursor, "\t")),
        egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            ..
        } => Some(apply_cursor_update(
            view,
            editing::apply_outdent(buffer, cursor),
        )),
        egui::Event::Key {
            key: egui::Key::Backspace,
            pressed: true,
            modifiers,
            ..
        } => {
            view.cursor_range = Some(editing::apply_backspace(buffer, cursor, modifiers));
            Some(true)
        }
        egui::Event::Key {
            key: egui::Key::Delete,
            pressed: true,
            modifiers,
            ..
        } => {
            view.cursor_range = Some(editing::apply_delete(buffer, cursor, modifiers));
            Some(true)
        }
        egui::Event::Key {
            key: egui::Key::Z,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command && !modifiers.shift => {
            Some(apply_history(view, buffer.undo_last_text_operation()))
        }
        egui::Event::Key {
            key: egui::Key::Z | egui::Key::Y,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command && (event_key_is_y(event) || modifiers.shift) => {
            Some(apply_history(view, buffer.redo_last_text_operation()))
        }
        egui::Event::Key {
            key: egui::Key::A,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command => {
            view.cursor_range = Some(select_all_cursor(total_chars));
            Some(false)
        }
        _ => None,
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

fn apply_cursor_update(view: &mut EditorViewState, next_cursor: Option<CursorRange>) -> bool {
    next_cursor
        .map(|new_cursor| {
            view.cursor_range = Some(new_cursor);
        })
        .is_some()
}
