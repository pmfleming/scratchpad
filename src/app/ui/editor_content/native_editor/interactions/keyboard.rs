use super::super::{CursorRange, cursor, editing, select_all_cursor};
use crate::app::domain::{BufferState, EditorViewState};
use eframe::egui;

#[derive(Clone, Copy, Debug)]
struct PressedKeyEvent {
    key: egui::Key,
    modifiers: egui::Modifiers,
}

#[derive(Debug)]
enum RelevantInputEvent {
    Text(String),
    Key(PressedKeyEvent),
    Copy,
    Cut,
    Paste(String),
}

pub(super) fn handle_keyboard_events(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    galley: &egui::Galley,
    page_jump_rows: usize,
    total_chars: usize,
    char_offset_base: usize,
    slice_chars: usize,
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
            char_offset_base,
            slice_chars,
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
    let events = relevant_input_events(ui);
    let total_chars = buffer.current_file_length().chars;
    let mut changed = false;

    for event in events {
        changed |= handle_relevant_input_event(
            ui,
            event,
            buffer,
            view,
            total_chars,
            &mut handle_movement_event,
        );
    }

    changed
}

fn handle_relevant_input_event(
    ui: &mut egui::Ui,
    event: RelevantInputEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    total_chars: usize,
    handle_movement_event: &mut impl FnMut(
        PressedKeyEvent,
        &mut BufferState,
        &CursorRange,
    ) -> Option<CursorRange>,
) -> bool {
    let cursor = view.cursor_range.unwrap_or_default();

    match event {
        RelevantInputEvent::Text(text) => insert_text(buffer, view, &cursor, &text),
        RelevantInputEvent::Key(key_event) => handle_key_event(
            key_event,
            buffer,
            view,
            &cursor,
            total_chars,
            handle_movement_event,
        ),
        RelevantInputEvent::Copy => {
            copy_selection(ui, buffer, &cursor);
            false
        }
        RelevantInputEvent::Cut if !cursor.is_empty() => {
            let (new_cursor, selected) = editing::apply_cut(buffer, &cursor);
            ui.copy_text(selected);
            view.set_cursor_range_anchored(buffer, new_cursor);
            true
        }
        RelevantInputEvent::Cut => false,
        RelevantInputEvent::Paste(text) => insert_text(buffer, view, &cursor, &text),
    }
}

fn handle_key_event(
    key_event: PressedKeyEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    total_chars: usize,
    handle_movement_event: &mut impl FnMut(
        PressedKeyEvent,
        &mut BufferState,
        &CursorRange,
    ) -> Option<CursorRange>,
) -> bool {
    if let Some(handled) =
        handle_non_movement_key_event(key_event, buffer, view, cursor, total_chars)
    {
        return handled;
    }

    let next_cursor = handle_movement_event(key_event, buffer, cursor);
    apply_cursor_update(view, buffer, next_cursor)
}

fn relevant_input_events(ui: &egui::Ui) -> Vec<RelevantInputEvent> {
    ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(relevant_input_event)
            .collect()
    })
}

fn relevant_input_event(event: &egui::Event) -> Option<RelevantInputEvent> {
    match event {
        egui::Event::Text(text) | egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
            is_insertable_text(text).then(|| RelevantInputEvent::Text(text.clone()))
        }
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => Some(RelevantInputEvent::Key(PressedKeyEvent {
            key: *key,
            modifiers: *modifiers,
        })),
        egui::Event::Copy => Some(RelevantInputEvent::Copy),
        egui::Event::Cut => Some(RelevantInputEvent::Cut),
        egui::Event::Paste(text) if !text.is_empty() => {
            Some(RelevantInputEvent::Paste(text.clone()))
        }
        _ => None,
    }
}

fn is_insertable_text(text: &str) -> bool {
    !text.is_empty() && text != "\n" && text != "\r"
}

fn handle_non_movement_key_event(
    key_event: PressedKeyEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    total_chars: usize,
) -> Option<bool> {
    if let Some(changed) = handle_text_key(key_event, buffer, view, cursor) {
        return Some(changed);
    }
    if let Some(changed) = handle_delete_key(key_event, buffer, view, cursor) {
        return Some(changed);
    }
    if let Some(changed) = handle_history_key(key_event, buffer, view) {
        return Some(changed);
    }
    if key_event.key == egui::Key::A && key_event.modifiers.command {
        view.set_cursor_range_anchored(buffer, select_all_cursor(total_chars));
        return Some(false);
    }

    None
}

fn handle_text_key(
    key_event: PressedKeyEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
) -> Option<bool> {
    match key_event.key {
        egui::Key::Enter => {
            let line_ending = buffer.document().preferred_line_ending_str().to_owned();
            Some(insert_text(buffer, view, cursor, &line_ending))
        }
        egui::Key::Tab => Some(handle_tab_key(key_event.modifiers, buffer, view, cursor)),
        _ => None,
    }
}

fn handle_tab_key(
    modifiers: egui::Modifiers,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
) -> bool {
    if !modifiers.shift {
        return insert_text(buffer, view, cursor, "\t");
    }

    let next_cursor = editing::apply_outdent(buffer, cursor);
    apply_cursor_update(view, buffer, next_cursor)
}

fn handle_delete_key(
    key_event: PressedKeyEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
) -> Option<bool> {
    let new_cursor = match key_event.key {
        egui::Key::Backspace => editing::apply_backspace(buffer, cursor, &key_event.modifiers),
        egui::Key::Delete => editing::apply_delete(buffer, cursor, &key_event.modifiers),
        _ => return None,
    };
    view.set_cursor_range_anchored(buffer, new_cursor);
    Some(true)
}

fn handle_history_key(
    key_event: PressedKeyEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
) -> Option<bool> {
    if key_event.key == egui::Key::Z && is_undo_shortcut(key_event.modifiers) {
        let selection = buffer.undo_last_text_operation();
        return Some(apply_history(view, buffer, selection));
    }
    if matches!(key_event.key, egui::Key::Z | egui::Key::Y) && is_redo_shortcut(key_event) {
        let selection = buffer.redo_last_text_operation();
        return Some(apply_history(view, buffer, selection));
    }

    None
}

fn is_undo_shortcut(modifiers: egui::Modifiers) -> bool {
    modifiers.command && !modifiers.shift
}

fn is_redo_shortcut(key_event: PressedKeyEvent) -> bool {
    key_event.modifiers.command && (key_event.key == egui::Key::Y || key_event.modifiers.shift)
}

fn insert_text(
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    text: &str,
) -> bool {
    let new_cursor = editing::apply_text_insert(buffer, cursor, text);
    view.set_cursor_range_anchored(buffer, new_cursor);
    true
}

fn apply_history(
    view: &mut EditorViewState,
    buffer: &mut BufferState,
    selection: Option<CursorRange>,
) -> bool {
    if let Some(selection) = selection {
        view.set_cursor_range_anchored(buffer, selection);
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

fn apply_cursor_update(
    view: &mut EditorViewState,
    buffer: &mut BufferState,
    next_cursor: Option<CursorRange>,
) -> bool {
    if let Some(new_cursor) = next_cursor {
        view.set_cursor_range_anchored(buffer, new_cursor);
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{
        PressedKeyEvent, RelevantInputEvent, apply_cursor_update, handle_key_event,
        relevant_input_event,
    };
    use crate::app::domain::{BufferState, EditorViewState};
    use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};
    use eframe::egui;

    #[test]
    fn cursor_only_keyboard_movement_does_not_mark_document_changed() {
        let mut buffer = BufferState::new("test.txt".to_owned(), "alpha\nbeta".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor = CursorRange::one(CharCursor::new(0));
        let next_cursor = CursorRange::one(CharCursor::new(6));
        let total_chars = buffer.current_file_length().chars;
        let mut movement = |_: PressedKeyEvent,
                            _: &mut BufferState,
                            _: &CursorRange|
         -> Option<CursorRange> { Some(next_cursor) };

        let changed = handle_key_event(
            PressedKeyEvent {
                key: egui::Key::ArrowDown,
                modifiers: egui::Modifiers::default(),
            },
            &mut buffer,
            &mut view,
            &cursor,
            total_chars,
            &mut movement,
        );

        assert!(!changed);
        assert_eq!(view.cursor_range, Some(next_cursor));
    }

    #[test]
    fn text_keyboard_input_still_marks_document_changed() {
        let mut buffer = BufferState::new("test.txt".to_owned(), "alpha".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor = CursorRange::one(CharCursor::new(5));

        let changed = match relevant_input_event(&egui::Event::Text("!".to_owned())) {
            Some(RelevantInputEvent::Text(text)) => {
                super::insert_text(&mut buffer, &mut view, &cursor, &text)
            }
            other => panic!("expected text event, got {other:?}"),
        };

        assert!(changed);
        assert_eq!(
            view.cursor_range,
            Some(CursorRange::one(CharCursor::new(6)))
        );
    }

    #[test]
    fn relevant_input_event_ignores_non_insertable_text() {
        assert!(relevant_input_event(&egui::Event::Text("\n".to_owned())).is_none());
    }

    #[test]
    fn cursor_update_helper_reports_no_document_change() {
        let mut buffer = BufferState::new("test.txt".to_owned(), "alpha".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);
        let next_cursor = CursorRange::one(CharCursor::new(3));

        assert!(!apply_cursor_update(
            &mut view,
            &mut buffer,
            Some(next_cursor)
        ));
        assert_eq!(view.cursor_range, Some(next_cursor));
    }
}
