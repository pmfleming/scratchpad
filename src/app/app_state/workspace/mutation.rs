use super::super::ScratchpadApp;
use crate::app::domain::buffer::TextHistoryEvent;
use crate::app::domain::{BufferId, CursorRevealMode};
use crate::app::text_history::TextHistorySource;
use crate::app::ui::editor_content::native_editor::{
    cut_selected_text, select_all_cursor, selected_text,
};

impl ScratchpadApp {
    pub(crate) fn active_buffer_transaction_label(&self) -> Option<String> {
        self.active_tab().map(|tab| {
            tab.active_buffer()
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| tab.active_buffer().name.clone())
        })
    }

    pub(crate) fn active_buffer_can_undo_text_operation(&self) -> bool {
        self.active_tab()
            .is_some_and(|tab| tab.active_buffer().document().operation_undo_depth() > 0)
    }

    pub(crate) fn active_buffer_can_redo_text_operation(&self) -> bool {
        self.active_tab()
            .is_some_and(|tab| tab.active_buffer().document().operation_redo_depth() > 0)
    }

    pub fn text_history_len(&self) -> usize {
        self.text_history.len()
    }

    pub fn text_history_len_for_buffer(&self, buffer_id: BufferId) -> usize {
        self.text_history.len_for_buffer(buffer_id)
    }

    pub fn text_history_editor_len(&self) -> usize {
        self.text_history.len_for_source(TextHistorySource::Editor)
    }

    pub fn text_history_search_replace_len(&self) -> usize {
        self.text_history
            .len_for_source(TextHistorySource::SearchReplace)
    }

    pub fn text_history_redo_len(&self) -> usize {
        self.text_history.redo_len()
    }

    pub fn latest_text_history_entry_id_for_buffer(&self, buffer_id: BufferId) -> Option<u64> {
        self.text_history.latest_entry_id_for_buffer(buffer_id)
    }

    pub fn latest_text_history_summary(&self) -> Option<&str> {
        self.text_history.latest_summary()
    }

    pub fn latest_text_history_edit_count(&self) -> Option<usize> {
        self.text_history.latest_edit_count()
    }

    pub fn latest_text_history_inserted_text(&self) -> Option<&str> {
        self.text_history.latest_inserted_text()
    }

    pub(crate) fn finalize_active_buffer_text_mutation(&mut self, active_tab_index: usize) {
        let tab = &mut self.tabs_mut()[active_tab_index];
        let buffer_id = tab.buffer.id;
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
        self.record_pending_text_history_event(active_tab_index, buffer_id);
        self.mark_search_dirty();
        self.mark_session_dirty();
        self.note_settings_toml_edit(active_tab_index);
    }

    pub(crate) fn prune_text_history_for_buffers(
        &mut self,
        buffer_ids: impl IntoIterator<Item = BufferId>,
    ) {
        self.text_history.prune_buffers(buffer_ids);
    }

    pub(crate) fn record_pending_text_history_event(
        &mut self,
        tab_index: usize,
        buffer_id: BufferId,
    ) {
        let pending = {
            let Some(buffer) = self
                .tabs_mut()
                .get_mut(tab_index)
                .and_then(|tab| tab.buffer_by_id_mut(buffer_id))
            else {
                return;
            };
            buffer
                .take_text_history_event()
                .and_then(|event| match event {
                    TextHistoryEvent::Edit { source, operation } => Some((
                        buffer.id,
                        buffer
                            .path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| format!("untitled:{}", buffer.temp_id)),
                        buffer
                            .path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| buffer.name.clone()),
                        source,
                        operation,
                    )),
                    TextHistoryEvent::Replay => None,
                })
        };

        if let Some((buffer_id, file_identity, label, source, operation)) = pending {
            self.text_history
                .record_components(buffer_id, file_identity, label, source, operation);
        }
    }

    pub fn undo_text_history_entry(&mut self, entry_id: u64) -> bool {
        self.apply_text_history_entry(entry_id, true)
    }

    pub fn redo_text_history_entry(&mut self, entry_id: u64) -> bool {
        self.apply_text_history_entry(entry_id, false)
    }

    fn apply_text_history_entry(&mut self, entry_id: u64, undo: bool) -> bool {
        let action = if undo {
            self.text_history.prepare_selective_undo(entry_id)
        } else {
            self.text_history.prepare_selective_redo(entry_id)
        };
        let Ok(action) = action else {
            self.set_error_status("Text history entry is no longer available.".to_owned());
            return false;
        };
        let Some(tab_index) = self.tab_index_for_buffer(action.buffer_id) else {
            self.text_history.prune_buffer(action.buffer_id);
            self.set_error_status("Text history entry belongs to a closed file.".to_owned());
            return false;
        };

        let selection = {
            let tab = &mut self.tabs_mut()[tab_index];
            let Some(buffer) = tab.buffer_by_id_mut(action.buffer_id) else {
                return false;
            };
            let result = if undo {
                buffer.apply_text_history_undo(&action.operation)
            } else {
                buffer.apply_text_history_redo(&action.operation)
            };
            match result {
                Ok(selection) => {
                    buffer.is_dirty = true;
                    selection
                }
                Err(_) => {
                    self.set_error_status(
                        "Text history entry conflicts with the current file contents.".to_owned(),
                    );
                    return false;
                }
            }
        };

        if undo {
            self.text_history.mark_undone(action.entry_id);
        } else {
            self.text_history.mark_redone(action.entry_id);
        }
        self.restore_text_history_selection(tab_index, action.buffer_id, selection);
        self.mark_search_dirty();
        self.mark_session_dirty();
        true
    }

    fn tab_index_for_buffer(&self, buffer_id: BufferId) -> Option<usize> {
        self.tabs()
            .iter()
            .position(|tab| tab.buffers().any(|buffer| buffer.id == buffer_id))
    }

    fn restore_text_history_selection(
        &mut self,
        tab_index: usize,
        buffer_id: BufferId,
        selection: crate::app::ui::editor_content::native_editor::CursorRange,
    ) {
        let Some(view_id) = self.tabs().get(tab_index).and_then(|tab| {
            tab.views
                .iter()
                .find(|view| view.buffer_id == buffer_id)
                .map(|view| view.id)
        }) else {
            return;
        };
        let tab = &mut self.tabs_mut()[tab_index];
        let _ = tab.activate_view(view_id);
        if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
            view.set_cursor_range_anchored(buffer, selection);
            view.set_pending_cursor_range_anchored(buffer, selection);
            view.request_cursor_reveal(CursorRevealMode::Center);
        }
    }

    pub(crate) fn undo_active_buffer_text_operation(&mut self) -> bool {
        self.apply_active_buffer_text_operation(true)
    }

    pub(crate) fn redo_active_buffer_text_operation(&mut self) -> bool {
        self.apply_active_buffer_text_operation(false)
    }

    pub(crate) fn select_all_in_active_view(&mut self) -> bool {
        let active_tab_index = self.active_tab_index();
        let (total_chars, active_view_id) = match self.active_tab() {
            Some(tab) => (
                tab.active_buffer().current_file_length().chars,
                tab.active_view_id,
            ),
            None => return false,
        };
        let selection = select_all_cursor(total_chars);

        let tab = &mut self.tabs_mut()[active_tab_index];
        let Some((buffer, view)) = tab.buffer_and_view_mut(active_view_id) else {
            return false;
        };
        view.set_cursor_range_anchored(buffer, selection);
        view.set_pending_cursor_range_anchored(buffer, selection);
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
        let active_view_id = {
            let tab = self.active_tab()?;
            tab.active_view_id
        };

        let (next_selection, selected_text) = {
            let tab = &mut self.tabs_mut()[active_tab_index];
            let (buffer, view) = tab.buffer_and_view_mut(active_view_id)?;
            let current_selection = view.cursor_range?;
            let (next_selection, selected_text) = cut_selected_text(buffer, current_selection)?;
            view.set_cursor_range_anchored(buffer, next_selection);
            view.set_pending_cursor_range_anchored(buffer, next_selection);
            view.request_cursor_reveal(CursorRevealMode::KeepVisible);
            buffer.active_selection = None;
            (next_selection, selected_text)
        };

        self.finalize_active_buffer_text_mutation(active_tab_index);
        self.refresh_search_state();
        self.select_next_active_buffer_match_from(next_selection.primary.index);
        Some(selected_text)
    }

    fn apply_active_buffer_text_operation(&mut self, undo: bool) -> bool {
        let active_tab_index = self.active_tab_index();
        let active_buffer_label = match self.active_buffer_transaction_label() {
            Some(label) => label,
            None => return false,
        };

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

            let active_view_id = tab.active_view_id;
            if let Some((buffer, view)) = tab.buffer_and_view_mut(active_view_id) {
                view.set_cursor_range_anchored(buffer, selection);
                view.set_pending_cursor_range_anchored(buffer, selection);
                view.request_cursor_reveal(CursorRevealMode::Center);
            }
            selection
        };

        self.finalize_active_buffer_text_mutation(active_tab_index);
        self.refresh_search_state();
        self.select_next_active_buffer_match_from(selection.primary.index);
        let action = if undo { "Undid" } else { "Redid" };
        self.set_info_status(format!(
            "{action} last text operation in {active_buffer_label}."
        ));
        true
    }
}
