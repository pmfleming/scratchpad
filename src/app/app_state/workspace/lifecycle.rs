use super::super::{ScratchpadApp, StatusDomain};
use crate::app::commands::AppCommand;
use crate::app::diagnostics;
use crate::app::domain::{BufferId, SplitAxis, SplitPath, ViewId, WorkspaceTab};
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::{FileOpenDisposition, NewTabPlacement};

impl ScratchpadApp {
    pub fn new_tab(&mut self) {
        self.create_workspace_tab(WorkspaceTab::untitled());
        let _ = self.persist_session_now();
    }

    pub fn open_file(&mut self) {
        if matches!(
            self.file_open_disposition(),
            FileOpenDisposition::CurrentTab
        ) {
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
            diagnostics::record_io_error(
                "open_user_manual",
                Some(&path),
                "workspace::lifecycle",
                &"User manual not found",
            );
            self.set_error_status_with_detail(
                StatusDomain::File,
                "Could not open the user manual.",
                path.display().to_string(),
            );
            return;
        }

        self.activate_workspace_surface();
        FileController::open_paths_async(self, vec![path]);
    }

    pub fn save_file(&mut self) {
        FileController::save_file(self);
    }

    pub fn save_all_files(&mut self) {
        let active_tab_index = self.active_tab_index();
        let active_view_ids = self
            .tabs()
            .iter()
            .map(|tab| tab.active_view_id)
            .collect::<Vec<_>>();
        let targets = save_all_targets(self);
        let had_targets = !targets.is_empty();

        for (tab_index, view_id) in targets {
            self.tab_manager_mut().active_tab_index = tab_index;
            if let Some(tab) = self.tabs_mut().get_mut(tab_index) {
                tab.activate_view(view_id);
            }
            FileController::save_file_at(self, tab_index);
            if self.pending_action().is_some() {
                break;
            }
        }

        self.tab_manager_mut().active_tab_index =
            active_tab_index.min(self.tabs().len().saturating_sub(1));
        for (tab, view_id) in self.tabs_mut().iter_mut().zip(active_view_ids) {
            tab.activate_view(view_id);
        }
        self.request_focus_for_active_view();
        if had_targets {
            self.mark_session_dirty();
            let _ = self.persist_session_now();
        }
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
        self.close_tab_internal(index);
        let _ = self.persist_session_now();
    }

    pub fn perform_close_tab_no_persist(&mut self, index: usize) {
        let _ = self.close_tab_internal(index);
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

    pub(crate) fn insert_new_tab_from_settings(&mut self, tab: WorkspaceTab) {
        self.create_workspace_tab(tab);
    }

    pub fn reorder_tab(&mut self, from_index: usize, to_index: usize) {
        self.handle_command(AppCommand::ReorderTab {
            from_index,
            to_index,
        });
    }

    fn create_workspace_tab(&mut self, tab: WorkspaceTab) {
        self.reload_settings_before_workspace_change();
        self.begin_layout_transition();
        let index = self.new_tab_insert_index();
        self.tab_manager.insert_tab(index, tab);
        self.apply_current_tab_ordering();
        self.activate_workspace_surface();
        self.select_only_tab_slot(self.active_tab_slot_index());
        self.mark_search_dirty();
        self.request_focus_for_active_view();
    }

    fn new_tab_insert_index(&self) -> usize {
        match self.new_tab_placement() {
            NewTabPlacement::Start => 0,
            NewTabPlacement::End => self.tabs().len(),
            NewTabPlacement::BeforeSelection => self.selected_workspace_tab_range().0,
            NewTabPlacement::AfterSelection => self.selected_workspace_tab_range().1 + 1,
        }
        .min(self.tabs().len())
    }

    fn selected_workspace_tab_range(&self) -> (usize, usize) {
        let selected = self
            .selected_tab_slots
            .iter()
            .filter_map(|slot_index| self.workspace_index_for_slot(*slot_index))
            .collect::<Vec<_>>();
        let first = selected.iter().min().copied();
        let last = selected.iter().max().copied();
        match (first, last) {
            (Some(first), Some(last)) => (first, last),
            _ => {
                let active = self
                    .active_tab_index()
                    .min(self.tabs().len().saturating_sub(1));
                (active, active)
            }
        }
    }

    fn close_tab_internal(&mut self, index: usize) -> String {
        let closed_buffer_ids = self
            .tabs()
            .get(index)
            .map(|tab| tab.buffers().map(|buffer| buffer.id).collect::<Vec<_>>())
            .unwrap_or_default();
        let tab_description = self.tab_manager.describe_tab_at(index);
        let settings_refresh = self.settings_toml_refresh_on_tab_close(index);
        self.begin_layout_transition();
        self.tab_manager.close_tab_internal(index);
        self.prune_text_history_for_buffers(closed_buffer_ids);
        self.ensure_active_tab_slot_selected();
        self.mark_search_dirty();
        self.request_focus_for_active_view();
        self.apply_settings_toml_refresh(settings_refresh);
        tab_description
    }
}

fn save_all_targets(app: &ScratchpadApp) -> Vec<(usize, ViewId)> {
    app.tabs()
        .iter()
        .enumerate()
        .flat_map(|(tab_index, tab)| {
            let mut seen = Vec::<BufferId>::new();
            tab.views.iter().filter_map(move |view| {
                if seen.contains(&view.buffer_id) {
                    return None;
                }
                let buffer = tab.buffer_by_id(view.buffer_id)?;
                if !buffer.is_dirty {
                    return None;
                }
                seen.push(view.buffer_id);
                Some((tab_index, view.id))
            })
        })
        .collect()
}
