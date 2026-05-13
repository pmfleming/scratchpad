use super::super::{ScratchpadApp, TabRenameState};
use crate::app::domain::WorkspaceTab;
use crate::app::domain::{PendingAction, ViewId};
use crate::app::services::file_controller::FileController;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionStore;
use std::path::Path;

pub fn tabs(app: &ScratchpadApp) -> &[WorkspaceTab] {
    app.tab_manager.tabs.as_slice()
}

pub fn tabs_mut(app: &mut ScratchpadApp) -> &mut [WorkspaceTab] {
    app.tab_manager.tabs.as_mut_slice()
}

pub fn append_tab(app: &mut ScratchpadApp, tab: WorkspaceTab) {
    app.tab_manager.append_tab(tab);
}

pub fn user_manual_path(app: &ScratchpadApp) -> &Path {
    &app.state.user_manual_path
}

pub(crate) fn persist_session_now(app: &mut ScratchpadApp) -> std::io::Result<()> {
    session_manager::persist_session_now(app)
}

pub fn session_store(app: &ScratchpadApp) -> &SessionStore {
    &app.state.session_store
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

pub(crate) fn open_status_history(app: &mut ScratchpadApp) {
    app.state.status_history_open = true;
}

pub(crate) fn close_status_history(app: &mut ScratchpadApp) {
    app.state.status_history_open = false;
}

pub(crate) fn request_focus_for_view(app: &mut ScratchpadApp, view_id: ViewId) {
    app.state.pending_editor_focus = Some(view_id);
}

pub(crate) fn request_focus_for_active_view(app: &mut ScratchpadApp) {
    if let Some(view_id) = app.tab_manager.active_tab().map(|tab| tab.active_view_id) {
        request_focus_for_view(app, view_id);
    }
}

pub(crate) fn should_focus_view(app: &ScratchpadApp, view_id: ViewId) -> bool {
    app.state.pending_editor_focus == Some(view_id)
}

pub(crate) fn consume_focus_request(app: &mut ScratchpadApp, view_id: ViewId) {
    if app.state.pending_editor_focus == Some(view_id) {
        app.state.pending_editor_focus = None;
    }
}

pub(crate) fn begin_tab_rename(app: &mut ScratchpadApp, index: usize) {
    let Some(tab) = app.tab_manager.tabs.get(index) else {
        return;
    };
    let buffer = tab.active_buffer();
    app.state.tab_rename_state = Some(TabRenameState {
        buffer_id: buffer.id,
        draft: buffer.name.clone(),
        request_focus: true,
    });
}

pub(crate) fn tab_rename_matches_slot(app: &ScratchpadApp, slot_index: usize) -> bool {
    let Some(rename_state) = app.state.tab_rename_state.as_ref() else {
        return false;
    };

    app.workspace_index_for_slot(slot_index)
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

    app.state
        .tab_rename_state
        .as_mut()
        .map(|rename_state| std::mem::take(&mut rename_state.request_focus))
        .unwrap_or(false)
}

pub(crate) fn request_tab_rename_focus(app: &mut ScratchpadApp) {
    if let Some(rename_state) = app.state.tab_rename_state.as_mut() {
        rename_state.request_focus = true;
    }
}

pub(crate) fn tab_rename_draft_mut(app: &mut ScratchpadApp) -> Option<&mut String> {
    app.state
        .tab_rename_state
        .as_mut()
        .map(|rename_state| &mut rename_state.draft)
}

pub(crate) fn cancel_tab_rename(app: &mut ScratchpadApp) {
    app.state.tab_rename_state = None;
    request_focus_for_active_view(app);
}

pub(crate) fn commit_tab_rename(app: &mut ScratchpadApp) -> bool {
    let Some(rename_state) = app.state.tab_rename_state.as_ref() else {
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
        app.state.tab_rename_state = None;
        return false;
    };

    if FileController::rename_tab(app, index, &draft) {
        app.state.tab_rename_state = None;
        request_focus_for_active_view(app);
        true
    } else {
        false
    }
}

macro_rules! compat_scratchpad_app_methods {
    ($type:ty { $($item:item)* }) => {
        #[allow(dead_code)]
        impl $type {
            $($item)*
        }
    };
}

compat_scratchpad_app_methods!(ScratchpadApp {
    pub fn tabs(&self) -> &[WorkspaceTab] {
        tabs(self)
    }

    pub fn tabs_mut(&mut self) -> &mut [WorkspaceTab] {
        tabs_mut(self)
    }

    pub fn append_tab(&mut self, tab: WorkspaceTab) {
        append_tab(self, tab)
    }

    pub fn user_manual_path(&self) -> &Path {
        user_manual_path(self)
    }

    pub(crate) fn persist_session_now(&mut self) -> std::io::Result<()> {
        persist_session_now(self)
    }

    pub fn session_store(&self) -> &SessionStore {
        session_store(self)
    }

    pub fn pending_action(&self) -> Option<PendingAction> {
        pending_action(self)
    }

    pub fn set_pending_action(&mut self, action: Option<PendingAction>) -> Option<PendingAction> {
        set_pending_action(self, action)
    }

    pub(crate) fn clear_status_message(&mut self) {
        clear_status_message(self)
    }

    pub(crate) fn open_status_history(&mut self) {
        open_status_history(self)
    }

    pub(crate) fn close_status_history(&mut self) {
        close_status_history(self)
    }

    pub(crate) fn request_focus_for_view(&mut self, view_id: ViewId) {
        request_focus_for_view(self, view_id)
    }

    pub(crate) fn request_focus_for_active_view(&mut self) {
        request_focus_for_active_view(self)
    }

    pub(crate) fn should_focus_view(&self, view_id: ViewId) -> bool {
        should_focus_view(self, view_id)
    }

    pub(crate) fn consume_focus_request(&mut self, view_id: ViewId) {
        consume_focus_request(self, view_id)
    }

    pub(crate) fn begin_tab_rename(&mut self, index: usize) {
        begin_tab_rename(self, index)
    }

    pub(crate) fn tab_rename_matches_slot(&self, slot_index: usize) -> bool {
        tab_rename_matches_slot(self, slot_index)
    }

    pub(crate) fn take_tab_rename_focus_request_for_slot(&mut self, slot_index: usize) -> bool {
        take_tab_rename_focus_request_for_slot(self, slot_index)
    }

    pub(crate) fn request_tab_rename_focus(&mut self) {
        request_tab_rename_focus(self)
    }

    pub(crate) fn tab_rename_draft_mut(&mut self) -> Option<&mut String> {
        tab_rename_draft_mut(self)
    }

    pub(crate) fn cancel_tab_rename(&mut self) {
        cancel_tab_rename(self)
    }

    pub(crate) fn commit_tab_rename(&mut self) -> bool {
        commit_tab_rename(self)
    }
});
