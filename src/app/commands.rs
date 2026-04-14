use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PendingAction, SplitAxis, SplitPath, ViewId};
use crate::app::logging::LogLevel;

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
    PromoteViewToTab {
        view_id: ViewId,
    },
    PromoteTabFilesToTabs {
        index: usize,
    },
    NewTab,
    OpenFile,
    OpenFileHere,
    OpenSettings,
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
        let tab_description = self.describe_tab_at(index);
        self.activate_workspace_surface();
        self.tab_manager_mut().active_tab_index = index;
        self.tab_manager_mut().pending_scroll_to_active = true;
        self.request_focus_for_active_view();
        self.mark_session_dirty();
        self.log_event(
            LogLevel::Info,
            format!("Activated tab index {index}: {tab_description}"),
        );
    }

    fn activate_view_command(&mut self, view_id: ViewId) {
        self.reload_settings_if_switching_views(view_id);

        let index = self.active_tab_index();
        let tab_name = self.active_buffer_name_or_missing(index);
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.activate_view(view_id)
        {
            let previous_view_id = tab.active_view_id;
            self.request_focus_for_view(view_id);
            self.mark_session_dirty();
            self.log_event(
                LogLevel::Info,
                format!(
                    "Activated view {view_id} in tab '{tab_name}' (previous active view={previous_view_id})"
                ),
            );
        }
    }

    fn close_view_command(&mut self, view_id: ViewId) {
        self.reload_settings_if_closing_view(view_id);

        let index = self.active_tab_index();
        let tab_name = self.active_buffer_name_or_missing(index);
        let snapshot = self.capture_transaction_snapshot();
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.close_view(view_id)
        {
            let next_active_view = tab.active_view_id;
            let remaining_views = tab.views.len();
            self.request_focus_for_view(next_active_view);
            self.record_transaction("Close view", vec![tab_name.clone()], None, snapshot);
            self.mark_session_dirty();
            self.log_event(
                LogLevel::Info,
                format!(
                    "Closed view {view_id} in tab '{tab_name}' (remaining views={remaining_views}, active view={next_active_view})"
                ),
            );
        }
    }

    fn request_close_tab(&mut self, index: usize) {
        if index < self.tabs().len() {
            let tab_description = self.describe_tab_at(index);
            self.set_pending_action(Some(PendingAction::CloseTab(index)));
            self.log_event(
                LogLevel::Info,
                format!("Requested close for tab index {index}: {tab_description}"),
            );
        }
    }

    fn reorder_tab_command(&mut self, from_index: usize, to_index: usize) {
        let moved_tab_description = self.describe_tab_at(from_index);
        let snapshot = self.capture_transaction_snapshot();
        if !self.tab_manager_mut().reorder_tab(from_index, to_index) {
            return;
        }
        self.record_transaction(
            "Reorder tab",
            vec![moved_tab_description.clone()],
            None,
            snapshot,
        );
        self.log_event(
            LogLevel::Info,
            format!(
                "Reordered tab from index {from_index} to {to_index}: {moved_tab_description} (active tab index={})",
                self.active_tab_index()
            ),
        );
    }

    fn reorder_display_tab_command(&mut self, from_index: usize, to_index: usize) {
        let moved_tab_description = self
            .display_tab_name_at_slot(from_index)
            .unwrap_or_else(|| format!("tab#{from_index}<missing>"));
        if !self.reorder_display_tab(from_index, to_index) {
            return;
        }

        self.log_event(
            LogLevel::Info,
            format!(
                "Reordered displayed tab from slot {from_index} to {to_index}: {moved_tab_description}"
            ),
        );
    }

    fn resize_split_command(&mut self, path: SplitPath, ratio: f32) {
        let index = self.active_tab_index();
        let tab_name = self.active_buffer_name_or_missing(index);
        let path_description = format!("{:?}", path);
        let snapshot = self.capture_transaction_snapshot();
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.resize_split(path, ratio)
        {
            self.record_transaction("Resize split", vec![tab_name.clone()], None, snapshot);
            self.mark_session_dirty();
            self.log_event(
                LogLevel::Info,
                format!(
                    "Resized split in tab '{tab_name}' at path {path_description} to ratio {:.3}",
                    ratio.clamp(0.2, 0.8)
                ),
            );
        }
    }

    fn split_active_view_command(&mut self, axis: SplitAxis, new_view_first: bool, ratio: f32) {
        let index = self.active_tab_index();
        let tab_name = self.active_buffer_name_or_missing(index);
        let snapshot = self.capture_transaction_snapshot();
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab
                .split_active_view_with_placement(axis, new_view_first, ratio)
                .is_some()
        {
            let new_active_view = tab.active_view_id;
            let total_views = tab.views.len();
            self.request_focus_for_view(new_active_view);
            self.record_transaction("Split view", vec![tab_name.clone()], None, snapshot);
            self.mark_session_dirty();
            self.log_event(
                LogLevel::Info,
                format!(
                    "Split active view in tab '{tab_name}' with axis={axis:?}, new_view_first={new_view_first}, ratio={:.3}; new active view={new_active_view}, total views={total_views}",
                    ratio.clamp(0.2, 0.8)
                ),
            );
        }
    }
}
