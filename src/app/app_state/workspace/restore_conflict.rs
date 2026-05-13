use super::super::{
    PendingBackgroundAction, PendingStartupRestoreCompareAction, ScratchpadApp,
    StartupRestoreConflict, StatusDomain,
};
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferFreshness, BufferId, ViewId, WorkspaceTab};
use crate::app::services::background_io::LoadedPathResult;

impl ScratchpadApp {
    pub(crate) fn refresh_startup_restore_conflicts(&mut self) {
        self.state.startup_restore_conflicts = self
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .enumerate()
            .flat_map(|(tab_index, tab)| collect_tab_restore_conflicts(tab_index, tab))
            .collect();
    }

    pub(crate) fn current_startup_restore_conflict(&self) -> Option<&StartupRestoreConflict> {
        self.state.startup_restore_conflicts.first()
    }

    pub(crate) fn dismiss_current_startup_restore_conflict(&mut self) {
        if !self.state.startup_restore_conflicts.is_empty() {
            self.state.startup_restore_conflicts.remove(0);
        }
    }

    pub(crate) fn keep_session_version_for_current_startup_restore_conflict(&mut self) {
        let Some(conflict) = self.current_startup_restore_conflict().cloned() else {
            return;
        };
        if let Some(buffer) = self
            .tab_manager
            .tabs
            .as_mut_slice()
            .get_mut(conflict.tab_index)
            .and_then(|tab| {
                tab.buffer_for_view(conflict.view_id)
                    .map(|buffer| buffer.id)
            })
            .and_then(|buffer_id| {
                self.tab_manager
                    .tabs
                    .as_mut_slice()
                    .get_mut(conflict.tab_index)
                    .and_then(|tab| tab.buffer_by_id_mut(buffer_id))
            })
        {
            buffer.document_mut().revalidate_history_for_current_text();
        }
        self.dismiss_current_startup_restore_conflict();
    }

    pub(crate) fn open_disk_version_for_current_startup_restore_conflict(&mut self) -> bool {
        let Some(conflict) = take_current_startup_restore_conflict(self) else {
            return false;
        };
        if let Some(buffer) = self
            .tab_manager
            .tabs
            .as_mut_slice()
            .get_mut(conflict.tab_index)
            .and_then(|tab| {
                tab.buffer_for_view(conflict.view_id)
                    .map(|buffer| buffer.id)
            })
            .and_then(|buffer_id| {
                self.tab_manager
                    .tabs
                    .as_mut_slice()
                    .get_mut(conflict.tab_index)
                    .and_then(|tab| tab.buffer_by_id_mut(buffer_id))
            })
        {
            buffer.document_mut().clear_operation_history();
        }

        self.queue_background_path_loads(
            vec![conflict.path.clone()],
            PendingBackgroundAction::StartupRestoreCompare(PendingStartupRestoreCompareAction {
                conflict,
            }),
        );
        true
    }

    pub(crate) fn apply_async_startup_restore_compare_result(
        &mut self,
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
                self.state.status.set_warning_status_with_detail(
                    StatusDomain::Disk,
                    format!("Could not load disk version of {}.", conflict.buffer_name),
                    error,
                );
                return;
            }
        };

        let Some(buffer_id) = conflicted_buffer_id(self, &conflict) else {
            self.state.status.set_warning_status_in_domain(
                StatusDomain::Layout,
                format!(
                    "Could not find the conflicted tab for {}.",
                    conflict.buffer_name
                ),
            );
            return;
        };

        if conflict.tab_index < self.tab_manager.tabs.as_slice().len() {
            self.handle_command(AppCommand::ActivateTab {
                index: conflict.tab_index,
            });
            self.handle_command(AppCommand::ActivateView {
                view_id: conflict.view_id,
            });
        }

        let settings_path = self.settings_path().to_path_buf();
        if let Some(tab) = self
            .tab_manager
            .tabs
            .as_mut_slice()
            .get_mut(conflict.tab_index)
        {
            tab.clear_view_state_for_buffer_replacement(buffer_id);
            for view in &mut tab.views {
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

        self.mark_search_dirty();
        self.tab_manager.mark_session_dirty();
        let _ = self.persist_session_now();
        self.state.status.set_info_status_in_domain(
            StatusDomain::Disk,
            format!("Loaded disk version of {}.", conflict.buffer_name),
        );
    }
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
    tab.active_view()
        .filter(|view| view.buffer_id == buffer_id)
        .map(|view| view.id)
        .or_else(|| {
            tab.views
                .iter()
                .find(|view| view.buffer_id == buffer_id)
                .map(|view| view.id)
        })
}

fn take_current_startup_restore_conflict(
    app: &mut ScratchpadApp,
) -> Option<StartupRestoreConflict> {
    if app.state.startup_restore_conflicts.is_empty() {
        None
    } else {
        Some(app.state.startup_restore_conflicts.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BufferFreshness, LoadedPathResult, PendingStartupRestoreCompareAction, ScratchpadApp,
        StartupRestoreConflict,
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
        let view_id = tab.active_view_id;
        let buffer_id = tab.buffer.id;
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

        app.apply_async_startup_restore_compare_result(
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
