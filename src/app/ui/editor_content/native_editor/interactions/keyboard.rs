use super::super::{CursorRange, cursor, editing, select_all_cursor};
use super::KeyboardInputRequest;
use crate::app::domain::{BufferState, EditorViewState, ImePreeditState, PieceSource};
use crate::app::services::settings_store::IndentationStyle;
use eframe::egui;

#[derive(Clone, Copy)]
struct IndentationInput {
    style: IndentationStyle,
    width: u8,
}

#[derive(Clone, Copy)]
struct KeyboardInputContext {
    total_chars: usize,
    indentation: IndentationInput,
}

#[derive(Clone, Copy, Debug)]
struct PressedKeyEvent {
    key: egui::Key,
    modifiers: egui::Modifiers,
}

#[derive(Debug)]
enum RelevantInputEvent {
    Text(String),
    ImePreedit {
        text: String,
        active_range_chars: Option<std::ops::Range<usize>>,
    },

    Key(PressedKeyEvent),
    Copy,
    Cut,
    Paste(String),
}

pub(super) fn handle_keyboard_events(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    request: KeyboardInputRequest<'_>,
) -> bool {
    handle_keyboard_events_with(
        ui,
        buffer,
        view,
        IndentationInput {
            style: request.indentation_style,
            width: request.indentation_width,
        },
        |key_event, buffer, cursor| {
            cursor::apply_cursor_movement(cursor::CursorMovementRequest {
                cursor,
                key: key_event.key,
                modifiers: &key_event.modifiers,
                galley: request.galley,
                page_jump_rows: request.page_jump_rows,
                total_chars: request.total_chars,
                piece_tree: buffer.document().piece_tree(),
                char_offset_base: request.char_offset_base,
                slice_chars: request.slice_chars,
                display_map: request.display_map,
            })
        },
    )
}

fn handle_keyboard_events_with(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    indentation: IndentationInput,
    mut handle_movement_event: impl FnMut(
        PressedKeyEvent,
        &mut BufferState,
        &CursorRange,
    ) -> Option<CursorRange>,
) -> bool {
    let events = relevant_input_events(ui);
    let context = KeyboardInputContext {
        total_chars: buffer.current_file_length().chars,
        indentation,
    };
    let mut changed = false;

    for event in events {
        changed |= handle_relevant_input_event(
            ui,
            event,
            buffer,
            view,
            context,
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
    context: KeyboardInputContext,
    handle_movement_event: &mut impl FnMut(
        PressedKeyEvent,
        &mut BufferState,
        &CursorRange,
    ) -> Option<CursorRange>,
) -> bool {
    let cursor = view.cursor_range.unwrap_or_default();

    match event {
        RelevantInputEvent::Text(text) => insert_text(buffer, view, &cursor, &text),
        RelevantInputEvent::ImePreedit {
            text,
            active_range_chars,
        } => {
            view.ime_preedit =
                (!text.is_empty()).then(|| ImePreeditState::new(text, active_range_chars));
            false
        }
        RelevantInputEvent::Key(key_event) => handle_key_event(
            ui,
            key_event,
            buffer,
            view,
            &cursor,
            context,
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
        RelevantInputEvent::Paste(text) => {
            insert_text_with_source(buffer, view, &cursor, &text, PieceSource::Paste)
        }
    }
}

fn handle_key_event(
    ui: &mut egui::Ui,
    key_event: PressedKeyEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    context: KeyboardInputContext,
    handle_movement_event: &mut impl FnMut(
        PressedKeyEvent,
        &mut BufferState,
        &CursorRange,
    ) -> Option<CursorRange>,
) -> bool {
    if let Some(handled) =
        handle_non_movement_key_event(ui, key_event, buffer, view, cursor, context)
    {
        return handled;
    }

    let next_cursor = handle_movement_event(key_event, buffer, cursor);
    apply_cursor_update(view, buffer, next_cursor)
}

fn relevant_input_events(ui: &egui::Ui) -> Vec<RelevantInputEvent> {
    // Collect while the input lock is held, then consult context data after it
    // has been released. Both `Context::input` and `Context::data` lock the
    // same egui context, so looking up the runtime keymap inside `ui.input`
    // deadlocks the UI thread as soon as a key event arrives.
    let mut events: Vec<_> = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(relevant_input_event)
            .collect()
    });
    events.retain(|event| {
        !matches!(
            event,
            RelevantInputEvent::Key(key_event)
                if crate::app::shortcut_tooltips::is_app_shortcut(
                    ui.ctx(),
                    key_event.modifiers,
                    key_event.key,
                )
        )
    });
    events
}

fn relevant_input_event(event: &egui::Event) -> Option<RelevantInputEvent> {
    match event {
        egui::Event::Text(text) => {
            is_insertable_text(text).then(|| RelevantInputEvent::Text(text.clone()))
        }
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text,
            active_range_chars,
        }) => Some(RelevantInputEvent::ImePreedit {
            text: text.clone(),
            active_range_chars: active_range_chars.clone(),
        }),
        egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
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
    ui: &mut egui::Ui,
    key_event: PressedKeyEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    context: KeyboardInputContext,
) -> Option<bool> {
    if let Some(changed) = handle_text_key(key_event, buffer, view, cursor, context.indentation) {
        return Some(changed);
    }
    if let Some(changed) = handle_history_key(key_event, buffer, view) {
        return Some(changed);
    }
    if let Some(changed) = handle_insert_key(ui, key_event, buffer, cursor) {
        return Some(changed);
    }
    if let Some(changed) = handle_delete_key(key_event, buffer, view, cursor) {
        return Some(changed);
    }
    if key_event.key == egui::Key::A && key_event.modifiers.command {
        view.set_cursor_range_anchored(buffer, select_all_cursor(context.total_chars));
        return Some(false);
    }

    None
}

fn handle_text_key(
    key_event: PressedKeyEvent,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    indentation: IndentationInput,
) -> Option<bool> {
    match key_event.key {
        egui::Key::Enter => {
            let line_ending = buffer.document().preferred_line_ending_str().to_owned();
            Some(insert_text(buffer, view, cursor, &line_ending))
        }
        egui::Key::Tab => Some(handle_tab_key(
            key_event.modifiers,
            buffer,
            view,
            cursor,
            indentation,
        )),
        _ => None,
    }
}

fn handle_tab_key(
    modifiers: egui::Modifiers,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    indentation: IndentationInput,
) -> bool {
    if !modifiers.shift {
        let next_cursor =
            editing::apply_indent(buffer, cursor, indentation.style, indentation.width);
        view.set_cursor_range_anchored(buffer, next_cursor);
        return true;
    }

    let next_cursor = editing::apply_outdent(buffer, cursor, indentation.width);
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
    if (key_event.key == egui::Key::Z && is_undo_shortcut(key_event.modifiers))
        || is_classic_undo_shortcut(key_event)
    {
        let selection = buffer.undo_last_text_operation();
        return Some(apply_history(view, buffer, selection));
    }
    if key_event.key == egui::Key::Y && is_redo_shortcut(key_event.modifiers) {
        let selection = buffer.redo_last_text_operation();
        return Some(apply_history(view, buffer, selection));
    }

    None
}

fn is_undo_shortcut(modifiers: egui::Modifiers) -> bool {
    modifiers.command && !modifiers.shift
}

fn is_classic_undo_shortcut(key_event: PressedKeyEvent) -> bool {
    key_event.key == egui::Key::Backspace
        && key_event.modifiers.alt
        && !key_event.modifiers.ctrl
        && !key_event.modifiers.shift
}

fn handle_insert_key(
    ui: &mut egui::Ui,
    key_event: PressedKeyEvent,
    buffer: &BufferState,
    cursor: &CursorRange,
) -> Option<bool> {
    if key_event.key != egui::Key::Insert {
        return None;
    }
    if key_event.modifiers.ctrl && !key_event.modifiers.shift {
        copy_selection(ui, buffer, cursor);
        return Some(false);
    }
    if key_event.modifiers.shift && !key_event.modifiers.ctrl {
        ui.ctx()
            .clone()
            .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        return Some(false);
    }
    None
}

fn is_redo_shortcut(modifiers: egui::Modifiers) -> bool {
    modifiers.command && !modifiers.shift
}

fn insert_text(
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    text: &str,
) -> bool {
    insert_text_with_source(buffer, view, cursor, text, PieceSource::Edit)
}

fn insert_text_with_source(
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    cursor: &CursorRange,
    text: &str,
    source: PieceSource,
) -> bool {
    view.ime_preedit = None;
    let new_cursor = editing::apply_text_insert_with_source(buffer, cursor, text, source);
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
        PressedKeyEvent, RelevantInputEvent, is_classic_undo_shortcut, relevant_input_event,
        relevant_input_events,
    };
    use crate::app::platform::PlatformProfile;
    use crate::app::services::settings_store::ShortcutSettings;
    use eframe::egui;

    #[test]
    fn ime_preedit_event_is_kept_separate_from_committed_text() {
        let event = egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "kana".to_owned(),
            active_range_chars: Some(1..3),
        });

        assert!(matches!(
            relevant_input_event(&event),
            Some(RelevantInputEvent::ImePreedit { text, active_range_chars })
                if text == "kana" && active_range_chars == Some(1..3)
        ));
    }

    #[test]
    fn ime_commit_event_becomes_insertable_text() {
        let event = egui::Event::Ime(egui::ImeEvent::Commit("かな".to_owned()));

        assert!(matches!(
            relevant_input_event(&event),
            Some(RelevantInputEvent::Text(text)) if text == "かな"
        ));
    }

    #[test]
    fn alt_backspace_is_classic_undo_without_extra_modifiers() {
        assert!(is_classic_undo_shortcut(PressedKeyEvent {
            key: egui::Key::Backspace,
            modifiers: egui::Modifiers::ALT,
        }));
        assert!(!is_classic_undo_shortcut(PressedKeyEvent {
            key: egui::Key::Backspace,
            modifiers: egui::Modifiers::ALT | egui::Modifiers::SHIFT,
        }));
    }

    #[test]
    fn key_events_remain_available_for_runtime_keymap_filtering() {
        let event = egui::Event::Key {
            key: egui::Key::ArrowLeft,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        };

        assert!(matches!(
            relevant_input_event(&event),
            Some(RelevantInputEvent::Key(_))
        ));
    }

    #[test]
    fn runtime_app_shortcuts_are_filtered_after_releasing_the_input_lock() {
        let ctx = egui::Context::default();
        crate::app::shortcut_tooltips::sync_context(
            &ctx,
            PlatformProfile::LinuxGeneric,
            &ShortcutSettings::default(),
        );
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::F,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        };

        let mut events = None;
        let _ = ctx.run_ui(input, |ui| events = Some(relevant_input_events(ui)));

        assert!(events.is_some_and(|events| events.is_empty()));
    }
}
