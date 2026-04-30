use super::super::{
    PendingBackgroundAction, PendingStartupRestoreCompareAction, ScratchpadApp,
    StartupRestoreConflict,
};
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferFreshness, BufferId, ViewId, WorkspaceTab};
use crate::app::services::background_io::LoadedPathResult;

impl ScratchpadApp {
    pub(crate) fn refresh_startup_restore_conflicts(&mut self) {
        self.startup_restore_conflicts = self
            .tabs()
            .iter()
            .enumerate()
            .flat_map(|(tab_index, tab)| collect_tab_restore_conflicts(tab_index, tab))
            .collect();
    }

    pub(crate) fn current_startup_restore_conflict(&self) -> Option<&StartupRestoreConflict> {
        self.startup_restore_conflicts.first()
    }

    #[cfg(test)]
    pub(crate) fn startup_restore_conflict_count(&self) -> usize {
        self.startup_restore_conflicts.len()
    }

    pub(crate) fn dismiss_current_startup_restore_conflict(&mut self) {
        if !self.startup_restore_conflicts.is_empty() {
            self.startup_restore_conflicts.remove(0);
        }
    }

    pub(crate) fn open_disk_version_for_current_startup_restore_conflict(&mut self) -> bool {
        let Some(conflict) = take_current_startup_restore_conflict(self) else {
            return false;
        };

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

        let mut compare_buffer = match result.result {
            Ok(buffer) => buffer,
            Err(error) => {
                self.set_warning_status(format!(
                    "Could not open disk version of {} for comparison: {error}",
                    conflict.buffer_name
                ));
                return;
            }
        };

        if conflict.tab_index < self.tabs().len() {
            self.handle_command(AppCommand::ActivateTab {
                index: conflict.tab_index,
            });
            self.handle_command(AppCommand::ActivateView {
                view_id: conflict.view_id,
            });
        }

        compare_buffer.name = format!("{} (Disk)", conflict.buffer_name);
        compare_buffer.path = None;
        compare_buffer.is_settings_file = false;
        compare_buffer.sync_to_disk_state(result.disk_state);
        self.append_tab(WorkspaceTab::new(compare_buffer));
        self.set_info_status(format!(
            "Opened disk version of {} for comparison.",
            conflict.buffer_name
        ));
    }
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
    if app.startup_restore_conflicts.is_empty() {
        None
    } else {
        Some(app.startup_restore_conflicts.remove(0))
    }
}
