use super::super::ScratchpadApp;
use crate::app::domain::{EditorViewState, PendingAction, TabManager, ViewId, WorkspaceTab};
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

    pub(crate) fn describe_tab_at(&self, index: usize) -> String {
        self.tab_manager.describe_tab_at(index)
    }

    pub(crate) fn describe_active_tab(&self) -> String {
        self.tab_manager.describe_active_tab()
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
}
