//! Execute an inclusive native replay target after the shared state model validates it.
use super::model::TextHistoryAction;
use crate::app::{
    app_state::{ScratchpadApp, workspace::mutation},
    ui::editor_content::native_editor::history_feedback,
};
use eframe::egui;

pub(super) fn apply(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    action: TextHistoryAction,
) -> bool {
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
    retain_tool_focus(ctx, app);
    ctx.request_repaint();
    true
}

pub(super) fn retain_tool_focus(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if ctx.memory(|m| {
        m.has_focus(super::list::focus_id()) || m.has_focus(super::scrubber::focus_id())
    }) && let Some(view) = app
        .tab_manager
        .active_tab()
        .and_then(|tab| tab.layout.active_view())
        .map(|view| view.id)
    {
        crate::app::app_state::workspace::accessors::consume_focus_request(app, view);
    }
}

#[cfg(test)]
mod tests;
