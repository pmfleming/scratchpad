//! Switching the displayed file must not replay its current edit or restore a
//! history cursor over the user's current editor selection.
use crate::app::app_state::{
    ScratchpadApp, settings_controller,
    workspace::{accessors, display_tabs, mutation},
};
use eframe::egui;

pub(super) fn activate(ctx: &egui::Context, app: &mut ScratchpadApp, buffer: u64) {
    let Some(index) = mutation::tab_index_for_buffer(app, buffer) else {
        return;
    };
    let tab = &app.tab_manager.tabs.as_slice()[index];
    let view = tab
        .layout
        .active_view()
        .filter(|view| view.buffer_id == buffer)
        .or_else(|| {
            tab.layout
                .views
                .iter()
                .find(|view| view.buffer_id == buffer)
        })
        .map(|view| view.id);
    let Some(view) = view else {
        return;
    };
    let previous = app
        .tab_manager
        .active_tab()
        .map(|tab| tab.layout.active_view_id());
    let switching = app.tab_manager.active_tab_index != index || previous != Some(view);
    if let Some(previous) = previous {
        accessors::consume_focus_request(app, previous);
    }
    settings_controller::activate_workspace_surface(app);
    app.tab_manager.set_active_tab_index_clamped(index);
    app.tab_manager.tabs.as_mut_slice()[index].activate_view(view);
    display_tabs::ensure_active_tab_slot_selected(app);
    app.tab_manager.pending_scroll_to_active = true;
    accessors::consume_focus_request(app, view);
    if switching {
        app.tab_manager.mark_session_dirty();
        crate::app::app_state::search_runtime::mark_search_dirty(app);
    }
    ctx.memory_mut(|memory| memory.request_focus(super::focus_id()));
    ctx.request_repaint();
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) struct ArchivedFile {
    pub token: String,
    pub temp_id: String,
}

pub(super) fn reopen(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    file: ArchivedFile,
) -> Option<u64> {
    crate::app::app_state::workspace::lifecycle::reopen_archived_tab(app, &file.token);
    let id = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
        .find(|buffer| buffer.temp_id == file.temp_id)
        .map(|buffer| buffer.id);
    if let Some(id) = id {
        activate(ctx, app, id);
    }
    id
}
