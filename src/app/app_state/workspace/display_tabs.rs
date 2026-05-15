use crate::app::app_state::ScratchpadApp;
use crate::app::domain::DisplayTabSlot;
use std::collections::{BTreeSet, HashMap};

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

pub(crate) fn tab_slot_selected(app: &ScratchpadApp, slot_index: usize) -> bool {
    app.state.workspace_selection.contains(slot_index)
}

pub(crate) fn ensure_active_tab_slot_selected(app: &mut ScratchpadApp) {
    if total_tab_slots(app) == 0 {
        app.state.workspace_selection.reset();
        return;
    }

    let active_slot = active_tab_slot_index(app);
    let existing_slots = display_tab_slots(app);
    app.state
        .workspace_selection
        .ensure_active_slot(active_slot, |slot_index| {
            existing_slots.get(slot_index).is_some()
        });
}

fn tab_slot_exists(app: &ScratchpadApp, slot_index: usize) -> bool {
    display_tab_slot(app, slot_index).is_some()
}

fn reset_tab_selection(app: &mut ScratchpadApp) {
    app.state.workspace_selection.reset();
}

pub(crate) fn clear_tab_selection(app: &mut ScratchpadApp) {
    reset_tab_selection(app);
    ensure_active_tab_slot_selected(app);
}

pub(crate) fn select_only_tab_slot(app: &mut ScratchpadApp, slot_index: usize) {
    reset_tab_selection(app);
    if tab_slot_exists(app, slot_index) {
        app.state.workspace_selection.select_only(slot_index);
    }
}

pub(crate) fn toggle_tab_slot_selection(app: &mut ScratchpadApp, slot_index: usize) {
    if !tab_slot_exists(app, slot_index) {
        clear_tab_selection(app);
        return;
    }

    app.state.workspace_selection.toggle(slot_index);
    ensure_active_tab_slot_selected(app);
}

pub(crate) fn select_tab_slot_range(app: &mut ScratchpadApp, slot_index: usize) {
    if !tab_slot_exists(app, slot_index) {
        reset_tab_selection(app);
        ensure_active_tab_slot_selected(app);
        return;
    }

    let anchor = app
        .state
        .workspace_selection
        .anchor()
        .or_else(|| {
            tab_slot_exists(app, active_tab_slot_index(app)).then_some(active_tab_slot_index(app))
        })
        .unwrap_or(slot_index);
    let existing_slots = display_tab_slots(app);
    app.state
        .workspace_selection
        .select_range(anchor, slot_index, |candidate| {
            existing_slots.get(candidate).is_some()
        });
}

pub(crate) fn dragged_tab_slots(app: &ScratchpadApp, source_slot: usize) -> Vec<usize> {
    app.state.workspace_selection.dragged_slots(source_slot)
}

pub(crate) fn total_tab_slots(app: &ScratchpadApp) -> usize {
    app.tab_manager
        .total_tab_slots(crate::app::app_state::settings_state::settings_tab_open(
            app,
        ))
}

pub(crate) fn tab_slot_is_settings(app: &ScratchpadApp, slot_index: usize) -> bool {
    app.tab_manager.tab_slot_is_settings(
        slot_index,
        crate::app::app_state::settings_state::settings_tab_open(app),
        app.state.settings_tab_index,
    )
}

pub(crate) fn workspace_index_for_slot(app: &ScratchpadApp, slot_index: usize) -> Option<usize> {
    app.tab_manager.workspace_index_for_slot(
        slot_index,
        crate::app::app_state::settings_state::settings_tab_open(app),
        app.state.settings_tab_index,
    )
}

pub(crate) fn slot_for_workspace_index(app: &ScratchpadApp, workspace_index: usize) -> usize {
    app.tab_manager.slot_for_workspace_index(
        workspace_index,
        crate::app::app_state::settings_state::settings_tab_open(app),
        app.state.settings_tab_index,
    )
}

pub(crate) fn active_tab_slot_index(app: &ScratchpadApp) -> usize {
    app.tab_manager.active_tab_slot_index(
        crate::app::app_state::settings_state::showing_settings(app),
        crate::app::app_state::settings_state::settings_tab_open(app),
        app.state.settings_tab_index,
    )
}

fn display_tab_slot(app: &ScratchpadApp, slot_index: usize) -> Option<DisplayTabSlot> {
    app.tab_manager.display_tab_slot(
        slot_index,
        crate::app::app_state::settings_state::settings_tab_open(app),
        app.state.settings_tab_index,
    )
}

fn display_tab_slots(app: &ScratchpadApp) -> Vec<DisplayTabSlot> {
    app.tab_manager.display_tab_slots(
        crate::app::app_state::settings_state::settings_tab_open(app),
        app.state.settings_tab_index,
    )
}

pub(crate) fn display_tab_name_at_slot(app: &ScratchpadApp, slot_index: usize) -> Option<String> {
    match display_tab_slot(app, slot_index)? {
        DisplayTabSlot::Settings => Some("Settings".to_owned()),
        DisplayTabSlot::Workspace(workspace_index) => {
            let tabs = app.tab_manager.tabs.as_slice();
            let tab = tabs.get(workspace_index)?;
            let has_duplicate = tabs.iter().enumerate().any(|(candidate_index, candidate)| {
                candidate_index != workspace_index
                    && candidate.buffers.buffer.name == tab.buffers.buffer.name
            });
            Some(crate::app::domain::tab::summary::full_display_name(
                tab,
                has_duplicate,
            ))
        }
    }
}

pub(crate) fn display_tab_name_at_slot_with_counts(
    app: &ScratchpadApp,
    slot_index: usize,
    duplicate_name_counts: &HashMap<String, usize>,
) -> Option<String> {
    match display_tab_slot(app, slot_index)? {
        DisplayTabSlot::Settings => Some("Settings".to_owned()),
        DisplayTabSlot::Workspace(workspace_index) => {
            let tab = app.tab_manager.tabs.as_slice().get(workspace_index)?;
            let has_duplicate = duplicate_name_counts
                .get(&tab.buffers.buffer.name)
                .is_some_and(|count| *count > 1);
            Some(crate::app::domain::tab::summary::full_display_name(
                tab,
                has_duplicate,
            ))
        }
    }
}

pub(crate) fn reorder_display_tab(
    app: &mut ScratchpadApp,
    from_slot: usize,
    to_slot: usize,
) -> bool {
    let total_slots = total_tab_slots(app);
    if from_slot >= total_slots || to_slot >= total_slots || from_slot == to_slot {
        return false;
    }

    let mut display_slots = display_tab_slots(app);
    let moved_slot = display_slots.remove(from_slot);
    display_slots.insert(to_slot, moved_slot);
    apply_display_tab_order(app, display_slots);
    true
}

pub(crate) fn reorder_display_tab_group(
    app: &mut ScratchpadApp,
    mut from_slots: Vec<usize>,
    to_slot: usize,
) -> bool {
    let total_slots = total_tab_slots(app);
    if from_slots.is_empty() || to_slot > total_slots {
        return false;
    }

    from_slots.sort_unstable();
    from_slots.dedup();
    if from_slots.iter().any(|slot| *slot >= total_slots) {
        return false;
    }

    let display_slots = display_tab_slots(app);
    let adjusted_to_slot =
        to_slot.saturating_sub(from_slots.iter().filter(|slot| **slot < to_slot).count());
    let mut moved_slots = Vec::with_capacity(from_slots.len());
    let mut remaining_slots = Vec::with_capacity(display_slots.len() - from_slots.len());
    let mut next_moved_slot = from_slots.iter().copied().peekable();
    for (slot_index, slot) in display_slots.iter().copied().enumerate() {
        if next_moved_slot.peek() == Some(&slot_index) {
            moved_slots.push(slot);
            next_moved_slot.next();
        } else {
            remaining_slots.push(slot);
        }
    }

    let mut next_slots = remaining_slots;
    let insert_index = adjusted_to_slot.min(next_slots.len());
    next_slots.splice(insert_index..insert_index, moved_slots);

    if next_slots == display_slots {
        return false;
    }

    apply_display_tab_order(app, next_slots);
    true
}

fn apply_display_tab_order(app: &mut ScratchpadApp, display_slots: Vec<DisplayTabSlot>) {
    if let Some(settings_index) = display_slots
        .iter()
        .position(|slot| *slot == DisplayTabSlot::Settings)
    {
        app.state.settings_tab_index = settings_index;
    }

    let workspace_order = display_slots
        .into_iter()
        .filter_map(|slot| match slot {
            DisplayTabSlot::Workspace(index) => Some(index),
            DisplayTabSlot::Settings => None,
        })
        .collect::<Vec<_>>();

    app.apply_workspace_tab_order(workspace_order);
}
