use super::super::{ScratchpadApp, TabRenameState};
use crate::app::domain::{PendingAction, ViewId};
use crate::app::services::file_controller::FileController;
use crate::app::services::session_manager;
use std::path::Path;

pub fn user_manual_path(app: &ScratchpadApp) -> &Path {
    &app.state.user_manual_path
}

pub(crate) fn persist_session_now(app: &mut ScratchpadApp) -> std::io::Result<()> {
    session_manager::persist_session_now(app)
}

pub fn pending_action(app: &ScratchpadApp) -> Option<PendingAction> {
    app.tab_manager.pending_action
}

pub fn set_pending_action(
    app: &mut ScratchpadApp,
    action: Option<PendingAction>,
) -> Option<PendingAction> {
    let old = app.tab_manager.pending_action;
    app.tab_manager.pending_action = action;
    old
}

pub(crate) fn clear_status_message(app: &mut ScratchpadApp) {
    app.state.status.clear_current();
}

pub(crate) fn request_focus_for_view(app: &mut ScratchpadApp, view_id: ViewId) {
    app.state.focus.request_focus_for_view(view_id);
}

pub(crate) fn request_focus_for_active_view(app: &mut ScratchpadApp) {
    if let Some(view_id) = app
        .tab_manager
        .active_tab()
        .map(|tab| tab.layout.active_view_id)
    {
        request_focus_for_view(app, view_id);
    }
}

pub(crate) fn should_focus_view(app: &ScratchpadApp, view_id: ViewId) -> bool {
    app.state.focus.should_focus_view(view_id)
}

pub(crate) fn consume_focus_request(app: &mut ScratchpadApp, view_id: ViewId) {
    app.state.focus.consume_focus_request(view_id);
}

pub(crate) fn begin_tab_rename(app: &mut ScratchpadApp, index: usize) {
    let Some(tab) = app.tab_manager.tabs.get(index) else {
        return;
    };
    let buffer = tab.active_buffer();
    app.state.dialogs.begin_tab_rename(TabRenameState {
        buffer_id: buffer.id,
        draft: buffer.name.clone(),
        request_focus: true,
    });
}

pub(crate) fn tab_rename_matches_slot(app: &ScratchpadApp, slot_index: usize) -> bool {
    let Some(rename_state) = app.state.dialogs.tab_rename() else {
        return false;
    };

    crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(app, slot_index)
        .and_then(|index| app.tab_manager.tabs.get(index))
        .is_some_and(|tab| {
            tab.buffers()
                .any(|buffer| buffer.id == rename_state.buffer_id)
        })
}

pub(crate) fn take_tab_rename_focus_request_for_slot(
    app: &mut ScratchpadApp,
    slot_index: usize,
) -> bool {
    if !tab_rename_matches_slot(app, slot_index) {
        return false;
    }

    app.state.dialogs.take_tab_rename_focus_request()
}

pub(crate) fn request_tab_rename_focus(app: &mut ScratchpadApp) {
    app.state.dialogs.request_tab_rename_focus();
}

pub(crate) fn tab_rename_draft_mut(app: &mut ScratchpadApp) -> Option<&mut String> {
    app.state.dialogs.tab_rename_draft_mut()
}

pub(crate) fn cancel_tab_rename(app: &mut ScratchpadApp) {
    app.state.dialogs.cancel_tab_rename();
    request_focus_for_active_view(app);
}

pub(crate) fn commit_tab_rename(app: &mut ScratchpadApp) -> bool {
    let Some(rename_state) = app.state.dialogs.tab_rename() else {
        return false;
    };

    let buffer_id = rename_state.buffer_id;
    let draft = rename_state.draft.clone();
    let Some(index) = app
        .tab_manager
        .tabs
        .iter()
        .position(|tab| tab.buffers().any(|buffer| buffer.id == buffer_id))
    else {
        app.state.dialogs.cancel_tab_rename();
        return false;
    };

    if FileController::rename_tab(app, index, &draft) {
        app.state.dialogs.cancel_tab_rename();
        request_focus_for_active_view(app);
        true
    } else {
        false
    }
}
