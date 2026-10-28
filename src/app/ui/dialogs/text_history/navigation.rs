//! Native undo/redo navigation, with snapshot transitions delegated to snapshots.
use super::model::{HistoryAction, TextHistoryAction};
use crate::app::{
    app_state::{ScratchpadApp, workspace::mutation},
    text_history::TextHistoryEntryView,
    ui::editor_content::native_editor::history_feedback,
};
use eframe::egui;

fn buffer_key() -> egui::Id {
    egui::Id::unique("text_history.navigation.buffer")
}

pub(super) fn clear(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<u64>(buffer_key()));
}

pub(super) fn select_buffer(ctx: &egui::Context, id: u64) {
    ctx.data_mut(|data| data.insert_temp(buffer_key(), id));
}

pub(super) fn target_buffer(ctx: &egui::Context, app: &ScratchpadApp) -> Option<u64> {
    let selected = ctx.data(|data| data.get_temp::<u64>(buffer_key()));
    selected
        .filter(|id| {
            app.tab_manager
                .tabs
                .as_slice()
                .iter()
                .any(|tab| tab.buffer_by_id(*id).is_some())
        })
        .or_else(|| {
            app.tab_manager
                .active_tab()
                .map(|tab| tab.active_buffer().id)
        })
}

pub(super) fn step_target(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    undo: bool,
) -> Option<TextHistoryAction> {
    let buffer_id = target_buffer(ctx, app)?;
    choose_target(&mutation::cached_text_history_entries(app), buffer_id, undo)
}

fn choose_target(
    entries: &[TextHistoryEntryView],
    buffer_id: u64,
    undo: bool,
) -> Option<TextHistoryAction> {
    let candidates = entries
        .iter()
        .filter(|entry| entry.buffer_id == buffer_id && entry.undone != undo);
    let entry = if undo {
        candidates.max_by_key(|entry| entry.global_seq)
    } else {
        candidates.min_by_key(|entry| entry.global_seq)
    }?;
    entry.replayable.then_some(TextHistoryAction {
        buffer_id,
        entry_id: entry.id,
    })
}

pub(super) fn controls(
    ui: &mut egui::Ui,
    [older, newer]: [Option<HistoryAction>; 2],
) -> Option<HistoryAction> {
    let mut selected = None;
    // The control bar lays out right-to-left: Undo, Redo, then Delete visually.
    for (target, id, icon, tooltip) in [
        (
            newer,
            "redo",
            egui_phosphor::regular::ARROW_DOWN,
            "Down: Newer state in this file",
        ),
        (
            older,
            "undo",
            egui_phosphor::regular::ARROW_UP,
            "Up: Older state in this file",
        ),
    ] {
        if super::control_icon_button(ui, id, icon, tooltip, false, target.is_some()).clicked() {
            selected = target;
        }
    }
    selected
}

pub(super) fn apply(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    action: TextHistoryAction,
) -> bool {
    if let Some(result) = super::snapshots::return_for_edit(ctx, app, action) {
        return result;
    }
    // Mark the start of the selected change, not the selection or an expensive
    // whole-document diff. For deletions this still identifies the affected line.
    let offset = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(action.buffer_id))
        .and_then(|buffer| {
            buffer
                .document()
                .history_entries()
                .iter()
                .find(|entry| entry.id == action.entry_id)
        })
        .and_then(|entry| {
            entry
                .edits
                .iter()
                .map(|edit| edit.start_char() as usize)
                .min()
        });
    if !mutation::apply_text_history_to_entry(
        app,
        action.buffer_id,
        action.entry_id,
        super::read_follow_focus(ctx),
    ) {
        return false;
    }
    ctx.data_mut(|data| data.insert_temp(buffer_key(), action.buffer_id));
    if let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(action.buffer_id))
        && let Some(offset) = offset
    {
        history_feedback::mark(ctx, buffer, offset);
    }
    ctx.request_repaint();
    true
}

#[cfg(test)]
mod tests;
