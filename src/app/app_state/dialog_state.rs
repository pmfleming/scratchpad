use super::{PendingTabContextMenu, StartupRestoreConflict, TabRenameState};

#[derive(Default)]
pub(crate) struct EncodingDialogState {
    pub(crate) open: bool,
    pub(crate) choice: String,
}

impl EncodingDialogState {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            choice: "UTF-8".to_owned(),
        }
    }

    pub(crate) fn open_with_choice(&mut self, choice: String) {
        self.choice = choice;
        self.open = true;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn choice_mut(&mut self) -> &mut String {
        &mut self.choice
    }

    pub(crate) fn take_choice(&mut self) -> String {
        std::mem::take(&mut self.choice)
    }

    pub(crate) fn restore_choice(&mut self, choice: String) {
        self.choice = choice;
    }
}

#[derive(Default)]
pub(crate) struct TextHistoryDialogState {
    pub(crate) open: bool,
}

impl TextHistoryDialogState {
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

#[derive(Default)]
pub(crate) struct StatusHistoryDialogState {
    pub(crate) open: bool,
}

impl StatusHistoryDialogState {
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

#[derive(Default)]
pub(crate) struct DialogState {
    pub(crate) encoding: EncodingDialogState,
    pub(crate) text_history: TextHistoryDialogState,
    pub(crate) status_history: StatusHistoryDialogState,
    pub(crate) tab_rename: Option<TabRenameState>,
    pub(crate) pending_tab_context_menu: Option<PendingTabContextMenu>,
    pub(crate) startup_restore_conflicts: Vec<StartupRestoreConflict>,
}

impl DialogState {
    pub(crate) fn new() -> Self {
        Self {
            encoding: EncodingDialogState::new(),
            ..Self::default()
        }
    }

    pub(crate) fn any_modal_open(&self) -> bool {
        self.encoding.is_open() || self.text_history.is_open() || self.status_history.is_open()
    }

    pub(crate) fn begin_tab_rename(&mut self, state: TabRenameState) {
        self.tab_rename = Some(state);
    }

    pub(crate) fn cancel_tab_rename(&mut self) {
        self.tab_rename = None;
    }

    pub(crate) fn request_tab_rename_focus(&mut self) {
        if let Some(rename_state) = self.tab_rename.as_mut() {
            rename_state.request_focus = true;
        }
    }

    pub(crate) fn take_tab_rename_focus_request(&mut self) -> bool {
        self.tab_rename
            .as_mut()
            .map(|rename_state| std::mem::take(&mut rename_state.request_focus))
            .unwrap_or(false)
    }

    pub(crate) fn tab_rename_draft_mut(&mut self) -> Option<&mut String> {
        self.tab_rename
            .as_mut()
            .map(|rename_state| &mut rename_state.draft)
    }

    pub(crate) fn tab_rename(&self) -> Option<&TabRenameState> {
        self.tab_rename.as_ref()
    }

    pub(crate) fn set_startup_restore_conflicts(&mut self, conflicts: Vec<StartupRestoreConflict>) {
        self.startup_restore_conflicts = conflicts;
    }

    pub(crate) fn current_startup_restore_conflict(&self) -> Option<&StartupRestoreConflict> {
        self.startup_restore_conflicts.first()
    }

    pub(crate) fn dismiss_current_startup_restore_conflict(&mut self) {
        if !self.startup_restore_conflicts.is_empty() {
            self.startup_restore_conflicts.remove(0);
        }
    }

    pub(crate) fn take_current_startup_restore_conflict(
        &mut self,
    ) -> Option<StartupRestoreConflict> {
        if self.startup_restore_conflicts.is_empty() {
            None
        } else {
            Some(self.startup_restore_conflicts.remove(0))
        }
    }

    pub(crate) fn open_pending_tab_context_menu(&mut self, pending: PendingTabContextMenu) {
        self.pending_tab_context_menu = Some(pending);
    }

    pub(crate) fn pending_tab_context_menu_for_slot(
        &self,
        slot_index: usize,
    ) -> Option<PendingTabContextMenu> {
        self.pending_tab_context_menu
            .filter(|pending| pending.slot_index == slot_index)
    }

    pub(crate) fn store_pending_tab_context_menu(&mut self, pending: PendingTabContextMenu) {
        self.pending_tab_context_menu = pending.open.then_some(pending);
    }
}
