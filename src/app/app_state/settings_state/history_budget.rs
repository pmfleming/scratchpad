use super::ScratchpadApp;

impl ScratchpadApp {
    pub(crate) fn apply_history_budget_to_open_buffers(&mut self) {
        let budget = self.state.app_settings.history.budget;
        for tab in self.tab_manager.tabs.as_mut_slice() {
            for buffer in tab.buffers_mut() {
                buffer.document_mut().set_history_budget(budget);
            }
        }
    }
    pub(crate) fn enforce_aggregate_text_history_budget(&mut self) {
        // Memory pressure must never discard durable edit tree nodes.
    }
}
