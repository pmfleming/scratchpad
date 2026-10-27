use super::{
    BufferId, DiskFileState, FileController, PendingBackgroundAction, ScratchpadApp, StatusDomain,
    diagnostics,
};
use crate::app::{
    app_state::{PendingReloadBufferAction, PendingReloadMode},
    domain::BufferState,
    services::background_io::LoadedPathResult,
};

impl FileController {
    pub(crate) fn reload_buffer_from_disk(app: &mut ScratchpadApp, index: usize) -> bool {
        let Some(action) = app.tab_manager.tabs.as_slice().get(index).and_then(|tab| {
            PendingReloadBufferAction::new(tab.active_buffer(), PendingReloadMode::ExplicitReload)
        }) else {
            return false;
        };
        if Self::has_pending_reload_for_buffer(app, action.buffer_id) {
            return true;
        }
        app.queue_background_path_loads(
            vec![action.expected_path.clone()],
            PendingBackgroundAction::ReloadBuffer(action),
        );
        true
    }

    pub(crate) fn reopen_buffer_with_encoding(
        app: &mut ScratchpadApp,
        index: usize,
        encoding_name: &str,
    ) -> bool {
        let Some(tab) = app.tab_manager.tabs.as_slice().get(index) else {
            return false;
        };
        let buffer = tab.active_buffer();
        if buffer.is_dirty {
            app.state.status.set_warning_status_in_domain(
                StatusDomain::Encoding,
                "Save or discard changes before reopening with a different encoding.",
            );
            return false;
        }
        let Some(action) =
            PendingReloadBufferAction::new(buffer, PendingReloadMode::ReopenWithEncoding)
        else {
            app.state.status.set_warning_status_in_domain(
                StatusDomain::Encoding,
                "Save this file before reopening it with another encoding.",
            );
            return false;
        };
        if Self::has_pending_reopen_with_encoding_for_buffer(app, action.buffer_id) {
            return true;
        }
        app.queue_background_path_load_with_encoding(
            action.expected_path.clone(),
            encoding_name.to_owned(),
            PendingBackgroundAction::ReopenWithEncoding(action),
        );
        true
    }

    pub(crate) fn apply_async_reload_buffer_result(
        app: &mut ScratchpadApp,
        action: PendingReloadBufferAction,
        results: Vec<LoadedPathResult>,
    ) {
        let Some((index, result)) = Self::resolve_background_result(app, &action, results) else {
            return;
        };
        let Some(buffer) = app
            .tab_manager
            .tabs
            .as_mut_slice()
            .get_mut(index)
            .and_then(|tab| tab.buffer_by_id_mut(action.buffer_id))
        else {
            return;
        };
        // Explicit reload may discard the edits that existed when requested, never
        // edits made while I/O was pending (even if subsequently saved/undone).
        let changed = buffer.document().revision_key() != action.expected_revision;
        let requires_clean = action.mode != PendingReloadMode::ExplicitReload;
        if changed || (requires_clean && buffer.is_dirty) {
            if action.mode == PendingReloadMode::AutoRefreshCleanBuffer {
                buffer.mark_conflict_on_disk(result.disk_state);
                app.tab_manager.mark_session_dirty();
            }
            app.state.status.set_warning_status_in_domain(
                action.mode.status_domain(),
                format!(
                    "{} changed while loading. Kept the current text.",
                    action.buffer_name
                ),
            );
            return;
        }
        match result.result {
            Ok(loaded) => {
                let message = action.mode.success_message(&action.buffer_name, &loaded);
                if Self::replace_buffer_from_loaded_buffer(
                    app,
                    index,
                    action.buffer_id,
                    loaded,
                    result.disk_state,
                    action.mode == PendingReloadMode::AutoRefreshCleanBuffer,
                )
                .is_some()
                {
                    app.state
                        .status
                        .set_info_status_in_domain(action.mode.status_domain(), message);
                }
            }
            Err(error) => {
                diagnostics::record_io_error(
                    "reload_file",
                    Some(&action.expected_path),
                    "file_controller::save",
                    &error,
                );
                Self::handle_async_reload_error(app, index, &action, result.disk_state, error);
            }
        }
    }

    fn replace_buffer_from_loaded_buffer(
        app: &mut ScratchpadApp,
        index: usize,
        buffer_id: BufferId,
        loaded: BufferState,
        disk_state: Option<DiskFileState>,
        mark_auto_reloaded: bool,
    ) -> Option<()> {
        let tab = app.tab_manager.tabs.as_mut_slice().get_mut(index)?;
        tab.clear_view_state_for_buffer_replacement(buffer_id);
        let buffer = tab.buffer_by_id_mut(buffer_id)?;
        buffer.replace_from_loaded_buffer(loaded);
        buffer.is_dirty = false;
        if mark_auto_reloaded {
            buffer.mark_auto_reloaded_from_disk(disk_state);
        } else {
            buffer.sync_to_disk_state(disk_state);
        }
        let refresh = Self::deferred_buffer_refresh(buffer);
        app.tab_manager.rebuild_buffer_tab_index();
        crate::app::app_state::search_runtime::mark_search_dirty(app);
        app.tab_manager.mark_session_dirty();
        Self::queue_deferred_buffer_refreshes(app, refresh);
        Some(())
    }

    fn resolve_background_result(
        app: &mut ScratchpadApp,
        action: &PendingReloadBufferAction,
        results: Vec<LoadedPathResult>,
    ) -> Option<(usize, LoadedPathResult)> {
        let [result]: [LoadedPathResult; 1] = results.try_into().ok()?;
        if !crate::app::paths_match(&result.path, &action.expected_path) {
            return None;
        }
        let index = crate::app::app_state::workspace::mutation::tab_index_for_buffer(
            app,
            action.buffer_id,
        )?;
        let buffer = app
            .tab_manager
            .tabs
            .as_slice()
            .get(index)?
            .buffer_by_id(action.buffer_id)?;
        // A completed save or newer reload supersedes this request's disk baseline.
        (buffer
            .path
            .as_deref()
            .is_some_and(|path| crate::app::paths_match(path, &action.expected_path))
            && buffer.disk_state == action.previous_disk_state)
            .then_some((index, result))
    }

    fn handle_async_reload_error(
        app: &mut ScratchpadApp,
        index: usize,
        action: &PendingReloadBufferAction,
        disk_state: Option<DiskFileState>,
        error: String,
    ) {
        if action.mode == PendingReloadMode::AutoRefreshCleanBuffer {
            let Some(buffer) = app
                .tab_manager
                .tabs
                .as_mut_slice()
                .get_mut(index)
                .and_then(|tab| tab.buffer_by_id_mut(action.buffer_id))
            else {
                return;
            };
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
        } else {
            app.state.status.set_error_status_with_detail(
                action.mode.status_domain(),
                format!("Could not reload {} from disk.", action.buffer_name),
                error,
            );
        }
    }
}

impl PendingReloadMode {
    fn status_domain(self) -> StatusDomain {
        if self == Self::ReopenWithEncoding {
            StatusDomain::Encoding
        } else {
            StatusDomain::Disk
        }
    }

    fn success_message(self, name: &str, loaded: &BufferState) -> String {
        match self {
            Self::AutoRefreshCleanBuffer => format!("Reloaded {name} because it changed on disk."),
            Self::ExplicitReload => format!("Reloaded {name} from disk."),
            Self::ReopenWithEncoding => {
                format!("Reopened {name} with {}.", loaded.format.encoding_label())
            }
        }
    }
}
