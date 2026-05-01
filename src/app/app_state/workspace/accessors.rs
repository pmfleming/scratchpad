use super::super::{ScratchpadApp, TabRenameState};
use crate::app::domain::{EditorViewState, PendingAction, TabManager, ViewId, WorkspaceTab};
use crate::app::services::file_controller::FileController;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionStore;
use std::path::Path;

impl ScratchpadApp {
    pub fn user_manual_path(&self) -> &Path {
        &self.user_manual_path
    }

    pub(crate) fn active_tab(&self) -> Option<&WorkspaceTab> {
        self.tab_manager.active_tab()
    }

    pub(crate) fn active_tab_mut(&mut self) -> Option<&mut WorkspaceTab> {
        self.tab_manager.active_tab_mut()
    }

    pub(crate) fn active_view_mut(&mut self) -> Option<&mut EditorViewState> {
        self.active_tab_mut()
            .and_then(WorkspaceTab::active_view_mut)
    }

    pub(crate) fn mark_session_dirty(&mut self) {
        self.tab_manager.mark_session_dirty();
    }

    pub(crate) fn session_dirty(&self) -> bool {
        self.tab_manager.session_dirty
    }

    pub(crate) fn clear_session_dirty(&mut self) {
        self.tab_manager.session_dirty = false;
    }

    pub(crate) fn persist_session_now(&mut self) -> std::io::Result<()> {
        session_manager::persist_session_now(self)
    }

    pub fn tabs(&self) -> &[WorkspaceTab] {
        &self.tab_manager.tabs
    }

    pub fn tabs_mut(&mut self) -> &mut [WorkspaceTab] {
        &mut self.tab_manager.tabs
    }

    pub fn active_tab_index(&self) -> usize {
        self.tab_manager.active_tab_index
    }

    pub(crate) fn find_tab_by_path(&self, candidate: &Path) -> Option<(usize, ViewId)> {
        self.tab_manager.find_tab_by_path(candidate)
    }

    pub fn session_store(&self) -> &SessionStore {
        &self.session_store
    }

    pub fn tab_manager(&self) -> &TabManager {
        &self.tab_manager
    }

    pub fn tab_manager_mut(&mut self) -> &mut TabManager {
        &mut self.tab_manager
    }

    pub fn pending_action(&self) -> Option<PendingAction> {
        self.tab_manager.pending_action
    }

    pub fn set_pending_action(&mut self, action: Option<PendingAction>) -> Option<PendingAction> {
        let old = self.tab_manager.pending_action;
        self.tab_manager.pending_action = action;
        old
    }

    pub(crate) fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    pub(crate) fn request_focus_for_view(&mut self, view_id: ViewId) {
        self.pending_editor_focus = Some(view_id);
    }

    pub(crate) fn request_focus_for_active_view(&mut self) {
        if let Some(view_id) = self.active_tab().map(|tab| tab.active_view_id) {
            self.request_focus_for_view(view_id);
        }
    }

    pub(crate) fn should_focus_view(&self, view_id: ViewId) -> bool {
        self.pending_editor_focus == Some(view_id)
    }

    pub(crate) fn consume_focus_request(&mut self, view_id: ViewId) {
        if self.pending_editor_focus == Some(view_id) {
            self.pending_editor_focus = None;
        }
    }

    pub(crate) fn begin_tab_rename(&mut self, index: usize) {
        let Some(tab) = self.tabs().get(index) else {
            return;
        };
        let buffer = tab.active_buffer();
        self.tab_rename_state = Some(TabRenameState {
            buffer_id: buffer.id,
            draft: buffer.name.clone(),
            request_focus: true,
        });
    }

    pub(crate) fn tab_rename_matches_slot(&self, slot_index: usize) -> bool {
        let Some(rename_state) = self.tab_rename_state.as_ref() else {
            return false;
        };

        self.workspace_index_for_slot(slot_index)
            .and_then(|index| self.tabs().get(index))
            .is_some_and(|tab| {
                tab.buffers()
                    .any(|buffer| buffer.id == rename_state.buffer_id)
            })
    }

    pub(crate) fn take_tab_rename_focus_request_for_slot(&mut self, slot_index: usize) -> bool {
        if !self.tab_rename_matches_slot(slot_index) {
            return false;
        }

        self.tab_rename_state
            .as_mut()
            .map(|rename_state| std::mem::take(&mut rename_state.request_focus))
            .unwrap_or(false)
    }

    pub(crate) fn request_tab_rename_focus(&mut self) {
        if let Some(rename_state) = self.tab_rename_state.as_mut() {
            rename_state.request_focus = true;
        }
    }

    pub(crate) fn tab_rename_draft_mut(&mut self) -> Option<&mut String> {
        self.tab_rename_state
            .as_mut()
            .map(|rename_state| &mut rename_state.draft)
    }

    pub(crate) fn cancel_tab_rename(&mut self) {
        self.tab_rename_state = None;
        self.request_focus_for_active_view();
    }

    pub(crate) fn commit_tab_rename(&mut self) -> bool {
        let Some(rename_state) = self.tab_rename_state.as_ref() else {
            return false;
        };

        let buffer_id = rename_state.buffer_id;
        let draft = rename_state.draft.clone();
        let Some(index) = self
            .tabs()
            .iter()
            .position(|tab| tab.buffers().any(|buffer| buffer.id == buffer_id))
        else {
            self.tab_rename_state = None;
            return false;
        };

        if FileController::rename_tab(self, index, &draft) {
            self.tab_rename_state = None;
            self.request_focus_for_active_view();
            true
        } else {
            false
        }
    }
}
