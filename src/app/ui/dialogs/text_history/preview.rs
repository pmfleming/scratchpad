use super::model::TextHistoryAction;
use crate::app::{
    app_state::{ScratchpadApp, StatusDomain, workspace::mutation},
    domain::{BufferState, WorkspaceTab},
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
    edit_action: Option<(TextHistoryAction, u64)>,
}
fn key() -> egui::Id {
    egui::Id::unique("history.recovery.preview")
}

pub(super) fn open(ctx: &egui::Context, buffer_id: u64, title: String, text: Arc<String>) {
    ctx.data_mut(|data| {
        data.insert_temp(
            key(),
            Preview {
                buffer_id,
                title,
                text,
                edit_action: None,
            },
        )
    });
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
    let mut document = buffer.document().clone();
    let Some(entry) = document
        .history_entries()
        .iter()
        .find(|entry| entry.id == action.entry_id)
    else {
        return;
    };
    let result = if entry.is_undone() {
        document.apply_text_history_redo(action.entry_id)
    } else {
        document.apply_text_history_undo(action.entry_id)
    };
    if result.is_err() {
        app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            "This edit is no longer replayable. Check recovery points for earlier text.",
        );
    } else {
        ctx.data_mut(|data| {
            data.insert_temp(
                key(),
                Preview {
                    buffer_id: action.buffer_id,
                    title: format!("{} (historical text)", buffer.name),
                    text: Arc::new(document.extract_text()),
                    edit_action: Some((action, buffer.document().history_revision_counter())),
                },
            )
        });
    }
}

pub(super) fn show(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(preview) = ctx.data(|data| data.get_temp::<Preview>(key())) else {
        return;
    };
    let mut visible = true;
    let mut restore = false;
    let mut new_tab = false;
    let mut replay = false;
    egui::Window::new("History preview").id(egui::Id::unique("history.preview.window"))
        .open(&mut visible).default_size(egui::vec2(620.0, 400.0)).show(ctx, |ui| {
        let label = ui.label(&preview.title);
        let mut text = preview.text.chars().take(32_768).collect::<String>();
        egui::ScrollArea::both().max_height(300.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace).interactive(false).desired_width(f32::INFINITY)).labelled_by(label.id);
        });
        if text.len() < preview.text.len() { ui.small("Preview truncated; recovery uses the complete text."); }
        ui.horizontal(|ui| {
            restore = ui.button("Restore into this tab").on_hover_text("Replace the source buffer's text as one undoable edit; does not save to disk").clicked();
            new_tab = ui.button("Open as new tab").clicked();
            if preview.edit_action.is_some() {
                replay = ui.button("Apply undo/redo").on_hover_text("Move the undo boundary without creating a replacement edit").clicked();
            }
        });
    });
    if restore {
        let previous_view = app
            .tab_manager
            .active_tab()
            .map(|tab| tab.layout.active_view_id());
        visible = !mutation::restore_recovery_text(app, preview.buffer_id, preview.text.as_str());
        if !super::read_follow_focus(ctx)
            && let Some(view_id) = previous_view
            && let Some(index) = app
                .tab_manager
                .tabs
                .as_slice()
                .iter()
                .position(|tab| tab.layout.views.iter().any(|view| view.id == view_id))
        {
            app.tab_manager.set_active_tab_index_clamped(index);
            app.tab_manager.tabs.as_mut_slice()[index].activate_view(view_id);
            crate::app::app_state::workspace::accessors::request_focus_for_active_view(app);
        }
    }
    if replay && let Some((action, revision)) = preview.edit_action {
        let unchanged = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(WorkspaceTab::buffers)
            .any(|buffer| {
                buffer.id == action.buffer_id
                    && buffer.document().history_revision_counter() == revision
            });
        if unchanged {
            visible = !mutation::apply_text_history_to_entry(
                app,
                action.buffer_id,
                action.entry_id,
                super::read_follow_focus(ctx),
            );
        } else {
            app.state.status.set_error_status_in_domain(
                StatusDomain::History,
                "History changed since preview; select the edit again.",
            );
        }
    }
    if new_tab {
        crate::app::app_state::workspace_controller::append_tab(
            app,
            WorkspaceTab::new(BufferState::new(
                preview.title,
                preview.text.as_ref().clone(),
                None,
            )),
        );
        visible = false;
    }
    if !visible {
        ctx.data_mut(|data| data.remove::<Preview>(key()));
    }
}
