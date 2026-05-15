use super::super::{ScratchpadApp, StatusDomain};
use crate::app::app_state::settings_controller;
use crate::app::app_state::workspace::accessors as workspace_accessors;
use crate::app::domain::{BufferId, CursorRevealMode};
use crate::app::text_history::{TextHistoryEntryView, entries_for_buffer};

pub(crate) fn text_history_entries(app: &ScratchpadApp) -> Vec<TextHistoryEntryView> {
    let mut entries = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers().flat_map(entries_for_buffer))
        .collect::<Vec<_>>();
    sort_text_history_entries(&mut entries);
    entries
}

pub(crate) fn cached_text_history_entries(app: &mut ScratchpadApp) -> Vec<TextHistoryEntryView> {
    let revisions = text_history_revisions(app);
    if app.state.text_history_cache.revisions != revisions {
        app.state.text_history_cache.entries = text_history_entries(app);
        app.state.text_history_cache.revisions = revisions;
    }
    app.state.text_history_cache.entries.clone()
}

fn text_history_revisions(app: &ScratchpadApp) -> Vec<(BufferId, u64)> {
    app.tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| {
            tab.buffers()
                .map(|buffer| (buffer.id, buffer.document().history_revision_counter()))
        })
        .collect()
}

pub(crate) fn clear_text_history(app: &mut ScratchpadApp) -> bool {
    let mut cleared = false;
    for tab in app.tab_manager.tabs.as_mut_slice() {
        for buffer in tab.buffers_mut() {
            if !buffer.document().history_entries().is_empty() {
                buffer.document_mut().clear_operation_history();
                cleared = true;
            }
        }
    }
    if cleared {
        app.state.text_history_cache = Default::default();
        app.tab_manager.mark_session_dirty();
        app.state
            .status
            .set_info_status_in_domain(StatusDomain::History, "Cleared text history.");
    }
    cleared
}

pub(crate) fn finalize_active_buffer_text_mutation(
    app: &mut ScratchpadApp,
    active_tab_index: usize,
) {
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[active_tab_index];
    let buffer_id = tab.buffers.buffer.id;
    let latest_edit = tab
        .buffers
        .buffer
        .document()
        .latest_operation_record()
        .cloned();
    tab.buffers
        .buffer
        .refresh_text_metadata_after_operation(latest_edit.as_ref());
    tab.buffers.buffer.mark_dirty_after_local_edit();
    let warning_message = tab
        .buffers
        .buffer
        .artifact_summary
        .status_text()
        .map(|message| format!("{message}; raw-text editing remains enabled"));
    let _ = tab;

    if let Some(message) = warning_message {
        app.state
            .status
            .set_warning_status_in_domain(StatusDomain::Encoding, message);
    } else {
        crate::app::app_state::workspace::accessors::clear_status_message(app);
    }
    record_pending_text_history_event(app, active_tab_index, buffer_id);
    app.enforce_aggregate_text_history_budget();
    crate::app::app_state::search_runtime::mark_search_dirty(app);
    app.tab_manager.mark_session_dirty();
    app.note_settings_toml_edit(active_tab_index);
    app.apply_current_tab_ordering();
}

pub(crate) fn prune_text_history_for_buffers(
    app: &mut ScratchpadApp,
    buffer_ids: impl IntoIterator<Item = BufferId>,
) {
    if buffer_ids.into_iter().next().is_some() {
        app.state.text_history_cache = Default::default();
    }
}

pub(crate) fn record_pending_text_history_event(
    app: &mut ScratchpadApp,
    tab_index: usize,
    buffer_id: BufferId,
) {
    if let Some(buffer) = app
        .tab_manager
        .tabs
        .as_mut_slice()
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
    app: &mut ScratchpadApp,
    buffer_id: BufferId,
    entry_id: u64,
    follow_focus: bool,
) -> bool {
    let target = match text_history_entries(app)
        .into_iter()
        .find(|entry| entry.buffer_id == buffer_id && entry.id == entry_id)
    {
        Some(target) => target,
        None => {
            app.state.status.set_error_status_in_domain(
                StatusDomain::History,
                "Text history entry is no longer available.",
            );
            return false;
        }
    };
    let undo = !target.undone;
    apply_text_history_entry_with_focus(app, buffer_id, entry_id, undo, follow_focus)
}

fn apply_text_history_entry_with_focus(
    app: &mut ScratchpadApp,
    buffer_id: BufferId,
    entry_id: u64,
    undo: bool,
    follow_focus: bool,
) -> bool {
    let Some(action) = text_history_entries(app)
        .into_iter()
        .find(|entry| entry.buffer_id == buffer_id && entry.id == entry_id)
    else {
        app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            "Text history entry is no longer available.",
        );
        return false;
    };
    if undo && action.undone || !undo && !action.undone || !action.replayable {
        app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            "Text history entry is not replayable in that direction.",
        );
        return false;
    }
    let Some(tab_index) = tab_index_for_buffer(app, action.buffer_id) else {
        app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            "Text history entry belongs to a closed file.",
        );
        return false;
    };

    let selection = {
        let tab = &mut app.tab_manager.tabs.as_mut_slice()[tab_index];
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
                buffer.mark_dirty_after_local_edit();
                selection
            }
            Err(_) => {
                app.state.status.set_error_status_in_domain(
                    StatusDomain::History,
                    "Text history entry conflicts with the current file contents.",
                );
                return false;
            }
        }
    };

    if follow_focus {
        restore_text_history_selection(app, tab_index, action.buffer_id, selection);
    }
    crate::app::app_state::search_runtime::mark_search_dirty(app);
    app.tab_manager.mark_session_dirty();
    app.apply_current_tab_ordering();
    true
}

pub(crate) fn tab_index_for_buffer(app: &mut ScratchpadApp, buffer_id: BufferId) -> Option<usize> {
    if let Some(index) = app.tab_manager.tab_index_for_buffer(buffer_id)
        && app
            .tab_manager
            .tabs
            .as_slice()
            .get(index)
            .is_some_and(|tab| tab.buffer_by_id(buffer_id).is_some())
    {
        return Some(index);
    }

    app.tab_manager.rebuild_buffer_tab_index();
    app.tab_manager.tab_index_for_buffer(buffer_id)
}

fn restore_text_history_selection(
    app: &mut ScratchpadApp,
    tab_index: usize,
    buffer_id: BufferId,
    selection: crate::app::ui::editor_content::native_editor::CursorRange,
) {
    let Some(view_id) = app
        .tab_manager
        .tabs
        .as_slice()
        .get(tab_index)
        .and_then(|tab| {
            tab.layout
                .views
                .iter()
                .find(|view| view.buffer_id == buffer_id)
                .map(|view| view.id)
        })
    else {
        return;
    };
    settings_controller::activate_workspace_surface(app);
    app.tab_manager.set_active_tab_index_clamped(tab_index);
    crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
    app.tab_manager.pending_scroll_to_active = true;
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[tab_index];
    let _ = tab.activate_view(view_id);
    if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
        view.set_cursor_range_anchored(buffer, selection);
        view.set_pending_cursor_range_anchored(buffer, selection);
        view.request_cursor_reveal(CursorRevealMode::Center);
    }
    crate::app::app_state::search_runtime::refresh_search_view_state(app);
    workspace_accessors::request_focus_for_view(app, view_id);
}

fn sort_text_history_entries(entries: &mut [TextHistoryEntryView]) {
    entries.sort_by_key(|entry| (entry.global_seq, entry.buffer_id, entry.id));
}

#[cfg(test)]
mod tests {
    use super::ScratchpadApp;
    use super::apply_text_history_to_entry;
    use super::sort_text_history_entries;
    use crate::app::domain::{BufferId, BufferState, PieceSource, SplitAxis, WorkspaceTab};
    use crate::app::services::session_store::SessionStore;
    use crate::app::services::settings_store::SettingsStore;
    use crate::app::text_history::TextHistoryEntryView;
    use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};
    use crate::app::{domain::TabManager, startup::StartupOptions};

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

    #[test]
    fn follow_history_undo_activates_containing_tab_and_moves_one_view() {
        let mut app = test_app_with_tabs(["first.txt", "second.txt"]);
        let target_tab_index = 1;
        let target_buffer_id = app.tab_manager.tabs.as_slice()[target_tab_index]
            .buffers
            .buffer
            .id;
        let previous_selection = CursorRange::one(CharCursor::new(0));
        let next_selection = CursorRange::one(CharCursor::new(5));
        app.tab_manager.tabs.as_mut_slice()[target_tab_index]
            .buffers
            .buffer
            .replace_char_ranges_with_undo(
                &[(0..0, "hello".to_owned())],
                previous_selection,
                next_selection,
            )
            .expect("record text history");
        let entry_id = app.tab_manager.tabs.as_slice()[target_tab_index]
            .buffers
            .buffer
            .document()
            .history_entries()
            .last()
            .expect("history entry")
            .id;

        app.tab_manager.active_tab_index = target_tab_index;
        let original_view_id = app.tab_manager.tabs.as_slice()[target_tab_index]
            .layout
            .active_view_id;
        let split_view_id = app.tab_manager.tabs.as_mut_slice()[target_tab_index]
            .split_active_view(SplitAxis::Vertical)
            .expect("split target view");
        {
            let tab = &mut app.tab_manager.tabs.as_mut_slice()[target_tab_index];
            let original_view = tab.view_mut(original_view_id).expect("original view");
            original_view.cursor_range = Some(CursorRange::one(CharCursor::new(1)));
            original_view.pending_cursor_range = None;
            let split_view = tab.view_mut(split_view_id).expect("split view");
            split_view.cursor_range = Some(CursorRange::one(CharCursor::new(2)));
            split_view.pending_cursor_range = None;
        }
        app.tab_manager.active_tab_index = 0;

        assert!(apply_text_history_to_entry(
            &mut app,
            target_buffer_id,
            entry_id,
            true
        ));

        let tab = &app.tab_manager.tabs.as_slice()[app.tab_manager.active_tab_index];
        let original_view = tab.view(original_view_id).expect("original view");
        let split_view = tab.view(split_view_id).expect("split view");
        assert_eq!(app.tab_manager.active_tab_index, target_tab_index);
        assert_eq!(tab.layout.active_view_id, original_view_id);
        assert_eq!(original_view.cursor_range, Some(previous_selection));
        assert_eq!(original_view.pending_cursor_range, Some(previous_selection));
        assert_eq!(
            original_view.cursor_reveal_mode(),
            Some(crate::app::domain::CursorRevealMode::Center)
        );
        assert_eq!(
            split_view.cursor_range,
            Some(CursorRange::one(CharCursor::new(2)))
        );
        assert_eq!(split_view.pending_cursor_range, None);
        assert_eq!(split_view.cursor_reveal_mode(), None);
    }

    fn test_app_with_tabs<const N: usize>(names: [&str; N]) -> ScratchpadApp {
        let temp_dir = tempfile::tempdir().expect("create temp app root");
        let root = temp_dir.keep();
        let mut app = ScratchpadApp::with_stores_and_startup(
            SessionStore::new(root.clone()),
            SettingsStore::new(root),
            StartupOptions::default(),
        );
        app.set_session_persist_on_drop(false);
        app.tab_manager = TabManager {
            tabs: names.into_iter().map(test_tab).collect(),
            active_tab_index: 0,
            pending_action: None,
            session_dirty: false,
            pending_scroll_to_active: false,
            buffer_tab_index: Default::default(),
            cold_session_tabs: Default::default(),
        };
        app.tab_manager.rebuild_buffer_tab_index();
        crate::app::app_state::workspace::display_tabs::clear_tab_selection(app);
        app
    }

    fn test_tab(name: &str) -> WorkspaceTab {
        WorkspaceTab::new(BufferState::new(name.to_owned(), String::new(), None))
    }
}
