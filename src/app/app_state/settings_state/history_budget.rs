use super::ScratchpadApp;

impl ScratchpadApp {
    pub(crate) fn apply_history_budget_to_open_buffers(&mut self) {
        let budget = self.state.app_settings.history.budget;
        for tab in self.tab_manager.tabs.as_mut_slice() {
            for buffer in tab.buffers_mut() {
                buffer.document_mut().set_history_budget(budget);
            }
        }
        self.enforce_aggregate_text_history_budget();
    }
    pub(crate) fn enforce_aggregate_text_history_budget(&mut self) {
        let budget = self.state.app_settings.history.budget;
        crate::app::domain::buffer::text_cache::set_limits(
            budget.aggregate_byte_budget as usize,
            budget.per_file_byte_budget as usize,
        );
        if let Some(message) = crate::app::domain::buffer::text_cache::take_error() {
            self.state.status.set_warning_status_in_domain(
                crate::app::app_state::StatusDomain::History,
                message,
            );
        }
    }
}
