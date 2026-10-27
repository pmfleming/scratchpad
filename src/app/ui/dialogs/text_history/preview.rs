use super::model::TextHistoryAction;
use crate::app::{
    app_state::{ScratchpadApp, StatusDomain, workspace::mutation},
    domain::{BufferState, WorkspaceTab, buffer::DocumentRevision},
};
use eframe::egui;
use std::sync::Arc;

#[cfg(test)]
mod tests;

#[derive(Clone)]
struct Preview {
    buffer_id: u64,
    title: String,
    text: Arc<String>,
    display: Arc<String>,
    edit_action: Option<EditPreview>,
}

#[derive(Clone)]
struct EditPreview {
    action: TextHistoryAction,
    history_revision: u64,
    document_revision: DocumentRevision,
}

#[derive(Clone, Copy)]
enum PreviewAction {
    Restore,
    OpenNew,
    Replay,
}

fn key() -> egui::Id {
    egui::Id::unique("history.recovery.preview")
}

fn store(
    ctx: &egui::Context,
    buffer_id: u64,
    title: String,
    text: Arc<String>,
    edit_action: Option<EditPreview>,
) {
    let display = Arc::new(text.chars().take(32_768).collect());
    ctx.data_mut(|data| {
        data.insert_temp(
            key(),
            Preview {
                buffer_id,
                title,
                text,
                display,
                edit_action,
            },
        )
    });
}

pub(super) fn open(ctx: &egui::Context, buffer_id: u64, title: String, text: Arc<String>) {
    store(ctx, buffer_id, title, text, None);
}

pub(super) fn preview_edit(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    action: TextHistoryAction,
) {
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(WorkspaceTab::buffers)
        .find(|buffer| buffer.id == action.buffer_id)
    else {
        return;
    };
    let Some(entry) = buffer
        .document()
        .history_entries()
        .iter()
        .find(|entry| entry.id == action.entry_id)
    else {
        return;
    };
    let redo = entry.is_undone();
    let mut document = buffer.document().clone();
    let result = if redo {
        document.apply_text_history_redo(action.entry_id)
    } else {
        document.apply_text_history_undo(action.entry_id)
    };
    if result.is_err() {
        app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            "This edit is no longer replayable. Check recovery points for earlier text.",
        );
        return;
    }
    store(
        ctx,
        action.buffer_id,
        format!("{} (historical text)", buffer.name),
        Arc::new(document.extract_text()),
        Some(EditPreview {
            action,
            history_revision: buffer.document().history_revision_counter(),
            document_revision: buffer.document().revision_key(),
        }),
    );
}

pub(super) fn show(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(preview) = ctx.data(|data| data.get_temp::<Preview>(key())) else {
        return;
    };
    let mut visible = true;
    let mut action = None;
    egui::Window::new("History preview")
        .id(egui::Id::unique("history.preview.window"))
        .open(&mut visible)
        .default_size(egui::vec2(620.0, 400.0))
        .show(ctx, |ui| {
            action = render(ui, &preview);
        });
    if let Some(action) = action {
        visible = !apply_action(app, &preview, action, super::read_follow_focus(ctx));
    }
    if !visible {
        ctx.data_mut(|data| data.remove::<Preview>(key()));
    }
}

fn render(ui: &mut egui::Ui, preview: &Preview) -> Option<PreviewAction> {
    let label = ui.label(&preview.title);
    let mut text = preview.display.as_str();
    egui::ScrollArea::both().max_height(300.0).show(ui, |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut text)
                .font(egui::TextStyle::Monospace)
                .interactive(false)
                .desired_width(f32::INFINITY),
        )
        .labelled_by(label.id);
    });
    if text.len() < preview.text.len() {
        ui.small("Preview truncated; recovery uses the complete text.");
    }
    let mut selected = None;
    ui.horizontal(|ui| {
        for (action, label, help) in [
            (
                PreviewAction::Restore,
                "Restore into this tab",
                "Replace the source buffer's text as one undoable edit; does not save to disk",
            ),
            (
                PreviewAction::OpenNew,
                "Open as new tab",
                "Recover text into a new untitled tab",
            ),
            (
                PreviewAction::Replay,
                "Apply undo/redo",
                "Move the undo boundary without creating a replacement edit",
            ),
        ] {
            if matches!(action, PreviewAction::Replay) && preview.edit_action.is_none() {
                continue;
            }
            if ui.button(label).on_hover_text(help).clicked() {
                selected = Some(action);
            }
        }
    });
    selected
}

/// True closes the preview. Failed or stale actions leave it available for copying.
fn apply_action(
    app: &mut ScratchpadApp,
    preview: &Preview,
    action: PreviewAction,
    follow_focus: bool,
) -> bool {
    match action {
        PreviewAction::Restore => {
            mutation::restore_recovery_text(app, preview.buffer_id, &preview.text, follow_focus)
        }
        PreviewAction::OpenNew => {
            crate::app::app_state::workspace_controller::append_tab(
                app,
                WorkspaceTab::new(BufferState::new(
                    preview.title.clone(),
                    preview.text.as_ref().clone(),
                    None,
                )),
            );
            true
        }
        PreviewAction::Replay => replay(app, preview, follow_focus),
    }
}

fn replay(app: &mut ScratchpadApp, preview: &Preview, follow_focus: bool) -> bool {
    let Some(edit) = &preview.edit_action else {
        return false;
    };
    let unchanged = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(WorkspaceTab::buffers)
        .any(|buffer| {
            buffer.id == edit.action.buffer_id
                && buffer.document().history_revision_counter() == edit.history_revision
                && buffer.document().revision_key() == edit.document_revision
        });
    if !unchanged {
        app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            "History changed since preview; select the edit again.",
        );
        return false;
    }
    mutation::apply_text_history_to_entry(
        app,
        edit.action.buffer_id,
        edit.action.entry_id,
        follow_focus,
    )
}
