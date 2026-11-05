use super::FileController;
use crate::app::app_state::{
    PendingBackgroundAction, PendingReloadBufferAction, PendingReloadMode, ScratchpadApp,
    StatusDomain,
};
use crate::app::{
    diagnostics,
    domain::{BufferId, DiskFileState},
    services::{
        file_service::FileService,
        session_store::activity::{ActivityEvent, ActivityKind},
    },
};
use std::path::PathBuf;

impl FileController {
    pub(crate) fn refresh_active_buffer_disk_state(app: &mut ScratchpadApp) -> bool {
        Self::refresh_buffer_disk_state(app, app.tab_manager.active_tab_index)
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
        if app
            .state
            .background_io
            .has_pending_save_for_buffer(buffer_id)
            || app.tab_manager.cold_session_tabs().contains_key(&index)
            || app.tab_manager.tabs.as_slice()[index]
                .buffer_by_id(buffer_id)
                .is_none_or(|buffer| buffer.is_loading_preview)
        {
            return false;
        }
        match FileService::read_disk_state(&path) {
            Ok(state) => Self::handle_refreshed_disk_state(app, index, buffer_id, state),
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
        state: DiskFileState,
    ) -> bool {
        let Some(buffer) = app.tab_manager.tabs.as_slice()[index].buffer_by_id(buffer_id) else {
            return false;
        };
        if buffer.disk_state.as_ref() == Some(&state) && !buffer.observed_disk_missing {
            return false;
        }
        if Self::has_pending_reload_for_buffer(app, buffer_id) {
            return true;
        }
        let Some(action) =
            PendingReloadBufferAction::new(buffer, PendingReloadMode::ObserveDiskChange)
        else {
            return false;
        };
        app.queue_background_path_loads(
            vec![action.expected_path.clone()],
            PendingBackgroundAction::ReloadBuffer(action),
        );
        true
    }
    pub(super) fn mark_buffer_missing_on_disk(
        app: &mut ScratchpadApp,
        index: usize,
        buffer_id: BufferId,
    ) -> bool {
        let Some(buffer) = app.tab_manager.tabs.as_mut_slice()[index].buffer_by_id_mut(buffer_id)
        else {
            return false;
        };
        if buffer.observed_disk_missing {
            return false;
        }
        buffer.disk_state = None;
        buffer.mark_missing_on_disk();
        buffer.observed_disk_missing = true;
        buffer.pending_disk_observation = false;
        let event = ActivityEvent::new(ActivityKind::Missing, buffer);
        let name = buffer.name.clone();
        app.state.persistence.pending_activity.push(event);
        app.tab_manager.mark_session_dirty();
        app.state.status.set_warning_status_in_domain(
            StatusDomain::Disk,
            format!("{name} is missing on disk."),
        );
        true
    }
}
