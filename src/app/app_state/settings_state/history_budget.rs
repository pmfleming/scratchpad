use super::ScratchpadApp;

impl ScratchpadApp {
    pub(crate) fn apply_history_budget_to_open_buffers(&mut self) {
        let budget = self.state.app_settings.history.budget;
        let store = self.state.persistence.session_store.opened_chunk_store();
        for tab in self.tab_manager.tabs.as_mut_slice() {
            for buffer in tab.buffers_mut() {
                if let Some(store) = &store
                    && let Err(cause) = buffer.document_mut().bind_store(store.clone())
                {
                    log::warn!("Could not bind live text to its session: {cause}");
                }
                buffer.document_mut().set_history_budget(budget);
            }
        }
        self.enforce_aggregate_text_history_budget();
    }
    pub(crate) fn enforce_aggregate_text_history_budget(&mut self) {
        let budget = self.state.app_settings.history.budget;
        if let Some(store) = self.state.persistence.session_store.opened_chunk_store() {
            store.set_limits(
                budget.aggregate_byte_budget as usize,
                budget.per_file_byte_budget as usize,
            );
        }
    }
}
