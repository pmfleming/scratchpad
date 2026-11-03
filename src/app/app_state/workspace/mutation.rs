use super::super::{ScratchpadApp, StatusDomain};
use crate::app::app_state::settings_controller;
use crate::app::app_state::workspace::accessors as workspace_accessors;
use crate::app::domain::{BufferId, CursorRevealMode};
pub(crate) fn clear_text_history(app: &mut ScratchpadApp, buffer_ids: &[BufferId]) -> bool {
    let mut cleared = false;
    for tab in app.tab_manager.tabs.as_mut_slice() {
        for buffer in tab.buffers_mut() {
            if buffer_ids.contains(&buffer.id) && !buffer.document().history_entries().is_empty() {
                buffer.document_mut().clear_operation_history();
                cleared = true;
            }
        }
    }
    if cleared {
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
    let text_metadata_refresh = tab.buffers.buffer.text_metadata_refresh_needed().then(|| {
        let buffer = &tab.buffers.buffer;
        (
            buffer.id,
            buffer.document_revision(),
            buffer.document_snapshot(),
            buffer.format.clone(),
        )
    });
    let warning_message = tab
        .buffers
        .buffer
        .artifact_summary
        .status_text()
        .map(|message| format!("{message}; raw-text editing remains enabled"));
    let _ = tab;

    if let Some((buffer_id, revision, snapshot, format)) = text_metadata_refresh {
        app.queue_background_text_metadata_refresh(buffer_id, revision, snapshot, format);
    }

    if let Some(message) = warning_message {
        app.state
            .status
            .set_warning_status_in_domain(StatusDomain::Encoding, message);
    } else {
        crate::app::app_state::workspace::accessors::clear_status_message(app);
    }
    app.enforce_aggregate_text_history_budget();
    crate::app::app_state::search_runtime::mark_search_dirty(app);
    app.tab_manager.mark_session_dirty();
    app.note_settings_toml_edit(active_tab_index);
    app.apply_current_tab_ordering();
}

pub(crate) fn navigate_text_history(
    app: &mut ScratchpadApp,
    buffer_id: BufferId,
    entry_id: u64,
    follow_focus: bool,
) -> bool {
    let Some(tab_index) = tab_index_for_buffer(app, buffer_id) else {
        app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            "Text history entry belongs to a closed file.",
        );
        return false;
    };
    let (selection, text_metadata_refresh) = {
        let tab = &mut app.tab_manager.tabs.as_mut_slice()[tab_index];
        let Some(buffer) = tab.buffer_by_id_mut(buffer_id) else {
            return false;
        };
        let result = buffer.navigate_history(entry_id);
        if let Ok(selection) = result {
            // Tool navigation updates an already visible scrubber. Automatic
            // opening belongs to editor undo/redo, not rail hover.
            buffer.note_text_history_event(crate::app::domain::buffer::TextHistoryEvent::Navigate);
            buffer.mark_dirty_after_local_edit();
            let text_metadata_refresh = buffer.text_metadata_refresh_needed().then(|| {
                (
                    buffer.id,
                    buffer.document_revision(),
                    buffer.document_snapshot(),
                    buffer.format.clone(),
                )
            });
            (selection, text_metadata_refresh)
        } else {
            app.state.status.set_error_status_in_domain(
                StatusDomain::History,
                "Text history entry conflicts with the current file contents.",
            );
            return false;
        }
    };

    if let Some((buffer_id, revision, snapshot, format)) = text_metadata_refresh {
        app.queue_background_text_metadata_refresh(buffer_id, revision, snapshot, format);
    }

    if follow_focus {
        restore_text_history_selection(app, tab_index, buffer_id, selection);
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
                .active_view()
                .filter(|view| view.buffer_id == buffer_id)
                .or_else(|| {
                    tab.layout
                        .views
                        .iter()
                        .find(|view| view.buffer_id == buffer_id)
                })
                .map(|view| view.id)
        })
    else {
        return;
    };
    let switching = app.tab_manager.active_tab_index != tab_index
        || app.tab_manager.tabs.as_slice()[tab_index]
            .layout
            .active_view_id
            != view_id
        || crate::app::app_state::settings_state::showing_settings(app);
    if switching {
        settings_controller::activate_workspace_surface(app);
        app.tab_manager.set_active_tab_index_clamped(tab_index);
        crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
        app.tab_manager.pending_scroll_to_active = true;
    }
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[tab_index];
    if switching {
        let _ = tab.activate_view(view_id);
    }
    if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
        view.set_cursor_range_anchored(buffer, selection);
        view.set_pending_cursor_range_anchored(buffer, selection);
        view.request_cursor_reveal(CursorRevealMode::Center);
    }
    // Both replay callers mark search dirty immediately after restoration.
    // Repainting old search results here would be redundant (and stale).
    workspace_accessors::request_focus_for_view(app, view_id);
}

#[cfg(test)]
mod tests {
    use super::ScratchpadApp;
    use super::navigate_text_history;
    use crate::app::domain::{BufferState, SplitAxis, WorkspaceTab};
    use crate::app::services::session_store::SessionStore;
    use crate::app::services::settings_store::SettingsStore;
    use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};
    use crate::app::{domain::TabManager, startup::StartupOptions};

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

        app.tab_manager.active_tab_index = target_tab_index;
        let original_view_id = app.tab_manager.tabs.as_slice()[target_tab_index]
            .layout
            .active_view_id;
        let split_view_id = app.tab_manager.tabs.as_mut_slice()[target_tab_index]
            .split_active_view(SplitAxis::Vertical)
            .expect("split target view");
        {
            let tab = &mut app.tab_manager.tabs.as_mut_slice()[target_tab_index];
            let original_view = tab
                .layout
                .view_mut(original_view_id)
                .expect("original view");
            original_view.cursor_range = Some(CursorRange::one(CharCursor::new(1)));
            original_view.pending_cursor_range = None;
            let split_view = tab.layout.view_mut(split_view_id).expect("split view");
            split_view.cursor_range = Some(CursorRange::one(CharCursor::new(2)));
            split_view.pending_cursor_range = None;
        }
        app.tab_manager.active_tab_index = 0;

        assert!(navigate_text_history(&mut app, target_buffer_id, 0, true));

        let tab = &app.tab_manager.tabs.as_slice()[app.tab_manager.active_tab_index];
        let original_view = tab.layout.view(original_view_id).expect("original view");
        let split_view = tab.layout.view(split_view_id).expect("split view");
        assert_eq!(app.tab_manager.active_tab_index, target_tab_index);
        assert_eq!(tab.layout.active_view_id, split_view_id);
        assert_eq!(split_view.cursor_range, Some(previous_selection));
        assert_eq!(split_view.pending_cursor_range, Some(previous_selection));
        assert_eq!(
            split_view.cursor_reveal_mode(),
            Some(crate::app::domain::CursorRevealMode::Center)
        );
        assert_eq!(
            original_view.cursor_range,
            Some(CursorRange::one(CharCursor::new(1)))
        );
        assert_eq!(original_view.pending_cursor_range, None);
        assert_eq!(original_view.cursor_reveal_mode(), None);
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
        app.tab_manager = TabManager::for_test_tabs(names.into_iter().map(test_tab).collect());
        crate::app::app_state::workspace::display_tabs::clear_tab_selection(&mut app);
        app
    }

    fn test_tab(name: &str) -> WorkspaceTab {
        WorkspaceTab::new(BufferState::new(name.to_owned(), String::new(), None))
    }
}
