use crate::app::{
    app_state::{ScratchpadApp, workspace::accessors},
    domain::buffer::TextHistoryEvent,
};
use eframe::egui;

/// Drain each buffer once per frame; only the current file controls visibility.
/// Mutations and input intent share this stream, so later input supersedes earlier input.
pub(super) fn sync_history_activity(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let active = super::active_buffer(app);
    let mut activity = None;
    for buffer in app
        .tab_manager
        .tabs
        .as_mut_slice()
        .iter_mut()
        .flat_map(|tab| tab.buffers_mut())
    {
        let event = buffer.take_text_history_event();
        if Some(buffer.id) == active {
            activity = event;
        }
    }
    match activity {
        Some(TextHistoryEvent::Replay) => app.state.dialogs.history_scrubber.open(),
        Some(TextHistoryEvent::Edit) => {
            app.state.dialogs.history_scrubber.close();
            ctx.data_mut(|d| d.remove::<super::Drag>(super::drag_id()));
            if ctx.memory(|m| m.has_focus(super::focus_id())) {
                accessors::request_focus_for_active_view(app);
            }
        }
        Some(TextHistoryEvent::Navigate) | None => {}
    }
}

pub(in super::super) fn close(ctx: &egui::Context, app: &mut ScratchpadApp) {
    app.state.dialogs.history_scrubber.close();
    ctx.data_mut(|data| data.remove::<super::Drag>(super::drag_id()));
    // Escape may already have released the editor's focus by the time this runs.
    accessors::request_focus_for_active_view(app);
}

/// Return typing and undo/redo to the editor without consuming or reordering
/// events, so pointer scrubbing and keyboard history use the same native path.
pub(in super::super) fn resume_editor_input(ctx: &egui::Context, app: &mut ScratchpadApp) -> bool {
    let resume = ctx.input(|input| {
        input.events.iter().any(|event| match event {
            egui::Event::Text(text)
            | egui::Event::Paste(text)
            | egui::Event::Ime(
                egui::ImeEvent::Preedit { text, .. } | egui::ImeEvent::Commit(text),
            ) => !text.is_empty(),
            egui::Event::Cut => true,
            egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } => {
                matches!(
                    key,
                    egui::Key::Backspace | egui::Key::Delete | egui::Key::Enter
                ) || (matches!(key, egui::Key::Z | egui::Key::Y)
                    && modifiers.command
                    && !modifiers.alt
                    && !modifiers.shift)
            }
            _ => false,
        })
    });
    if resume {
        accessors::request_focus_for_active_view(app);
    }
    resume
}
