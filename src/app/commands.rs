use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PendingAction, SplitAxis, SplitPath, ViewId};
use crate::app::services::file_controller::FileController;

mod dispatch;
mod tab_transfer;
#[cfg(test)]
mod tests;

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
    OpenUserManual,
    CloseSearch,
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
    fn active_buffer_name_or_missing(&self, index: usize) -> String {
        self.tabs().get(index).map_or_else(
            || "<missing>".to_owned(),
            |tab| tab.active_buffer().name.to_owned(),
        )
    }

    fn activate_tab(&mut self, index: usize) {
        if index >= self.tabs().len() {
            return;
        }

        self.reload_settings_before_workspace_change();
        self.activate_workspace_surface();
        self.tab_manager_mut().active_tab_index = index;
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
        self.reload_settings_if_closing_view(view_id);

        let index = self.active_tab_index();
        let tab_name = self.active_buffer_name_or_missing(index);
        let snapshot = self.capture_transaction_snapshot();
        let mut next_active_view = None;
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.close_view(view_id)
        {
            next_active_view = Some(tab.active_view_id);
        }
        if let Some(next_active_view) = next_active_view {
            self.begin_layout_transition();
            self.mark_search_dirty();
            self.request_focus_for_view(next_active_view);
            self.record_transaction("Close view", vec![tab_name.clone()], None, snapshot);
            self.mark_session_dirty();
        }
    }

    fn request_close_tab(&mut self, index: usize) {
        if index < self.tabs().len() {
            self.set_pending_action(Some(PendingAction::CloseTab(index)));
        }
    }

    fn reorder_tab_command(&mut self, from_index: usize, to_index: usize) {
        let moved_tab_description = self.describe_tab_at(from_index);
        let affected_items = vec![moved_tab_description];
        let snapshot = self.capture_coalesced_layout_snapshot("Reorder tab", &affected_items);
        if !self.tab_manager_mut().reorder_tab(from_index, to_index) {
            return;
        }
        self.begin_layout_transition();
        self.record_coalesced_layout_transaction("Reorder tab", affected_items, snapshot);
    }

    fn reorder_display_tab_command(&mut self, from_index: usize, to_index: usize) {
        if self.reorder_display_tab(from_index, to_index) {
            self.begin_layout_transition();
        }
    }

    fn resize_split_command(&mut self, path: SplitPath, ratio: f32) {
        let index = self.active_tab_index();
        let tab_name = self.active_buffer_name_or_missing(index);
        let affected_items = vec![tab_name];
        let snapshot = self.capture_coalesced_layout_snapshot("Resize split", &affected_items);
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.resize_split(path, ratio)
        {
            self.begin_layout_transition();
            self.record_coalesced_layout_transaction("Resize split", affected_items, snapshot);
            self.mark_session_dirty();
        }
    }

    fn split_active_view_command(&mut self, axis: SplitAxis, new_view_first: bool, ratio: f32) {
        let index = self.active_tab_index();
        let tab_name = self.active_buffer_name_or_missing(index);
        let snapshot = self.capture_transaction_snapshot();
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
            self.record_transaction("Split view", vec![tab_name.clone()], None, snapshot);
            self.mark_session_dirty();
        }
    }
}
