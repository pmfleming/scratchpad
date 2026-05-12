use crate::app::app_state::ScratchpadApp;
use crate::app::domain::DisplayTabSlot;
use std::collections::BTreeSet;

#[derive(Default)]
pub(crate) struct WorkspaceSelectionState {
    selected_slots: BTreeSet<usize>,
    anchor: Option<usize>,
}

impl WorkspaceSelectionState {
    pub(crate) fn contains(&self, slot_index: usize) -> bool {
        self.selected_slots.contains(&slot_index)
    }

    pub(crate) fn selected_slots(&self) -> impl Iterator<Item = usize> + '_ {
        self.selected_slots.iter().copied()
    }

    fn len(&self) -> usize {
        self.selected_slots.len()
    }

    fn reset(&mut self) {
        self.selected_slots.clear();
        self.anchor = None;
    }

    fn prune_invalid(&mut self, mut slot_exists: impl FnMut(usize) -> bool) {
        self.selected_slots
            .retain(|slot_index| slot_exists(*slot_index));
        if self
            .anchor
            .is_some_and(|slot_index| !slot_exists(slot_index))
        {
            self.anchor = None;
        }
    }

    fn ensure_active_slot(
        &mut self,
        active_slot: usize,
        mut slot_exists: impl FnMut(usize) -> bool,
    ) {
        self.prune_invalid(&mut slot_exists);
        self.selected_slots.insert(active_slot);
        if self
            .anchor
            .is_none_or(|slot_index| !slot_exists(slot_index))
        {
            self.anchor = Some(active_slot);
        }
    }

    fn select_only(&mut self, slot_index: usize) {
        self.reset();
        self.selected_slots.insert(slot_index);
        self.anchor = Some(slot_index);
    }

    fn toggle(&mut self, slot_index: usize) {
        if !self.selected_slots.remove(&slot_index) {
            self.selected_slots.insert(slot_index);
        }
        self.anchor = Some(slot_index);
    }

    fn select_range(
        &mut self,
        anchor: usize,
        slot_index: usize,
        mut slot_exists: impl FnMut(usize) -> bool,
    ) {
        let (start, end) = if anchor <= slot_index {
            (anchor, slot_index)
        } else {
            (slot_index, anchor)
        };
        self.reset();
        for candidate in start..=end {
            if slot_exists(candidate) {
                self.selected_slots.insert(candidate);
            }
        }
        self.anchor = Some(anchor);
    }

    fn anchor(&self) -> Option<usize> {
        self.anchor
    }

    fn dragged_slots(&self, source_slot: usize) -> Vec<usize> {
        if self.contains(source_slot) && self.len() > 1 {
            self.selected_slots().collect()
        } else {
            vec![source_slot]
        }
    }
}

impl ScratchpadApp {
    pub(crate) fn tab_slot_selected(&self, slot_index: usize) -> bool {
        self.state.workspace_selection.contains(slot_index)
    }

    pub(crate) fn ensure_active_tab_slot_selected(&mut self) {
        if self.total_tab_slots() == 0 {
            self.state.workspace_selection.reset();
            return;
        }

        let active_slot = self.active_tab_slot_index();
        let existing_slots = self.display_tab_slots();
        self.state
            .workspace_selection
            .ensure_active_slot(active_slot, |slot_index| {
                existing_slots.get(slot_index).is_some()
            });
    }

    fn tab_slot_exists(&self, slot_index: usize) -> bool {
        self.display_tab_slot(slot_index).is_some()
    }

    fn reset_tab_selection(&mut self) {
        self.state.workspace_selection.reset();
    }

    pub(crate) fn clear_tab_selection(&mut self) {
        self.reset_tab_selection();
        self.ensure_active_tab_slot_selected();
    }

    pub(crate) fn select_only_tab_slot(&mut self, slot_index: usize) {
        self.reset_tab_selection();
        if self.tab_slot_exists(slot_index) {
            self.state.workspace_selection.select_only(slot_index);
        }
    }

    pub(crate) fn toggle_tab_slot_selection(&mut self, slot_index: usize) {
        if !self.tab_slot_exists(slot_index) {
            self.clear_tab_selection();
            return;
        }

        self.state.workspace_selection.toggle(slot_index);
        self.ensure_active_tab_slot_selected();
    }

    pub(crate) fn select_tab_slot_range(&mut self, slot_index: usize) {
        if !self.tab_slot_exists(slot_index) {
            self.reset_tab_selection();
            self.ensure_active_tab_slot_selected();
            return;
        }

        let anchor = self
            .state
            .workspace_selection
            .anchor()
            .or_else(|| {
                self.tab_slot_exists(self.active_tab_slot_index())
                    .then_some(self.active_tab_slot_index())
            })
            .unwrap_or(slot_index);
        let existing_slots = self.display_tab_slots();
        self.state
            .workspace_selection
            .select_range(anchor, slot_index, |candidate| {
                existing_slots.get(candidate).is_some()
            });
    }

    pub(crate) fn dragged_tab_slots(&self, source_slot: usize) -> Vec<usize> {
        self.state.workspace_selection.dragged_slots(source_slot)
    }

    pub(crate) fn total_tab_slots(&self) -> usize {
        self.tab_manager.total_tab_slots(self.settings_tab_open())
    }

    pub(crate) fn tab_slot_is_settings(&self, slot_index: usize) -> bool {
        self.tab_manager.tab_slot_is_settings(
            slot_index,
            self.settings_tab_open(),
            self.state.settings_tab_index,
        )
    }

    pub(crate) fn workspace_index_for_slot(&self, slot_index: usize) -> Option<usize> {
        self.tab_manager.workspace_index_for_slot(
            slot_index,
            self.settings_tab_open(),
            self.state.settings_tab_index,
        )
    }

    pub(crate) fn slot_for_workspace_index(&self, workspace_index: usize) -> usize {
        self.tab_manager.slot_for_workspace_index(
            workspace_index,
            self.settings_tab_open(),
            self.state.settings_tab_index,
        )
    }

    pub(crate) fn active_tab_slot_index(&self) -> usize {
        self.tab_manager.active_tab_slot_index(
            self.showing_settings(),
            self.settings_tab_open(),
            self.state.settings_tab_index,
        )
    }

    fn display_tab_slot(&self, slot_index: usize) -> Option<DisplayTabSlot> {
        self.tab_manager.display_tab_slot(
            slot_index,
            self.settings_tab_open(),
            self.state.settings_tab_index,
        )
    }

    fn display_tab_slots(&self) -> Vec<DisplayTabSlot> {
        self.tab_manager
            .display_tab_slots(self.settings_tab_open(), self.state.settings_tab_index)
    }

    pub(crate) fn display_tab_name_at_slot(&self, slot_index: usize) -> Option<String> {
        match self.display_tab_slot(slot_index)? {
            DisplayTabSlot::Settings => Some("Settings".to_owned()),
            DisplayTabSlot::Workspace(workspace_index) => {
                let tab = self.tab_manager.tabs.as_slice().get(workspace_index)?;
                let duplicate_count = self
                    .tab_manager
                    .tabs
                    .as_slice()
                    .iter()
                    .filter(|candidate| candidate.buffer.name == tab.buffer.name)
                    .count();
                Some(crate::app::domain::tab::summary::full_display_name(
                    tab,
                    duplicate_count > 1,
                ))
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
            self.state.settings_tab_index = settings_index;
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
