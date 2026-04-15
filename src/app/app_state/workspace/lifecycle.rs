use super::super::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{SplitAxis, SplitPath, ViewId, WorkspaceTab};
use crate::app::logging::LogLevel;
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::FileOpenDisposition;

impl ScratchpadApp {
    fn log_tab_lifecycle_event(&self, action: &str, index: usize, description: &str) {
        self.log_event(
            LogLevel::Info,
            format!(
                "{action} at index {index}: {description} (total tabs={})",
                self.tab_manager.tabs.len()
            ),
        );
    }

    pub fn new_tab(&mut self) {
        let snapshot = self.capture_transaction_snapshot();
        self.create_workspace_tab(WorkspaceTab::untitled());
        let description = self.tab_manager.describe_active_tab();
        self.record_transaction("New tab", vec![self.describe_active_tab()], None, snapshot);
        self.log_tab_lifecycle_event(
            "Created new tab",
            self.tab_manager.active_tab_index,
            &description,
        );
        let _ = self.persist_session_now();
    }

    pub fn open_file(&mut self) {
        if matches!(self.file_open_disposition(), FileOpenDisposition::CurrentTab) {
            FileController::open_file_here(self);
        } else {
            FileController::open_file(self);
        }
    }

    pub fn open_file_here(&mut self) {
        FileController::open_file_here(self);
    }

    pub fn open_user_manual(&mut self) {
        let path = self.user_manual_path().to_path_buf();
        if !path.is_file() {
            self.log_event(
                LogLevel::Error,
                format!("User manual not found: {}", path.display()),
            );
            self.set_error_status(format!("User manual not found: {}", path.display()));
            return;
        }

        self.activate_workspace_surface();
        FileController::open_paths(self, vec![path]);
    }

    pub fn save_file(&mut self) {
        FileController::save_file(self);
    }

    pub fn save_file_at(&mut self, index: usize) -> bool {
        FileController::save_file_at(self, index)
    }

    pub fn save_file_as(&mut self) {
        FileController::save_file_as(self);
    }

    pub fn save_file_as_at(&mut self, index: usize) -> bool {
        FileController::save_file_as_at(self, index)
    }

    pub(crate) fn perform_close_tab(&mut self, index: usize) {
        let snapshot = self.capture_transaction_snapshot();
        let tab_description = self.close_tab_internal(index);
        self.record_transaction("Close tab", vec![tab_description.clone()], None, snapshot);
        self.log_tab_lifecycle_event("Closed tab", index, &tab_description);
        let _ = self.persist_session_now();
    }

    pub fn perform_close_tab_no_persist(&mut self, index: usize) {
        let tab_description = self.close_tab_internal(index);
        self.log_event(
            LogLevel::Info,
            format!("Closed tab without immediate persist at index {index}: {tab_description}"),
        );
    }

    pub fn split_active_view_with_placement(
        &mut self,
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    ) {
        self.handle_command(AppCommand::SplitActiveView {
            axis,
            new_view_first,
            ratio,
        });
    }

    pub(crate) fn close_view(&mut self, view_id: ViewId) {
        self.handle_command(AppCommand::CloseView { view_id });
    }

    pub(crate) fn promote_view_to_tab(&mut self, view_id: ViewId) {
        self.handle_command(AppCommand::PromoteViewToTab { view_id });
    }

    pub(crate) fn activate_view(&mut self, view_id: ViewId) {
        self.handle_command(AppCommand::ActivateView { view_id });
    }

    pub(crate) fn resize_split(&mut self, path: SplitPath, ratio: f32) {
        self.handle_command(AppCommand::ResizeSplit { path, ratio });
    }

    pub fn append_tab(&mut self, tab: WorkspaceTab) {
        self.create_workspace_tab(tab);
    }

    pub fn create_untitled_tab(&mut self) {
        self.create_workspace_tab(WorkspaceTab::untitled());
    }

    pub fn reorder_tab(&mut self, from_index: usize, to_index: usize) {
        self.handle_command(AppCommand::ReorderTab {
            from_index,
            to_index,
        });
    }

    fn create_workspace_tab(&mut self, tab: WorkspaceTab) {
        self.reload_settings_before_workspace_change();
        self.tab_manager.append_tab(tab);
        self.mark_search_dirty();
        self.request_focus_for_active_view();
    }

    fn close_tab_internal(&mut self, index: usize) -> String {
        let tab_description = self.tab_manager.describe_tab_at(index);
        let settings_refresh = self.settings_toml_refresh_on_tab_close(index);
        self.tab_manager.close_tab_internal(index);
        self.mark_search_dirty();
        self.request_focus_for_active_view();
        self.apply_settings_toml_refresh(settings_refresh);
        tab_description
    }
}