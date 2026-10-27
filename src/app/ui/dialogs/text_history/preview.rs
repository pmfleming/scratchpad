use super::model::TextHistoryAction;
use crate::app::{app_state::{ScratchpadApp, StatusDomain, workspace::mutation}, domain::{BufferState, WorkspaceTab}};
use eframe::egui;
use std::sync::Arc;

#[derive(Clone)]
struct Preview { buffer_id: u64, title: String, text: Arc<String> }
fn key() -> egui::Id { egui::Id::unique("history.recovery.preview") }

pub(super) fn open(ctx: &egui::Context, buffer_id: u64, title: String, text: Arc<String>) {
    ctx.data_mut(|data| data.insert_temp(key(), Preview { buffer_id, title, text }));
}

pub(super) fn preview_edit(ctx: &egui::Context, app: &mut ScratchpadApp, action: TextHistoryAction) {
    let Some(buffer) = app.tab_manager.tabs.as_slice().iter().flat_map(WorkspaceTab::buffers)
        .find(|buffer| buffer.id == action.buffer_id) else { return; };
    let mut document = buffer.document().clone();
    let Some(entry) = document.history_entries().iter().find(|entry| entry.id == action.entry_id) else { return; };
    let result = if entry.is_undone() { document.apply_text_history_redo(action.entry_id) }
        else { document.apply_text_history_undo(action.entry_id) };
    if result.is_err() {
        app.state.status.set_error_status_in_domain(StatusDomain::History, "This edit is no longer replayable. Check recovery points for earlier text.");
    } else {
        open(ctx, action.buffer_id, format!("{} (historical text)", buffer.name), Arc::new(document.extract_text()));
    }
}

pub(super) fn show(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(preview) = ctx.data(|data| data.get_temp::<Preview>(key())) else { return; };
    let mut visible = true;
    let mut restore = false;
    let mut new_tab = false;
    egui::Window::new("History preview").id(egui::Id::unique("history.preview.window"))
        .open(&mut visible).default_size(egui::vec2(620.0, 400.0)).show(ctx, |ui| {
        ui.label(&preview.title);
        let mut text = preview.text.chars().take(32_768).collect::<String>();
        egui::ScrollArea::both().max_height(300.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace).interactive(false).desired_width(f32::INFINITY));
        });
        if text.len() < preview.text.len() { ui.small("Preview truncated; recovery uses the complete text."); }
        ui.horizontal(|ui| {
            restore = ui.button("Restore into this tab").on_hover_text("Replace the source buffer's text as one undoable edit; does not save to disk").clicked();
            new_tab = ui.button("Open as new tab").clicked();
        });
    });
    if restore {
        visible = !mutation::restore_recovery_text(app, preview.buffer_id, preview.text.as_str());
    }
    if new_tab {
        crate::app::app_state::workspace_controller::append_tab(app,
            WorkspaceTab::new(BufferState::new(preview.title, preview.text.as_ref().clone(), None)));
        visible = false;
    }
    if !visible { ctx.data_mut(|data| data.remove::<Preview>(key())); }
}
