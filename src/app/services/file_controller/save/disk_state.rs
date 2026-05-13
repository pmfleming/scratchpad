use super::FileController;
use crate::app::app_state::{
    PendingBackgroundAction, PendingReloadBufferAction, PendingReloadMode, ScratchpadApp,
    StatusDomain,
};
use crate::app::diagnostics;
use crate::app::domain::{BufferFreshness, BufferId, DiskFileState};
use crate::app::services::file_service::FileService;
use std::path::PathBuf;

impl FileController {
    pub(crate) fn refresh_active_buffer_disk_state(app: &mut ScratchpadApp) -> bool {
        let index = app.tab_manager.active_tab_index;
        Self::refresh_buffer_disk_state(app, index)
    }

    pub(crate) fn refresh_buffer_disk_state_by_id(
        app: &mut ScratchpadApp,
        buffer_id: BufferId,
    ) -> bool {
        let Some((index, path)) = Self::find_buffer_location(app, buffer_id) else {
            return false;
        };

        Self::refresh_buffer_disk_state_for_path(app, index, buffer_id, path)
    }

    pub(super) fn refresh_buffer_disk_state(app: &mut ScratchpadApp, index: usize) -> bool {
        let Some(target) = Self::active_buffer_path(app, index) else {
            return false;
        };

        Self::refresh_buffer_disk_state_for_path(app, index, target.buffer_id, target.path)
    }

    fn refresh_buffer_disk_state_for_path(
        app: &mut ScratchpadApp,
        index: usize,
        buffer_id: BufferId,
        path: PathBuf,
    ) -> bool {
        match FileService::read_disk_state(&path) {
            Ok(disk_state) => {
                Self::handle_refreshed_disk_state(app, index, buffer_id, path, disk_state)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Self::mark_buffer_missing_on_disk(app, index, buffer_id)
            }
            Err(error) => {
                diagnostics::record_io_error(
                    "refresh_disk_state",
                    Some(&path),
                    "file_controller::save",
                    &error,
                );
                false
            }
        }
    }

    fn handle_refreshed_disk_state(
        app: &mut ScratchpadApp,
        index: usize,
        buffer_id: BufferId,
        path: PathBuf,
        disk_state: DiskFileState,
    ) -> bool {
        let (is_dirty, known_disk_state, freshness, buffer_name) = {
            let Some(buffer) = app.tab_manager.tabs.as_slice()[index].buffer_by_id(buffer_id)
            else {
                return false;
            };
            (
                buffer.is_dirty,
                buffer.disk_state.clone(),
                buffer.freshness,
                buffer.name.clone(),
            )
        };

        if known_disk_state.as_ref() == Some(&disk_state) || known_disk_state.is_none() {
            if freshness == BufferFreshness::AutoReloaded {
                return false;
            }
            Self::sync_buffer_disk_state(app, index, buffer_id, Some(disk_state));
            return false;
        }

        if is_dirty {
            Self::mark_buffer_conflict_on_disk(app, index, buffer_id, buffer_name, disk_state);
            return true;
        }

        Self::queue_clean_buffer_auto_reload(app, buffer_id, path, buffer_name, known_disk_state)
    }

    fn mark_buffer_conflict_on_disk(
        app: &mut ScratchpadApp,
        index: usize,
        buffer_id: BufferId,
        buffer_name: String,
        disk_state: DiskFileState,
    ) {
        let buffer = app.tab_manager.tabs.as_mut_slice()[index]
            .buffer_by_id_mut(buffer_id)
            .expect("buffer location validated");
        buffer.mark_conflict_on_disk(Some(disk_state));
        app.state.status.set_warning_status_in_domain(
            StatusDomain::Disk,
            format!("{buffer_name} changed on disk. Your tab has unsaved edits."),
        );
        app.tab_manager.mark_session_dirty();
    }

    fn queue_clean_buffer_auto_reload(
        app: &mut ScratchpadApp,
        buffer_id: BufferId,
        path: PathBuf,
        buffer_name: String,
        known_disk_state: Option<DiskFileState>,
    ) -> bool {
        if Self::has_pending_reload_for_buffer(app, buffer_id) {
            return true;
        }

        app.queue_background_path_loads(
            vec![path.clone()],
            PendingBackgroundAction::ReloadBuffer(PendingReloadBufferAction {
                buffer_id,
                expected_path: path,
                buffer_name,
                previous_disk_state: known_disk_state,
                mode: PendingReloadMode::AutoRefreshCleanBuffer,
            }),
        );
        true
    }

    fn mark_buffer_missing_on_disk(
        app: &mut ScratchpadApp,
        index: usize,
        buffer_id: BufferId,
    ) -> bool {
        let Some(buffer_name) = app.tab_manager.tabs.as_slice()[index]
            .buffer_by_id(buffer_id)
            .map(|buffer| buffer.name.clone())
        else {
            return false;
        };
        let buffer = app.tab_manager.tabs.as_mut_slice()[index]
            .buffer_by_id_mut(buffer_id)
            .expect("buffer location validated");
        buffer.disk_state = None;
        buffer.mark_missing_on_disk();
        app.state.status.set_warning_status_in_domain(
            StatusDomain::Disk,
            format!("{buffer_name} is missing on disk."),
        );
        app.tab_manager.mark_session_dirty();
        true
    }
}
