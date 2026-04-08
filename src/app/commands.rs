use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PendingAction, SplitAxis, SplitPath, ViewId};
use crate::app::logging::LogLevel;

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
    NewTab,
    OpenFile,
    ReorderTab {
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
    pub(crate) fn handle_command(&mut self, command: AppCommand) {
        match command {
            AppCommand::ActivateTab { index } => self.activate_tab(index),
            AppCommand::ActivateView { view_id } => self.activate_view_command(view_id),
            AppCommand::CloseTab { index } => self.perform_close_tab(index),
            AppCommand::CloseView { view_id } => self.close_view_command(view_id),
            AppCommand::NewTab => self.new_tab(),
            AppCommand::OpenFile => self.open_file(),
            AppCommand::ReorderTab {
                from_index,
                to_index,
            } => self.reorder_tab_command(from_index, to_index),
            AppCommand::RequestCloseTab { index } => self.request_close_tab(index),
            AppCommand::ResizeSplit { path, ratio } => self.resize_split_command(path, ratio),
            AppCommand::SaveFile => self.save_file(),
            AppCommand::SaveFileAs => self.save_file_as(),
            AppCommand::SplitActiveView {
                axis,
                new_view_first,
                ratio,
            } => self.split_active_view_command(axis, new_view_first, ratio),
        }
    }

    fn activate_tab(&mut self, index: usize) {
        if index >= self.tabs().len() {
            return;
        }

        let tab_description = self.describe_tab_at(index);
        self.tab_manager_mut().active_tab_index = index;
        self.tab_manager_mut().pending_scroll_to_active = true;
        self.mark_session_dirty();
        self.log_event(
            LogLevel::Info,
            format!("Activated tab index {index}: {tab_description}"),
        );
    }

    fn activate_view_command(&mut self, view_id: ViewId) {
        let index = self.active_tab_index();
        let tab_name = self
            .tabs()
            .get(index)
            .map(|tab| tab.buffer.name.clone())
            .unwrap_or_else(|| "<missing>".to_owned());
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.view(view_id).is_some()
        {
            let previous_view_id = tab.active_view_id;
            tab.active_view_id = view_id;
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
        let index = self.active_tab_index();
        let tab_name = self
            .tabs()
            .get(index)
            .map(|tab| tab.buffer.name.clone())
            .unwrap_or_else(|| "<missing>".to_owned());
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.close_view(view_id)
        {
            let next_active_view = tab.active_view_id;
            let remaining_views = tab.views.len();
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
        if !self.tab_manager_mut().reorder_tab(from_index, to_index) {
            return;
        }
        self.log_event(
            LogLevel::Info,
            format!(
                "Reordered tab from index {from_index} to {to_index}: {moved_tab_description} (active tab index={})",
                self.active_tab_index()
            ),
        );
    }

    fn resize_split_command(&mut self, path: SplitPath, ratio: f32) {
        let index = self.active_tab_index();
        let tab_name = self
            .tabs()
            .get(index)
            .map(|tab| tab.buffer.name.clone())
            .unwrap_or_else(|| "<missing>".to_owned());
        let path_description = format!("{:?}", path);
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.resize_split(path, ratio)
        {
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
        let tab_name = self
            .tabs()
            .get(index)
            .map(|tab| tab.buffer.name.clone())
            .unwrap_or_else(|| "<missing>".to_owned());
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab
                .split_active_view_with_placement(axis, new_view_first, ratio)
                .is_some()
        {
            let new_active_view = tab.active_view_id;
            let total_views = tab.views.len();
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
