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

    fn tab_slot_exists(&self, slot_index: usize) -> bool {
        self.display_tab_slot(slot_index).is_some()
    }

    pub(crate) fn clear_tab_selection(&mut self) {
        self.selected_tab_slots.clear();
        self.tab_selection_anchor = None;
    }

    pub(crate) fn select_only_tab_slot(&mut self, slot_index: usize) {
        self.clear_tab_selection();
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
    }

    pub(crate) fn select_tab_slot_range(&mut self, slot_index: usize) {
        if !self.tab_slot_exists(slot_index) {
            self.clear_tab_selection();
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
        self.clear_tab_selection();
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

#[cfg(test)]
mod tests {
    use crate::app::app_state::ScratchpadApp;
    use crate::app::commands::AppCommand;
    use crate::app::domain::WorkspaceTab;
    use crate::app::services::session_store::SessionStore;

    fn test_app() -> ScratchpadApp {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());
        ScratchpadApp::with_session_store(session_store)
    }

    fn app_with_settings_between_tabs() -> ScratchpadApp {
        let mut app = test_app();
        app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
        app.append_tab(WorkspaceTab::untitled());
        app.tabs_mut()[1].buffer.name = "two.txt".to_owned();
        app.handle_command(AppCommand::OpenSettings);
        app.handle_command(AppCommand::ReorderDisplayTab {
            from_index: 2,
            to_index: 1,
        });
        app
    }

    #[test]
    fn settings_slot_can_be_selected() {
        let mut app = app_with_settings_between_tabs();

        app.select_only_tab_slot(1);

        assert!(app.tab_slot_selected(1));
    }

    #[test]
    fn range_selection_can_include_settings_slot() {
        let mut app = app_with_settings_between_tabs();

        app.select_only_tab_slot(0);
        app.select_tab_slot_range(2);

        assert!(app.tab_slot_selected(0));
        assert!(app.tab_slot_selected(1));
        assert!(app.tab_slot_selected(2));
    }
}
