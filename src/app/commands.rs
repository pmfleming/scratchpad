use crate::app::app_state::search_controller::{
    activate_search_match_at, focus_search_result_file_at,
};
use crate::app::app_state::{ScratchpadApp, SearchScope};
use crate::app::domain::{BufferId, PendingAction, SplitAxis, SplitPath, ViewId};
use crate::app::services::file_controller::FileController;
use crate::app::services::search::SearchMode;

mod dispatch;
mod tab_transfer;

pub enum AppCommand {
    ActivateTab {
        index: usize,
    },
    ActivateView {
        view_id: ViewId,
    },
    CloseTab {
        index: usize,
    },
    CloseView {
        view_id: ViewId,
    },
    CloseSettings,
    CombineTabIntoTab {
        source_index: usize,
        target_index: usize,
    },
    CombineTabsIntoTab {
        source_indices: Vec<usize>,
        target_index: usize,
    },
    PromoteViewToTab {
        view_id: ViewId,
    },
    PromoteTabFilesToTabs {
        index: usize,
    },
    NewTab,
    OpenFile,
    OpenFileHere,
    OpenSearch,
    OpenSearchAndReplace,
    OpenSettings,
    OpenTextHistory,
    OpenUserManual,
    CloseSearch,
    ToggleSearch,
    SetSearchQuery {
        query: String,
    },
    SetSearchReplacement {
        replacement: String,
    },
    SetSearchReplaceOpen {
        open: bool,
    },
    SetSearchScope {
        scope: SearchScope,
    },
    SetSearchMode {
        mode: SearchMode,
    },
    SetSearchMatchCase {
        enabled: bool,
    },
    SetSearchWholeWord {
        enabled: bool,
    },
    FocusSearchResultFile {
        match_index: usize,
    },
    ActivateSearchMatch {
        match_index: usize,
    },
    UndoActiveBufferTextOperation,
    RedoActiveBufferTextOperation,
    NextSearchMatch,
    PreviousSearchMatch,
    ReplaceCurrentMatch,
    ReplaceAllMatches,
    ReorderTab {
        from_index: usize,
        to_index: usize,
    },
    ReorderDisplayTab {
        from_index: usize,
        to_index: usize,
    },
    RequestCloseTab {
        index: usize,
    },
    ResizeSplit {
        path: SplitPath,
        ratio: f32,
    },
    ReopenBufferWithEncoding {
        tab_index: usize,
        encoding_name: String,
    },
    SaveFile,
    SaveAllFiles,
    SaveFileAs,
    SaveFileWithEncoding {
        tab_index: usize,
        encoding_name: String,
    },
    SaveConflictOverwrite {
        tab_index: usize,
        view_id: ViewId,
    },
    ReloadBufferFromDisk {
        tab_index: usize,
        view_id: ViewId,
    },
    SaveConflictAsCopy {
        tab_index: usize,
        view_id: ViewId,
    },
    SplitActiveView {
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    },
}

pub(crate) fn open_text_history(app: &mut ScratchpadApp) {
    app.state.text_history_open = true;
}

pub(crate) fn close_text_history(app: &mut ScratchpadApp) {
    app.state.text_history_open = false;
}

fn activate_tab(app: &mut ScratchpadApp, index: usize) -> bool {
    if index >= app.tab_manager.tabs.as_slice().len() {
        return false;
    }

    app.reload_settings_before_workspace_change();
    app.activate_workspace_surface();
    app.tab_manager.set_active_tab_index_clamped(index);
    app.hydrate_tab_if_needed(index);
    app.ensure_active_tab_slot_selected();
    app.tab_manager.pending_scroll_to_active = true;
    app.refresh_search_view_state();
    app.request_focus_for_active_view();
    FileController::refresh_active_buffer_disk_state(app);
    app.tab_manager.mark_session_dirty();
    true
}

fn activate_view_command(app: &mut ScratchpadApp, view_id: ViewId) -> bool {
    app.reload_settings_if_switching_views(view_id);

    let index = app.tab_manager.active_tab_index;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab.activate_view(view_id)
    {
        app.refresh_search_view_state();
        app.request_focus_for_view(view_id);
        FileController::refresh_active_buffer_disk_state(app);
        app.tab_manager.mark_session_dirty();
        return true;
    }
    false
}

fn close_view_command(app: &mut ScratchpadApp, view_id: ViewId) -> bool {
    let index = app.tab_manager.active_tab_index;
    let Some(tab) = app.tab_manager.tabs.as_slice().get(index) else {
        return false;
    };
    if tab.root_pane.leaf_count() <= 1 || !tab.root_pane.contains_view(view_id) {
        return false;
    }

    if tab
        .buffer_for_view(view_id)
        .is_some_and(|buffer| buffer.is_dirty)
        && tab.is_last_view_for_buffer(view_id) == Some(true)
    {
        app.set_pending_action(Some(PendingAction::CloseView {
            tab_index: index,
            view_id,
        }));
        return true;
    }

    perform_close_view(app, view_id);
    true
}

pub(crate) fn perform_close_view(app: &mut ScratchpadApp, view_id: ViewId) {
    app.reload_settings_if_closing_view(view_id);

    let index = app.tab_manager.active_tab_index;
    let open_buffer_ids_before = app
        .tab_manager
        .tabs
        .as_slice()
        .get(index)
        .map(|tab| tab.buffers().map(|buffer| buffer.id).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut next_active_view = None;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab.close_view(view_id)
    {
        next_active_view = Some(tab.active_view_id);
    }
    if let Some(next_active_view) = next_active_view {
        let open_buffer_ids_after = app
            .tab_manager
            .tabs
            .as_slice()
            .get(index)
            .map(|tab| tab.buffers().map(|buffer| buffer.id).collect::<Vec<_>>())
            .unwrap_or_default();
        let closed_buffer_ids = removed_buffer_ids(open_buffer_ids_before, &open_buffer_ids_after);
        app.prune_text_history_for_buffers(closed_buffer_ids);
        app.tab_manager.rebuild_buffer_tab_index();
        app.begin_layout_transition();
        app.mark_search_dirty();
        app.request_focus_for_view(next_active_view);
        app.tab_manager.mark_session_dirty();
    }
}

fn request_close_tab(app: &mut ScratchpadApp, index: usize) -> bool {
    if index < app.tab_manager.tabs.as_slice().len() {
        app.set_pending_action(Some(PendingAction::CloseTab(index)));
        return true;
    }
    false
}

fn reorder_tab_command(app: &mut ScratchpadApp, from_index: usize, to_index: usize) -> bool {
    if !app.tab_manager.reorder_tab(from_index, to_index) {
        return false;
    }
    app.begin_layout_transition();
    app.mark_search_dirty();
    app.tab_manager.mark_session_dirty();
    true
}

fn reorder_display_tab_command(
    app: &mut ScratchpadApp,
    from_index: usize,
    to_index: usize,
) -> bool {
    if app.reorder_display_tab(from_index, to_index) {
        app.begin_layout_transition();
        app.mark_search_dirty();
        return true;
    }
    false
}

fn resize_split_command(app: &mut ScratchpadApp, path: SplitPath, ratio: f32) -> bool {
    let index = app.tab_manager.active_tab_index;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab.resize_split(path, ratio)
    {
        app.begin_layout_transition();
        app.tab_manager.mark_session_dirty();
        return true;
    }
    false
}

fn split_active_view_command(
    app: &mut ScratchpadApp,
    axis: SplitAxis,
    new_view_first: bool,
    ratio: f32,
) -> bool {
    let index = app.tab_manager.active_tab_index;
    let mut new_active_view = None;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab
            .split_active_view_with_placement(axis, new_view_first, ratio)
            .is_some()
    {
        new_active_view = Some(tab.active_view_id);
    }
    if let Some(new_active_view) = new_active_view {
        app.begin_layout_transition();
        app.mark_search_dirty();
        app.request_focus_for_view(new_active_view);
        app.tab_manager.mark_session_dirty();
        return true;
    }
    false
}

fn focus_search_result_file_command(app: &mut ScratchpadApp, match_index: usize) -> bool {
    if focus_search_result_file_at(app, match_index) {
        app.request_focus_for_active_view();
        return true;
    }
    false
}

fn activate_search_match_command(app: &mut ScratchpadApp, match_index: usize) -> bool {
    if activate_search_match_at(app, match_index) {
        app.request_focus_for_active_view();
        return true;
    }
    false
}

fn reopen_buffer_with_encoding_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    encoding_name: &str,
) -> bool {
    FileController::reopen_buffer_with_encoding(app, tab_index, encoding_name)
}

fn save_file_with_encoding_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    encoding_name: &str,
) -> bool {
    FileController::save_file_with_encoding_at(app, tab_index, encoding_name)
}

fn save_conflict_overwrite_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> bool {
    run_save_conflict_command(
        app,
        tab_index,
        view_id,
        FileController::save_conflict_overwrite,
    )
}

fn reload_buffer_from_disk_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> bool {
    run_save_conflict_command(
        app,
        tab_index,
        view_id,
        FileController::reload_buffer_from_disk,
    )
}

fn save_conflict_as_copy_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> bool {
    run_save_conflict_command(app, tab_index, view_id, |app, tab_index| {
        crate::app::app_state::workspace_controller::save_file_as_at(app, tab_index)
    })
}

fn run_save_conflict_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    action: impl FnOnce(&mut ScratchpadApp, usize) -> bool,
) -> bool {
    if !activate_pending_view_command(app, tab_index, view_id) || !action(app, tab_index) {
        return false;
    }
    app.set_pending_action(None);
    true
}

pub(crate) fn activate_pending_view_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> bool {
    if tab_index >= app.tab_manager.tabs.as_slice().len() {
        return false;
    }

    if app.tab_manager.active_tab_index != tab_index {
        app.handle_command(AppCommand::ActivateTab { index: tab_index });
    }

    let Some(tab) = app.tab_manager.tabs.as_slice().get(tab_index) else {
        return false;
    };
    if tab.view(view_id).is_none() {
        return false;
    }

    if tab.active_view_id != view_id {
        app.handle_command(AppCommand::ActivateView { view_id });
    }

    app.tab_manager
        .tabs
        .as_slice()
        .get(tab_index)
        .is_some_and(|tab| tab.active_view_id == view_id)
}

fn removed_buffer_ids(before: Vec<BufferId>, after: &[BufferId]) -> Vec<BufferId> {
    before
        .into_iter()
        .filter(|buffer_id| !after.contains(buffer_id))
        .collect()
}
