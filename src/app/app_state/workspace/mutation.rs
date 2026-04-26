use super::super::ScratchpadApp;
use crate::app::domain::{BufferId, CursorRevealMode};
use crate::app::transactions::TransactionSnapshot;
use crate::app::ui::editor_content::native_editor::{
    cut_selected_text, select_all_cursor, selected_text,
};

impl ScratchpadApp {
    pub(crate) fn active_buffer_can_undo_text_operation(&self) -> bool {
        self.active_tab()
            .is_some_and(|tab| tab.active_buffer().document().operation_undo_depth() > 0)
    }

    pub(crate) fn active_buffer_can_redo_text_operation(&self) -> bool {
        self.active_tab()
            .is_some_and(|tab| tab.active_buffer().document().operation_redo_depth() > 0)
    }

    pub(crate) fn finalize_active_buffer_text_mutation(
        &mut self,
        active_tab_index: usize,
        active_buffer_id: BufferId,
        active_buffer_label: String,
        transaction_snapshot: TransactionSnapshot,
    ) {
        let tab = &mut self.tabs_mut()[active_tab_index];
        let latest_edit = tab.buffer.document().latest_operation_record().cloned();
        tab.buffer
            .refresh_text_metadata_after_operation(latest_edit.as_ref());
        let has_control_chars = tab.buffer.artifact_summary.has_control_chars();
        for view in &mut tab.views {
            if !has_control_chars {
                view.show_control_chars = false;
            }
        }
        tab.buffer.is_dirty = true;
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
            latest_edit,
        );
        self.mark_search_dirty();
        self.mark_session_dirty();
        self.note_settings_toml_edit(active_tab_index);
    }

    pub(crate) fn undo_active_buffer_text_operation(&mut self) -> bool {
        self.apply_active_buffer_text_operation(true)
    }

    pub(crate) fn redo_active_buffer_text_operation(&mut self) -> bool {
        self.apply_active_buffer_text_operation(false)
    }

    pub(crate) fn select_all_in_active_view(&mut self) -> bool {
        let active_tab_index = self.active_tab_index();
        let total_chars = match self.active_tab() {
            Some(tab) => tab.active_buffer().current_file_length().chars,
            None => return false,
        };
        let selection = select_all_cursor(total_chars);

        let tab = &mut self.tabs_mut()[active_tab_index];
        let Some(view) = tab.active_view_mut() else {
            return false;
        };
        view.cursor_range = Some(selection);
        view.pending_cursor_range = Some(selection);
        view.request_cursor_reveal(CursorRevealMode::Center);
        tab.active_buffer_mut().active_selection =
            (!selection.is_empty()).then_some(selection.as_sorted_char_range());
        true
    }

    pub(crate) fn copy_selected_text_in_active_view(&self) -> Option<String> {
        let tab = self.active_tab()?;
        let view = tab.active_view()?;
        let buffer = tab.buffer_for_view(view.id)?;
        selected_text(buffer, view.cursor_range?)
    }

    pub(crate) fn cut_selected_text_in_active_view(&mut self) -> Option<String> {
        let active_tab_index = self.active_tab_index();
        let (active_buffer_id, active_buffer_label, active_view_id) = {
            let tab = self.active_tab()?;
            (
                tab.active_buffer().id,
                self.active_buffer_transaction_label()?,
                tab.active_view_id,
            )
        };
        let transaction_snapshot = self.capture_transaction_snapshot();

        let (next_selection, selected_text) = {
            let tab = &mut self.tabs_mut()[active_tab_index];
            let (buffer, view) = tab.buffer_and_view_mut(active_view_id)?;
            let current_selection = view.cursor_range?;
            let (next_selection, selected_text) = cut_selected_text(buffer, current_selection)?;
            view.cursor_range = Some(next_selection);
            view.pending_cursor_range = Some(next_selection);
            view.request_cursor_reveal(CursorRevealMode::KeepVisible);
            buffer.active_selection = None;
            (next_selection, selected_text)
        };

        self.finalize_active_buffer_text_mutation(
            active_tab_index,
            active_buffer_id,
            active_buffer_label,
            transaction_snapshot,
        );
        self.refresh_search_state();
        self.select_next_active_buffer_match_from(next_selection.primary.index);
        Some(selected_text)
    }

    fn apply_active_buffer_text_operation(&mut self, undo: bool) -> bool {
        let active_tab_index = self.active_tab_index();
        let active_buffer_id = match self.active_tab() {
            Some(tab) => tab.active_buffer().id,
            None => return false,
        };
        let active_buffer_label = match self.active_buffer_transaction_label() {
            Some(label) => label,
            None => return false,
        };
        let transaction_snapshot = self.capture_transaction_snapshot();

        let selection = {
            let tab = &mut self.tabs_mut()[active_tab_index];
            let Some(selection) = ({
                let buffer = &mut tab.buffer;
                if undo {
                    buffer.undo_last_text_operation()
                } else {
                    buffer.redo_last_text_operation()
                }
            }) else {
                return false;
            };

            if let Some(view) = tab.active_view_mut() {
                view.cursor_range = Some(selection);
                view.pending_cursor_range = Some(selection);
                view.request_cursor_reveal(CursorRevealMode::Center);
            }
            selection
        };

        self.finalize_active_buffer_text_mutation(
            active_tab_index,
            active_buffer_id,
            active_buffer_label.clone(),
            transaction_snapshot,
        );
        self.refresh_search_state();
        self.select_next_active_buffer_match_from(selection.primary.index);
        let action = if undo { "Undid" } else { "Redid" };
        self.set_info_status(format!(
            "{action} last text operation in {active_buffer_label}."
        ));
        true
    }
}
