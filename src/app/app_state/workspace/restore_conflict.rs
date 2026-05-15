use super::super::{
    PendingBackgroundAction, PendingStartupRestoreCompareAction, ScratchpadApp,
    StartupRestoreConflict, StatusDomain,
};
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::domain::{BufferFreshness, BufferId, ViewId, WorkspaceTab};
use crate::app::services::background_io::LoadedPathResult;

pub(crate) fn refresh_startup_restore_conflicts(app: &mut ScratchpadApp) {
    let conflicts = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .enumerate()
        .flat_map(|(tab_index, tab)| collect_tab_restore_conflicts(tab_index, tab))
        .collect();
    app.state.dialogs.set_startup_restore_conflicts(conflicts);
}

pub(crate) fn current_startup_restore_conflict(
    app: &ScratchpadApp,
) -> Option<&StartupRestoreConflict> {
    app.state.dialogs.current_startup_restore_conflict()
}

pub(crate) fn dismiss_current_startup_restore_conflict(app: &mut ScratchpadApp) {
    app.state.dialogs.dismiss_current_startup_restore_conflict();
}

pub(crate) fn keep_session_version_for_current_startup_restore_conflict(app: &mut ScratchpadApp) {
    let Some(conflict) = current_startup_restore_conflict(app).cloned() else {
        return;
    };
    if let Some(buffer) = app
        .tab_manager
        .tabs
        .as_mut_slice()
        .get_mut(conflict.tab_index)
        .and_then(|tab| {
            tab.buffer_for_view(conflict.view_id)
                .map(|buffer| buffer.id)
        })
        .and_then(|buffer_id| {
            app.tab_manager
                .tabs
                .as_mut_slice()
                .get_mut(conflict.tab_index)
                .and_then(|tab| tab.buffer_by_id_mut(buffer_id))
        })
    {
        buffer.document_mut().revalidate_history_for_current_text();
    }
    dismiss_current_startup_restore_conflict(app);
}

pub(crate) fn open_disk_version_for_current_startup_restore_conflict(
    app: &mut ScratchpadApp,
) -> bool {
    let Some(conflict) = take_current_startup_restore_conflict(app) else {
        return false;
    };
    if let Some(buffer) = app
        .tab_manager
        .tabs
        .as_mut_slice()
        .get_mut(conflict.tab_index)
        .and_then(|tab| {
            tab.buffer_for_view(conflict.view_id)
                .map(|buffer| buffer.id)
        })
        .and_then(|buffer_id| {
            app.tab_manager
                .tabs
                .as_mut_slice()
                .get_mut(conflict.tab_index)
                .and_then(|tab| tab.buffer_by_id_mut(buffer_id))
        })
    {
        buffer.document_mut().clear_operation_history();
    }

    app.queue_background_path_loads(
        vec![conflict.path.clone()],
        PendingBackgroundAction::StartupRestoreCompare(PendingStartupRestoreCompareAction {
            conflict,
        }),
    );
    true
}

pub(crate) fn apply_async_startup_restore_compare_result(
    app: &mut ScratchpadApp,
    action: PendingStartupRestoreCompareAction,
    mut results: Vec<LoadedPathResult>,
) {
    let Some(result) = results.pop() else {
        return;
    };
    let conflict = action.conflict;

    let loaded_buffer = match result.result {
        Ok(buffer) => buffer,
        Err(error) => {
            app.state.status.set_warning_status_with_detail(
                StatusDomain::Disk,
                format!("Could not load disk version of {}.", conflict.buffer_name),
                error,
            );
            return;
        }
    };

    let Some(buffer_id) = conflicted_buffer_id(app, &conflict) else {
        app.state.status.set_warning_status_in_domain(
            StatusDomain::Layout,
            format!(
                "Could not find the conflicted tab for {}.",
                conflict.buffer_name
            ),
        );
        return;
    };

    if conflict.tab_index < app.tab_manager.tabs.as_slice().len() {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ActivateTab {
                index: conflict.tab_index,
            }),
        );
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ActivateView {
                view_id: conflict.view_id,
            }),
        );
    }

    let settings_path = crate::app::app_state::settings_state::settings_path(app).to_path_buf();
    if let Some(tab) = app
        .tab_manager
        .tabs
        .as_mut_slice()
        .get_mut(conflict.tab_index)
    {
        tab.clear_view_state_for_buffer_replacement(buffer_id);
        for view in &mut tab.layout.views {
            if view.buffer_id == buffer_id {
                view.layout_cache.clear();
            }
        }
        if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
            buffer.replace_from_loaded_buffer(loaded_buffer);
            buffer.is_dirty = false;
            buffer.sync_to_disk_state(result.disk_state);
            buffer.is_settings_file = buffer
                .path
                .as_ref()
                .is_some_and(|path| crate::app::paths_match(path, &settings_path));
        }
    }

    crate::app::app_state::search_runtime::mark_search_dirty(app);
    app.tab_manager.mark_session_dirty();
    let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
    app.state.status.set_info_status_in_domain(
        StatusDomain::Disk,
        format!("Loaded disk version of {}.", conflict.buffer_name),
    );
}

fn conflicted_buffer_id(
    app: &ScratchpadApp,
    conflict: &StartupRestoreConflict,
) -> Option<BufferId> {
    let tab = app.tab_manager.tabs.as_slice().get(conflict.tab_index)?;
    let buffer = tab.buffer_for_view(conflict.view_id)?;
    buffer
        .path
        .as_ref()
        .and_then(|path| crate::app::paths_match(path, &conflict.path).then_some(buffer.id))
}

fn collect_tab_restore_conflicts(
    tab_index: usize,
    tab: &WorkspaceTab,
) -> impl Iterator<Item = StartupRestoreConflict> + '_ {
    tab.buffers().filter_map(move |buffer| {
        (buffer.freshness == BufferFreshness::ConflictOnDisk)
            .then(|| buffer.path.clone())?
            .and_then(|path| {
                representative_view_id(tab, buffer.id).map(|view_id| StartupRestoreConflict {
                    tab_index,
                    view_id,
                    buffer_name: buffer.name.clone(),
                    path,
                })
            })
    })
}

fn representative_view_id(tab: &WorkspaceTab, buffer_id: BufferId) -> Option<ViewId> {
    tab.layout
        .active_view()
        .filter(|view| view.buffer_id == buffer_id)
        .map(|view| view.id)
        .or_else(|| {
            tab.layout
                .views
                .iter()
                .find(|view| view.buffer_id == buffer_id)
                .map(|view| view.id)
        })
}

fn take_current_startup_restore_conflict(
    app: &mut ScratchpadApp,
) -> Option<StartupRestoreConflict> {
    app.state.dialogs.take_current_startup_restore_conflict()
}

#[cfg(test)]
mod tests {
    use super::{
        BufferFreshness, LoadedPathResult, PendingStartupRestoreCompareAction, ScratchpadApp,
        StartupRestoreConflict, apply_async_startup_restore_compare_result,
    };
    use crate::app::domain::{BufferState, DiskFileState, TabManager, WorkspaceTab};
    use crate::app::services::session_store::SessionStore;
    use crate::app::services::settings_store::SettingsStore;
    use crate::app::startup::StartupOptions;

    #[test]
    fn disk_version_replaces_conflicted_buffer_without_opening_duplicate_tab() {
        let temp_dir = tempfile::tempdir().expect("create temp app root");
        let root = temp_dir.keep();
        let path = root.join("note.txt");
        let disk_state = DiskFileState {
            modified_millis: Some(12),
            len: 4,
        };

        let mut app = ScratchpadApp::with_stores_and_startup(
            SessionStore::new(root.clone()),
            SettingsStore::new(root),
            StartupOptions::default(),
        );
        app.set_session_persist_on_drop(false);

        let mut restored_buffer = BufferState::new(
            "note.txt".to_owned(),
            "session".to_owned(),
            Some(path.clone()),
        );
        restored_buffer.is_dirty = true;
        restored_buffer.mark_conflict_on_disk(Some(disk_state.clone()));
        let tab = WorkspaceTab::new(restored_buffer);
        let view_id = tab.layout.active_view_id;
        let buffer_id = tab.buffers.buffer.id;
        app.tab_manager = TabManager {
            tabs: vec![tab],
            active_tab_index: 0,
            pending_action: None,
            session_dirty: false,
            pending_scroll_to_active: false,
            buffer_tab_index: Default::default(),
            cold_session_tabs: Default::default(),
        };
        app.tab_manager.rebuild_buffer_tab_index();

        let conflict = StartupRestoreConflict {
            tab_index: 0,
            view_id,
            buffer_name: "note.txt".to_owned(),
            path: path.clone(),
        };
        let loaded_buffer =
            BufferState::new("note.txt".to_owned(), "disk".to_owned(), Some(path.clone()));

        apply_async_startup_restore_compare_result(
            &mut app,
            PendingStartupRestoreCompareAction { conflict },
            vec![LoadedPathResult {
                path,
                disk_state: Some(disk_state),
                result: Ok(loaded_buffer),
            }],
        );

        assert_eq!(app.tab_manager.tabs.as_slice().len(), 1);
        let buffer = app.tab_manager.tabs.as_slice()[0].active_buffer();
        assert_eq!(buffer.id, buffer_id);
        assert_eq!(buffer.text(), "disk");
        assert!(!buffer.is_dirty);
        assert_eq!(buffer.freshness, BufferFreshness::InSync);
    }
}
