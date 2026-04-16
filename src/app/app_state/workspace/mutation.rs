use super::super::ScratchpadApp;
use crate::app::domain::BufferId;
use crate::app::transactions::TransactionSnapshot;

impl ScratchpadApp {
    pub(crate) fn finalize_active_buffer_text_mutation(
        &mut self,
        active_tab_index: usize,
        active_buffer_id: BufferId,
        active_buffer_label: String,
        transaction_snapshot: TransactionSnapshot,
    ) {
        let tab = &mut self.tabs_mut()[active_tab_index];
        tab.buffer.refresh_text_metadata();
        let has_control_chars = tab.buffer.artifact_summary.has_control_chars();
        for view in &mut tab.views {
            if !has_control_chars {
                view.show_control_chars = false;
            }
        }
        tab.buffer.is_dirty = true;
        let current_text = tab.buffer.text().to_owned();
        let warning_message = tab
            .buffer
            .artifact_summary
            .status_text()
            .map(|message| format!("{message}; raw-text editing remains enabled"));
        let _ = tab;

        if let Some(message) = warning_message {
            self.set_warning_status(message);
        } else {
            self.clear_status_message();
        }
        self.record_text_edit_transaction(
            active_buffer_id,
            active_buffer_label,
            transaction_snapshot,
            current_text,
        );
        self.mark_search_dirty();
        self.mark_session_dirty();
        self.note_settings_toml_edit(active_tab_index);
    }
}
