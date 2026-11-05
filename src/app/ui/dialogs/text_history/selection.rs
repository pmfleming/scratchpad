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
    if let Some(cold) = app.tab_manager.cold_session_tabs().get(&index).cloned() {
        let document = app.tab_manager.tabs.as_slice()[index]
            .buffer_by_id(buffer)
            .map(|b| b.temp_id.clone());
        app.queue_retained_open(
            crate::app::services::session_store::RetainedTarget::Cold(cold),
            document.clone(),
            None,
        );
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<super::HistoryUi>(super::state_key())
                .pending_file = document;
        });
        return;
    }
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
    pub temp_id: String,
}

pub(super) fn reopen(ctx: &egui::Context, app: &mut ScratchpadApp, file: ArchivedFile) {
    super::timeline::apply(
        ctx,
        app,
        super::timeline::Action {
            document: file.temp_id,
            node: None,
            inspect: true,
        },
    );
}
