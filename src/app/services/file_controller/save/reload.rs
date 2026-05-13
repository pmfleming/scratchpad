use super::{
    BufferId, DiskFileState, FileController, Path, PendingBackgroundAction, ScratchpadApp,
    StatusDomain, diagnostics,
};
use crate::app::app_state::{
    PendingReloadBufferAction, PendingReloadMode, PendingReopenWithEncodingAction,
};
use crate::app::services::background_io::LoadedPathResult;

impl FileController {
    pub(crate) fn reload_buffer_from_disk(app: &mut ScratchpadApp, index: usize) -> bool {
        let Some(target) = Self::active_buffer_path(app, index) else {
            return false;
        };
        if Self::has_pending_reload_for_buffer(app, target.buffer_id) {
            return true;
        }

        app.queue_background_path_loads(
            vec![target.path.clone()],
            PendingBackgroundAction::ReloadBuffer(PendingReloadBufferAction {
                buffer_id: target.buffer_id,
                expected_path: target.path,
                buffer_name: target.buffer_name,
                previous_disk_state: target.disk_state,
                mode: PendingReloadMode::ExplicitReload,
            }),
        );
        true
    }

    pub(crate) fn reopen_buffer_with_encoding(
        app: &mut ScratchpadApp,
        index: usize,
        encoding_name: &str,
    ) -> bool {
        if index >= app.tab_manager.tabs.as_slice().len() {
            return false;
        }

        if app.tab_manager.tabs.as_slice()[index]
            .active_buffer()
            .is_dirty
        {
            app.state.status.set_warning_status_in_domain(
                StatusDomain::Encoding,
                "Save or discard changes before reopening with a different encoding.",
            );
            return false;
        }

        let Some(path) = Self::buffer_path(app, index) else {
            app.state.status.set_warning_status_in_domain(
                StatusDomain::Encoding,
                "Save this file before reopening it with another encoding.",
            );
            return false;
        };

        let buffer = app.tab_manager.tabs.as_slice()[index].active_buffer();
        if Self::has_pending_reopen_with_encoding_for_buffer(app, buffer.id) {
            return true;
        }

        app.queue_background_path_load_with_encoding(
            path.clone(),
            encoding_name.to_owned(),
            PendingBackgroundAction::ReopenWithEncoding(PendingReopenWithEncodingAction {
                buffer_id: buffer.id,
                expected_path: path,
                buffer_name: buffer.name.clone(),
            }),
        );
        true
    }

    pub(crate) fn apply_async_reload_buffer_result(
        app: &mut ScratchpadApp,
        action: PendingReloadBufferAction,
        mut results: Vec<LoadedPathResult>,
    ) {
        let Some((tab_index, result)) = Self::resolve_background_result(
            app,
            action.buffer_id,
            &action.expected_path,
            &mut results,
        ) else {
            return;
        };

        let current_buffer = app.tab_manager.tabs.as_slice()[tab_index]
            .buffer_by_id(action.buffer_id)
            .expect("buffer location validated");
        if current_buffer.is_dirty && action.mode == PendingReloadMode::AutoRefreshCleanBuffer {
            let buffer = app.tab_manager.tabs.as_mut_slice()[tab_index]
                .buffer_by_id_mut(action.buffer_id)
                .expect("buffer location validated");
            buffer.mark_conflict_on_disk(result.disk_state);
            app.state.status.set_warning_status_in_domain(
                StatusDomain::Disk,
                format!(
                    "{} changed on disk. Your tab has unsaved edits.",
                    action.buffer_name
                ),
            );
            app.tab_manager.mark_session_dirty();
            return;
        }

        if action.mode == PendingReloadMode::AutoRefreshCleanBuffer
            && current_buffer.disk_state != action.previous_disk_state
        {
            return;
        }

        match result.result {
            Ok(loaded) => {
                let buffer_name = Self::replace_buffer_from_loaded_buffer(
                    app,
                    tab_index,
                    action.buffer_id,
                    loaded,
                    result.disk_state,
                    action.mode == PendingReloadMode::AutoRefreshCleanBuffer,
                );
                match action.mode {
                    PendingReloadMode::AutoRefreshCleanBuffer => {
                        app.state.status.set_info_status_in_domain(
                            StatusDomain::Disk,
                            format!("Reloaded {buffer_name} because it changed on disk."),
                        )
                    }
                    PendingReloadMode::ExplicitReload => {
                        app.state.status.set_info_status_in_domain(
                            StatusDomain::Disk,
                            format!("Reloaded {buffer_name} from disk."),
                        )
                    }
                }
            }
            Err(error) => {
                diagnostics::record_io_error(
                    "reload_file",
                    Some(&action.expected_path),
                    "file_controller::save",
                    &error,
                );
                Self::handle_async_reload_error(app, tab_index, &action, result.disk_state, error)
            }
        }
    }

    pub(crate) fn apply_async_reopen_with_encoding_result(
        app: &mut ScratchpadApp,
        action: PendingReopenWithEncodingAction,
        mut results: Vec<LoadedPathResult>,
    ) {
        let Some((tab_index, result)) = Self::resolve_background_result(
            app,
            action.buffer_id,
            &action.expected_path,
            &mut results,
        ) else {
            return;
        };

        if app.tab_manager.tabs.as_slice()[tab_index]
            .buffer_by_id(action.buffer_id)
            .is_some_and(|buffer| buffer.is_dirty)
        {
            return;
        }

        match result.result {
            Ok(loaded) => {
                let encoding_label = loaded.format.encoding_label();
                let buffer_name = Self::replace_buffer_from_loaded_buffer(
                    app,
                    tab_index,
                    action.buffer_id,
                    loaded,
                    result.disk_state,
                    false,
                );
                app.state.status.set_info_status_in_domain(
                    StatusDomain::Encoding,
                    format!("Reopened {buffer_name} with {encoding_label}."),
                );
            }
            Err(error) => {
                diagnostics::record_io_error(
                    "reopen_with_encoding",
                    Some(&action.expected_path),
                    "file_controller::save",
                    &error,
                );
                app.state.status.set_error_status_with_detail(
                    StatusDomain::Encoding,
                    format!(
                        "Could not reopen {} with that encoding.",
                        action.buffer_name
                    ),
                    error,
                );
            }
        }
    }

    fn replace_buffer_from_loaded_buffer(
        app: &mut ScratchpadApp,
        index: usize,
        buffer_id: BufferId,
        loaded: crate::app::domain::BufferState,
        disk_state: Option<DiskFileState>,
        mark_auto_reloaded: bool,
    ) -> String {
        app.tab_manager.tabs.as_mut_slice()[index]
            .clear_view_state_for_buffer_replacement(buffer_id);
        let (buffer_name, deferred_refresh) = {
            let buffer = app.tab_manager.tabs.as_mut_slice()[index]
                .buffer_by_id_mut(buffer_id)
                .expect("buffer location validated");
            buffer.replace_from_loaded_buffer(loaded);
            buffer.is_dirty = false;
            if mark_auto_reloaded {
                buffer.mark_auto_reloaded_from_disk(disk_state);
            } else {
                buffer.sync_to_disk_state(disk_state);
            }
            (buffer.name.clone(), Self::deferred_buffer_refresh(buffer))
        };
        app.mark_search_dirty();
        app.tab_manager.mark_session_dirty();
        Self::queue_deferred_buffer_refreshes(app, deferred_refresh);
        buffer_name
    }

    fn resolve_background_result(
        app: &mut ScratchpadApp,
        buffer_id: BufferId,
        expected_path: &Path,
        results: &mut Vec<LoadedPathResult>,
    ) -> Option<(usize, LoadedPathResult)> {
        let result = results.pop()?;
        let (tab_index, current_path) = Self::find_buffer_location(app, buffer_id)?;
        crate::app::paths_match(&current_path, expected_path).then_some((tab_index, result))
    }

    fn handle_async_reload_error(
        app: &mut ScratchpadApp,
        tab_index: usize,
        action: &PendingReloadBufferAction,
        disk_state: Option<DiskFileState>,
        error: String,
    ) {
        match action.mode {
            PendingReloadMode::AutoRefreshCleanBuffer => {
                let buffer = app.tab_manager.tabs.as_mut_slice()[tab_index]
                    .buffer_by_id_mut(action.buffer_id)
                    .expect("buffer location validated");
                buffer.mark_stale_on_disk(disk_state);
                app.state.status.set_warning_status_with_detail(
                    StatusDomain::Disk,
                    format!(
                        "Detected a newer on-disk version of {} but could not reload it.",
                        action.buffer_name
                    ),
                    error,
                );
                app.tab_manager.mark_session_dirty();
            }
            PendingReloadMode::ExplicitReload => {
                app.state.status.report_reload_failed(error);
            }
        }
    }
}
