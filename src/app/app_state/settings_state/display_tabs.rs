use crate::app::app_state::ScratchpadApp;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DisplayTabSlot {
    Workspace(usize),
    Settings,
}

impl ScratchpadApp {
    pub(crate) fn total_tab_slots(&self) -> usize {
        self.tabs().len() + usize::from(self.settings_tab_open())
    }

    pub(crate) fn settings_slot_index(&self) -> Option<usize> {
        self.settings_tab_open()
            .then_some(self.settings_tab_index.min(self.tabs().len()))
    }

    pub(crate) fn tab_slot_is_settings(&self, slot_index: usize) -> bool {
        self.display_tab_slot(slot_index) == Some(DisplayTabSlot::Settings)
    }

    pub(crate) fn workspace_index_for_slot(&self, slot_index: usize) -> Option<usize> {
        match self.display_tab_slot(slot_index)? {
            DisplayTabSlot::Workspace(index) => Some(index),
            DisplayTabSlot::Settings => None,
        }
    }

    pub(crate) fn slot_for_workspace_index(&self, workspace_index: usize) -> usize {
        match self.settings_slot_index() {
            Some(settings_index) if workspace_index >= settings_index => workspace_index + 1,
            _ => workspace_index,
        }
    }

    pub(crate) fn active_tab_slot_index(&self) -> usize {
        if self.showing_settings() {
            self.settings_slot_index()
                .unwrap_or_else(|| self.tabs().len())
        } else {
            self.slot_for_workspace_index(self.active_tab_index())
        }
    }

    fn display_tab_slot(&self, slot_index: usize) -> Option<DisplayTabSlot> {
        if slot_index >= self.total_tab_slots() {
            return None;
        }

        match self.settings_slot_index() {
            Some(settings_index) if slot_index == settings_index => Some(DisplayTabSlot::Settings),
            Some(settings_index) if slot_index > settings_index => {
                Some(DisplayTabSlot::Workspace(slot_index - 1))
            }
            _ => Some(DisplayTabSlot::Workspace(slot_index)),
        }
    }

    fn display_tab_slots(&self) -> Vec<DisplayTabSlot> {
        (0..self.total_tab_slots())
            .filter_map(|slot_index| self.display_tab_slot(slot_index))
            .collect()
    }

    pub(crate) fn display_tab_name_at_slot(&self, slot_index: usize) -> Option<String> {
        match self.display_tab_slot(slot_index)? {
            DisplayTabSlot::Settings => Some("Settings".to_owned()),
            DisplayTabSlot::Workspace(workspace_index) => {
                let tab = self.tabs().get(workspace_index)?;
                let duplicate_count = self
                    .tabs()
                    .iter()
                    .filter(|candidate| candidate.buffer.name == tab.buffer.name)
                    .count();
                Some(tab.full_display_name(duplicate_count > 1))
            }
        }
    }

    pub(crate) fn reorder_display_tab(&mut self, from_slot: usize, to_slot: usize) -> bool {
        let total_slots = self.total_tab_slots();
        if from_slot >= total_slots || to_slot >= total_slots || from_slot == to_slot {
            return false;
        }

        let mut display_slots = self.display_tab_slots();
        let moved_slot = display_slots.remove(from_slot);
        display_slots.insert(to_slot, moved_slot);
        self.apply_display_tab_order(display_slots);
        true
    }

    fn apply_display_tab_order(&mut self, display_slots: Vec<DisplayTabSlot>) {
        if let Some(settings_index) = display_slots
            .iter()
            .position(|slot| *slot == DisplayTabSlot::Settings)
        {
            self.settings_tab_index = settings_index;
        }

        let workspace_order = display_slots
            .into_iter()
            .filter_map(|slot| match slot {
                DisplayTabSlot::Workspace(index) => Some(index),
                DisplayTabSlot::Settings => None,
            })
            .collect::<Vec<_>>();

        self.apply_workspace_tab_order(workspace_order);
    }
}
