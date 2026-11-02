use crate::app::{
    app_state::{ScratchpadApp, workspace::accessors},
    domain::{BufferState, buffer::DocumentRevision},
};
use eframe::egui;

#[derive(Clone)]
struct EditorActivity {
    buffer_id: u64,
    revision: DocumentRevision,
    history_revision: u64,
    show: bool,
}

fn activity_key() -> egui::Id {
    egui::Id::unique("history.scrubber.editor_activity")
}

/// Record intent even at an undo boundary or before an IME edit commits.
pub(crate) fn note_editor_history_activity(ctx: &egui::Context, buffer: &BufferState, show: bool) {
    let activity = EditorActivity {
        buffer_id: buffer.id,
        revision: buffer.document().revision_key(),
        history_revision: buffer.document().history_revision_counter(),
        show,
    };
    ctx.data_mut(|data| data.insert_temp(activity_key(), activity));
}

pub(super) fn sync_editor_activity(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(activity) = ctx.data_mut(|data| {
        let activity = data.get_temp::<EditorActivity>(activity_key());
        data.remove::<EditorActivity>(activity_key());
        activity
    }) else {
        return;
    };
    // A later edit or tab switch in the same frame supersedes this input.
    let current = app.tab_manager.active_tab().is_some_and(|tab| {
        let buffer = tab.active_buffer();
        buffer.id == activity.buffer_id
            && buffer.document().revision_key() == activity.revision
            && buffer.document().history_revision_counter() == activity.history_revision
    });
    if current {
        if activity.show {
            app.state.dialogs.history_scrubber.open();
        } else {
            close(ctx, app);
        }
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
