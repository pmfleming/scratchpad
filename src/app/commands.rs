use crate::app::app_state::{PendingAction, ScratchpadApp};
use crate::app::domain::{SplitAxis, SplitPath, ViewId};

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
        if index >= self.tabs.len() {
            return;
        }

        self.active_tab_index = index;
        self.pending_scroll_to_active = true;
        self.mark_session_dirty();
    }

    fn activate_view_command(&mut self, view_id: ViewId) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index)
            && tab.view(view_id).is_some()
        {
            tab.active_view_id = view_id;
            self.mark_session_dirty();
        }
    }

    fn close_view_command(&mut self, view_id: ViewId) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index)
            && tab.close_view(view_id)
        {
            self.mark_session_dirty();
        }
    }

    fn request_close_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.pending_action = Some(PendingAction::CloseTab(index));
        }
    }

    fn resize_split_command(&mut self, path: SplitPath, ratio: f32) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index)
            && tab.resize_split(path, ratio)
        {
            self.mark_session_dirty();
        }
    }

    fn split_active_view_command(&mut self, axis: SplitAxis, new_view_first: bool, ratio: f32) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index)
            && tab
                .split_active_view_with_placement(axis, new_view_first, ratio)
                .is_some()
        {
            self.mark_session_dirty();
        }
    }
}
