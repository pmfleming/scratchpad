use super::{DisplayTabSlot, TabManager};

impl TabManager {
    pub(crate) fn total_tab_slots(&self, settings_open: bool) -> usize {
        self.tabs.len() + usize::from(settings_open)
    }

    pub(crate) fn settings_slot_index(
        &self,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> Option<usize> {
        settings_open.then_some(settings_tab_index.min(self.tabs.len()))
    }

    pub(crate) fn tab_slot_is_settings(
        &self,
        slot_index: usize,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> bool {
        self.display_tab_slot(slot_index, settings_open, settings_tab_index)
            == Some(DisplayTabSlot::Settings)
    }

    pub(crate) fn workspace_index_for_slot(
        &self,
        slot_index: usize,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> Option<usize> {
        match self.display_tab_slot(slot_index, settings_open, settings_tab_index)? {
            DisplayTabSlot::Workspace(index) => Some(index),
            DisplayTabSlot::Settings => None,
        }
    }

    pub(crate) fn slot_for_workspace_index(
        &self,
        workspace_index: usize,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> usize {
        match self.settings_slot_index(settings_open, settings_tab_index) {
            Some(settings_index) if workspace_index >= settings_index => workspace_index + 1,
            _ => workspace_index,
        }
    }

    pub(crate) fn active_tab_slot_index(
        &self,
        showing_settings: bool,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> usize {
        if showing_settings {
            self.settings_slot_index(settings_open, settings_tab_index)
                .unwrap_or(self.tabs.len())
        } else {
            self.slot_for_workspace_index(self.active_tab_index, settings_open, settings_tab_index)
        }
    }

    pub(crate) fn display_tab_slot(
        &self,
        slot_index: usize,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> Option<DisplayTabSlot> {
        if slot_index >= self.total_tab_slots(settings_open) {
            return None;
        }

        match self.settings_slot_index(settings_open, settings_tab_index) {
            Some(settings_index) if slot_index == settings_index => Some(DisplayTabSlot::Settings),
            Some(settings_index) if slot_index > settings_index => {
                Some(DisplayTabSlot::Workspace(slot_index - 1))
            }
            _ => Some(DisplayTabSlot::Workspace(slot_index)),
        }
    }

    pub(crate) fn display_tab_slots(
        &self,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> Vec<DisplayTabSlot> {
        (0..self.total_tab_slots(settings_open))
            .filter_map(|slot_index| {
                self.display_tab_slot(slot_index, settings_open, settings_tab_index)
            })
            .collect()
    }
}
