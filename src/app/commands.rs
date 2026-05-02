use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{BufferId, PendingAction, SplitAxis, SplitPath, ViewId};
use crate::app::services::file_controller::FileController;

mod dispatch;
mod tab_transfer;

pub enum AppCommand {
    ActivateTab {
        index: usize,
    },
    ActivateView {
        view_id: ViewId,
    },
    CloseTab {
        index: usize,
    },
    CloseView {
        view_id: ViewId,
    },
    CloseSettings,
    CombineTabIntoTab {
        source_index: usize,
        target_index: usize,
    },
    CombineTabsIntoTab {
        source_indices: Vec<usize>,
        target_index: usize,
    },
    PromoteViewToTab {
        view_id: ViewId,
    },
    PromoteTabFilesToTabs {
        index: usize,
    },
    NewTab,
    OpenFile,
    OpenFileHere,
    OpenSearch,
    OpenSearchAndReplace,
    OpenSettings,
    OpenTextHistory,
    OpenUserManual,
    CloseSearch,
    UndoActiveBufferTextOperation,
    RedoActiveBufferTextOperation,
    NextSearchMatch,
    PreviousSearchMatch,
    ReplaceCurrentMatch,
    ReplaceAllMatches,
    ReorderTab {
        from_index: usize,
        to_index: usize,
    },
    ReorderDisplayTab {
        from_index: usize,
        to_index: usize,
    },
    RequestCloseTab {
        index: usize,
    },
    ResizeSplit {
        path: SplitPath,
        ratio: f32,
    },
    SaveFile,
    SaveFileAs,
    SplitActiveView {
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    },
}

impl ScratchpadApp {
    pub fn open_text_history(&mut self) {
        self.text_history_open = true;
    }

    pub fn close_text_history(&mut self) {
        self.text_history_open = false;
    }

    fn activate_tab(&mut self, index: usize) {
        if index >= self.tabs().len() {
            return;
        }

        self.reload_settings_before_workspace_change();
        self.activate_workspace_surface();
        self.tab_manager_mut().active_tab_index = index;
        self.ensure_active_tab_slot_selected();
        self.tab_manager_mut().pending_scroll_to_active = true;
        self.refresh_search_view_state();
        self.request_focus_for_active_view();
        FileController::refresh_active_buffer_disk_state(self);
        self.mark_session_dirty();
    }

    fn activate_view_command(&mut self, view_id: ViewId) {
        self.reload_settings_if_switching_views(view_id);

        let index = self.active_tab_index();
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.activate_view(view_id)
        {
            self.refresh_search_view_state();
            self.request_focus_for_view(view_id);
            FileController::refresh_active_buffer_disk_state(self);
            self.mark_session_dirty();
        }
    }

    fn close_view_command(&mut self, view_id: ViewId) {
        let index = self.active_tab_index();
        let Some(tab) = self.tabs().get(index) else {
            return;
        };
        if tab.root_pane.leaf_count() <= 1 || !tab.root_pane.contains_view(view_id) {
            return;
        }

        if tab
            .buffer_for_view(view_id)
            .is_some_and(|buffer| buffer.is_dirty)
            && tab.is_last_view_for_buffer(view_id) == Some(true)
        {
            self.set_pending_action(Some(PendingAction::CloseView {
                tab_index: index,
                view_id,
            }));
            return;
        }

        self.perform_close_view(view_id);
    }

    pub(crate) fn perform_close_view(&mut self, view_id: ViewId) {
        self.reload_settings_if_closing_view(view_id);

        let index = self.active_tab_index();
        let open_buffer_ids_before = self
            .tabs()
            .get(index)
            .map(|tab| tab.buffers().map(|buffer| buffer.id).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut next_active_view = None;
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.close_view(view_id)
        {
            next_active_view = Some(tab.active_view_id);
        }
        if let Some(next_active_view) = next_active_view {
            let open_buffer_ids_after = self
                .tabs()
                .get(index)
                .map(|tab| tab.buffers().map(|buffer| buffer.id).collect::<Vec<_>>())
                .unwrap_or_default();
            let closed_buffer_ids =
                removed_buffer_ids(open_buffer_ids_before, &open_buffer_ids_after);
            self.prune_text_history_for_buffers(closed_buffer_ids);
            self.begin_layout_transition();
            self.mark_search_dirty();
            self.request_focus_for_view(next_active_view);
            self.mark_session_dirty();
        }
    }

    fn request_close_tab(&mut self, index: usize) {
        if index < self.tabs().len() {
            self.set_pending_action(Some(PendingAction::CloseTab(index)));
        }
    }

    fn reorder_tab_command(&mut self, from_index: usize, to_index: usize) {
        if !self.tab_manager_mut().reorder_tab(from_index, to_index) {
            return;
        }
        self.begin_layout_transition();
        self.mark_session_dirty();
    }

    fn reorder_display_tab_command(&mut self, from_index: usize, to_index: usize) {
        if self.reorder_display_tab(from_index, to_index) {
            self.begin_layout_transition();
        }
    }

    fn resize_split_command(&mut self, path: SplitPath, ratio: f32) {
        let index = self.active_tab_index();
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.resize_split(path, ratio)
        {
            self.begin_layout_transition();
            self.mark_session_dirty();
        }
    }

    fn split_active_view_command(&mut self, axis: SplitAxis, new_view_first: bool, ratio: f32) {
        let index = self.active_tab_index();
        let mut new_active_view = None;
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab
                .split_active_view_with_placement(axis, new_view_first, ratio)
                .is_some()
        {
            new_active_view = Some(tab.active_view_id);
        }
        if let Some(new_active_view) = new_active_view {
            self.begin_layout_transition();
            self.mark_search_dirty();
            self.request_focus_for_view(new_active_view);
            self.mark_session_dirty();
        }
    }
}

fn removed_buffer_ids(before: Vec<BufferId>, after: &[BufferId]) -> Vec<BufferId> {
    before
        .into_iter()
        .filter(|buffer_id| !after.contains(buffer_id))
        .collect()
}
