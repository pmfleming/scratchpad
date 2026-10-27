//! Separate startup recovery from the current disk version. This policy is
//! automatic; no deferred conflict dialog or async comparison state is needed.
use super::super::ScratchpadApp;
use crate::app::domain::{BufferFreshness, WorkspaceTab};
use crate::app::services::file_controller::FileController;

pub(crate) fn refresh_startup_restore_conflicts(app: &mut ScratchpadApp) {
    let mut changed = false;
    let mut disk_paths = Vec::new();
    for buffer in app
        .tab_manager
        .tabs
        .as_mut_slice()
        .iter_mut()
        .flat_map(WorkspaceTab::buffers_mut)
    {
        let recover = buffer.freshness == BufferFreshness::ConflictOnDisk
            || (buffer.freshness == BufferFreshness::MissingOnDisk && buffer.is_dirty);
        if !recover || buffer.path.is_none() {
            continue;
        }
        if buffer.freshness == BufferFreshness::ConflictOnDisk {
            disk_paths.extend(buffer.path.take());
        }
        buffer.recover_as_untitled("recovered");
        changed = true;
    }
    if changed {
        app.tab_manager.rebuild_buffer_tab_index();
        app.tab_manager.mark_session_dirty();
        if app
            .state
            .app_settings
            .history
            .recovery
            .open_changed_disk_files
            && !disk_paths.is_empty()
        {
            FileController::open_disk_paths_async(app, disk_paths);
        }
    }
}
