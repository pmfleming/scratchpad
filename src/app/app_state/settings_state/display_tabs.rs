use crate::app::app_state::ScratchpadApp;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DisplayTabSlot {
    Workspace(usize),
    Settings,
}

impl ScratchpadApp {
    pub(crate) fn tab_slot_selected(&self, slot_index: usize) -> bool {
        self.selected_tab_slots.contains(&slot_index)
    }

    pub(crate) fn ensure_active_tab_slot_selected(&mut self) {
        let invalid_slots = self
            .selected_tab_slots
            .iter()
            .copied()
            .filter(|slot_index| !self.tab_slot_exists(*slot_index))
            .collect::<Vec<_>>();
        for slot_index in invalid_slots {
            self.selected_tab_slots.remove(&slot_index);
        }

        if self.total_tab_slots() == 0 {
            self.tab_selection_anchor = None;
            return;
        }

        let active_slot = self.active_tab_slot_index();
        self.selected_tab_slots.insert(active_slot);
        if self
            .tab_selection_anchor
            .is_none_or(|slot_index| !self.tab_slot_exists(slot_index))
        {
            self.tab_selection_anchor = Some(active_slot);
        }
    }

    fn tab_slot_exists(&self, slot_index: usize) -> bool {
        self.display_tab_slot(slot_index).is_some()
    }

    fn reset_tab_selection(&mut self) {
        self.selected_tab_slots.clear();
        self.tab_selection_anchor = None;
    }

    pub(crate) fn clear_tab_selection(&mut self) {
        self.reset_tab_selection();
        self.ensure_active_tab_slot_selected();
    }

    pub(crate) fn select_only_tab_slot(&mut self, slot_index: usize) {
        self.reset_tab_selection();
        if self.tab_slot_exists(slot_index) {
            self.selected_tab_slots.insert(slot_index);
            self.tab_selection_anchor = Some(slot_index);
        }
    }

    pub(crate) fn toggle_tab_slot_selection(&mut self, slot_index: usize) {
        if !self.tab_slot_exists(slot_index) {
            self.clear_tab_selection();
            return;
        }

        if !self.selected_tab_slots.remove(&slot_index) {
            self.selected_tab_slots.insert(slot_index);
        }
        self.tab_selection_anchor = Some(slot_index);
        self.ensure_active_tab_slot_selected();
    }

    pub(crate) fn select_tab_slot_range(&mut self, slot_index: usize) {
        if !self.tab_slot_exists(slot_index) {
            self.reset_tab_selection();
            self.ensure_active_tab_slot_selected();
            return;
        }

        let anchor = self
            .tab_selection_anchor
            .or_else(|| {
                self.tab_slot_exists(self.active_tab_slot_index())
                    .then_some(self.active_tab_slot_index())
            })
            .unwrap_or(slot_index);
        let (start, end) = if anchor <= slot_index {
            (anchor, slot_index)
        } else {
            (slot_index, anchor)
        };
        self.reset_tab_selection();
        for candidate in start..=end {
            if self.tab_slot_exists(candidate) {
                self.selected_tab_slots.insert(candidate);
            }
        }
        self.tab_selection_anchor = Some(anchor);
    }

    pub(crate) fn dragged_tab_slots(&self, source_slot: usize) -> Vec<usize> {
        if self.selected_tab_slots.contains(&source_slot) && self.selected_tab_slots.len() > 1 {
            self.selected_tab_slots.iter().copied().collect()
        } else {
            vec![source_slot]
        }
    }

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

    pub(crate) fn reorder_display_tab_group(
        &mut self,
        mut from_slots: Vec<usize>,
        to_slot: usize,
    ) -> bool {
        let total_slots = self.total_tab_slots();
        if from_slots.is_empty() || to_slot > total_slots {
            return false;
        }

        from_slots.sort_unstable();
        from_slots.dedup();
        if from_slots.iter().any(|slot| *slot >= total_slots) {
            return false;
        }

        let display_slots = self.display_tab_slots();
        let moved_slots = from_slots
            .iter()
            .map(|slot| display_slots[*slot])
            .collect::<Vec<_>>();
        let adjusted_to_slot =
            to_slot.saturating_sub(from_slots.iter().filter(|slot| **slot < to_slot).count());
        let remaining_slots = display_slots
            .into_iter()
            .enumerate()
            .filter_map(|(slot_index, slot)| (!from_slots.contains(&slot_index)).then_some(slot))
            .collect::<Vec<_>>();

        let mut next_slots = remaining_slots;
        let insert_index = adjusted_to_slot.min(next_slots.len());
        next_slots.splice(insert_index..insert_index, moved_slots);

        if next_slots == self.display_tab_slots() {
            return false;
        }

        self.apply_display_tab_order(next_slots);
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
