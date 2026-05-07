use super::super::ScratchpadApp;
use crate::app::domain::{BufferId, CursorRevealMode};
use crate::app::text_history::{TextHistoryEntryView, entries_for_buffer};

impl ScratchpadApp {
    pub fn text_history_redo_len(&self) -> usize {
        self.text_history_entries()
            .iter()
            .filter(|entry| entry.undone)
            .count()
    }

    pub(crate) fn text_history_entries(&self) -> Vec<TextHistoryEntryView> {
        let mut entries = self
            .tabs()
            .iter()
            .flat_map(|tab| tab.buffers().flat_map(entries_for_buffer))
            .collect::<Vec<_>>();
        sort_text_history_entries(&mut entries);
        entries
    }

    pub(crate) fn cached_text_history_entries(&mut self) -> Vec<TextHistoryEntryView> {
        let revisions = self.text_history_revisions();
        if self.text_history_cache.revisions != revisions {
            self.text_history_cache.entries = self.text_history_entries();
            self.text_history_cache.revisions = revisions;
        }
        self.text_history_cache.entries.clone()
    }

    fn text_history_revisions(&self) -> Vec<(BufferId, u64)> {
        self.tabs()
            .iter()
            .flat_map(|tab| {
                tab.buffers()
                    .map(|buffer| (buffer.id, buffer.document().history_revision_counter()))
            })
            .collect()
    }

    pub(crate) fn clear_text_history(&mut self) -> bool {
        let mut cleared = false;
        for tab in self.tabs_mut() {
            for buffer in tab.buffers_mut() {
                if !buffer.document().history_entries().is_empty() {
                    buffer.document_mut().clear_operation_history();
                    cleared = true;
                }
            }
        }
        if cleared {
            self.text_history_cache = Default::default();
            self.mark_session_dirty();
            self.set_info_status("Cleared text history.");
        }
        cleared
    }

    pub(crate) fn finalize_active_buffer_text_mutation(&mut self, active_tab_index: usize) {
        let tab = &mut self.tabs_mut()[active_tab_index];
        let buffer_id = tab.buffer.id;
        let latest_edit = tab.buffer.document().latest_operation_record().cloned();
        tab.buffer
            .refresh_text_metadata_after_operation(latest_edit.as_ref());
        let has_control_chars = tab.buffer.artifact_summary.has_control_chars();
        if !has_control_chars {
            tab.buffer.show_control_chars = false;
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
        self.enforce_aggregate_text_history_budget();
        self.mark_search_dirty();
        self.mark_session_dirty();
        self.note_settings_toml_edit(active_tab_index);
        self.apply_current_tab_ordering();
    }

    pub(crate) fn prune_text_history_for_buffers(
        &mut self,
        buffer_ids: impl IntoIterator<Item = BufferId>,
    ) {
        if buffer_ids.into_iter().next().is_some() {
            self.text_history_cache = Default::default();
        }
    }

    pub(crate) fn record_pending_text_history_event(
        &mut self,
        tab_index: usize,
        buffer_id: BufferId,
    ) {
        if let Some(buffer) = self
            .tabs_mut()
            .get_mut(tab_index)
            .and_then(|tab| tab.buffer_by_id_mut(buffer_id))
        {
            let _ = buffer.take_text_history_event();
        }
    }

    /// Undo or redo every entry between the current "Now" boundary and the
    /// clicked entry, inclusive — but only within the clicked entry's own
    /// buffer. The direction is inferred from the clicked entry's current
    /// state (an applied entry is undone, an undone entry is redone). The
    /// per-buffer document already replays a contiguous batch internally when
    /// given any single entry id, so a single call is enough. Other buffers'
    /// histories are left alone even when their seqs fall between Now and the
    /// click target.
    pub fn apply_text_history_to_entry(
        &mut self,
        buffer_id: BufferId,
        entry_id: u64,
        follow_focus: bool,
    ) -> bool {
        let target = match self
            .text_history_entries()
            .into_iter()
            .find(|entry| entry.buffer_id == buffer_id && entry.id == entry_id)
        {
            Some(target) => target,
            None => {
                self.set_error_status("Text history entry is no longer available.".to_owned());
                return false;
            }
        };
        let undo = !target.undone;
        self.apply_text_history_entry_with_focus(buffer_id, entry_id, undo, follow_focus)
    }

    fn apply_text_history_entry_with_focus(
        &mut self,
        buffer_id: BufferId,
        entry_id: u64,
        undo: bool,
        follow_focus: bool,
    ) -> bool {
        let Some(action) = self
            .text_history_entries()
            .into_iter()
            .find(|entry| entry.buffer_id == buffer_id && entry.id == entry_id)
        else {
            self.set_error_status("Text history entry is no longer available.".to_owned());
            return false;
        };
        if undo && action.undone || !undo && !action.undone || !action.replayable {
            self.set_error_status("Text history entry is not replayable in that direction.");
            return false;
        }
        let Some(tab_index) = self.tab_index_for_buffer(action.buffer_id) else {
            self.set_error_status("Text history entry belongs to a closed file.".to_owned());
            return false;
        };

        let selection = {
            let tab = &mut self.tabs_mut()[tab_index];
            let Some(buffer) = tab.buffer_by_id_mut(action.buffer_id) else {
                return false;
            };
            let result = if undo {
                buffer.apply_text_history_undo(action.id)
            } else {
                buffer.apply_text_history_redo(action.id)
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

        if follow_focus {
            self.restore_text_history_selection(tab_index, action.buffer_id, selection);
        }
        self.mark_search_dirty();
        self.mark_session_dirty();
        self.apply_current_tab_ordering();
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
}

fn sort_text_history_entries(entries: &mut [TextHistoryEntryView]) {
    entries.sort_by_key(|entry| (entry.global_seq, entry.buffer_id, entry.id));
}

#[cfg(test)]
mod tests {
    use super::sort_text_history_entries;
    use crate::app::domain::{BufferId, PieceSource};
    use crate::app::text_history::TextHistoryEntryView;

    #[test]
    fn text_history_sort_uses_global_sequence_before_entry_id() {
        let mut entries = vec![entry(30, 10, 1), entry(1, 20, 2)];

        sort_text_history_entries(&mut entries);

        assert_eq!(
            entries
                .iter()
                .rev()
                .map(|entry| (entry.buffer_id, entry.id))
                .collect::<Vec<_>>(),
            vec![(2, 1), (1, 30)]
        );
    }

    fn entry(id: u64, global_seq: u64, buffer_id: BufferId) -> TextHistoryEntryView {
        TextHistoryEntryView {
            id,
            global_seq,
            buffer_id,
            label: format!("file-{buffer_id}"),
            source: PieceSource::Edit,
            summary: String::new(),
            undone: false,
            replayable: true,
            edit_count: 1,
            first_deleted_text: String::new(),
            first_inserted_text: String::new(),
        }
    }
}
