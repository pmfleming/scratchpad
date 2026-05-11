use super::*;

impl FileController {
    pub(crate) fn reload_buffer_from_disk(app: &mut ScratchpadApp, index: usize) -> bool {
        if index >= app.tabs().len() {
            return false;
        }

        let Some(path) = Self::buffer_path(app, index) else {
            return false;
        };
        let buffer = app.tabs()[index].active_buffer();
        if Self::has_pending_reload_for_buffer(app, buffer.id) {
            return true;
        }

        app.queue_background_path_loads(
            vec![path.clone()],
            PendingBackgroundAction::ReloadBuffer(PendingReloadBufferAction {
                buffer_id: buffer.id,
                expected_path: path,
                buffer_name: buffer.name.clone(),
                previous_disk_state: buffer.disk_state.clone(),
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
        if index >= app.tabs().len() {
            return false;
        }

        if app.tabs()[index].active_buffer().is_dirty {
            app.set_warning_status(
                "Save or discard changes before reopening with a different encoding.",
            );
            return false;
        }

        let Some(path) = Self::buffer_path(app, index) else {
            app.set_warning_status("Reopen With Encoding is available only for files on disk.");
            return false;
        };

        let buffer = app.tabs()[index].active_buffer();
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

    fn has_pending_reload_for_buffer(app: &ScratchpadApp, buffer_id: BufferId) -> bool {
        app.pending_background_actions.values().any(|action| {
            matches!(
                action,
                PendingBackgroundAction::ReloadBuffer(reload)
                    if reload.buffer_id == buffer_id
            )
        })
    }

    fn has_pending_reopen_with_encoding_for_buffer(
        app: &ScratchpadApp,
        buffer_id: BufferId,
    ) -> bool {
        app.pending_background_actions.values().any(|action| {
            matches!(
                action,
                PendingBackgroundAction::ReopenWithEncoding(reopen)
                    if reopen.buffer_id == buffer_id
            )
        })
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

        let current_buffer = app.tabs()[tab_index]
            .buffer_by_id(action.buffer_id)
            .expect("buffer location validated");
        if current_buffer.is_dirty && action.mode == PendingReloadMode::AutoRefreshCleanBuffer {
            let buffer = app.tabs_mut()[tab_index]
                .buffer_by_id_mut(action.buffer_id)
                .expect("buffer location validated");
            buffer.mark_conflict_on_disk(result.disk_state);
            app.set_warning_status(format!(
                "{} changed on disk. Your tab has unsaved edits.",
                action.buffer_name
            ));
            app.mark_session_dirty();
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
                    loaded,
                    result.disk_state,
                );
                match action.mode {
                    PendingReloadMode::AutoRefreshCleanBuffer => app.set_info_status(format!(
                        "Reloaded {buffer_name} because it changed on disk."
                    )),
                    PendingReloadMode::ExplicitReload => {
                        app.set_info_status(format!("Reloaded {buffer_name} from disk."))
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

        if app.tabs()[tab_index]
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
                    loaded,
                    result.disk_state,
                );
                app.set_info_status(format!("Reopened {buffer_name} with {encoding_label}."));
            }
            Err(error) => {
                diagnostics::record_io_error(
                    "reopen_with_encoding",
                    Some(&action.expected_path),
                    "file_controller::save",
                    &error,
                );
                app.set_error_status(format!(
                    "Reopen with encoding failed for {}: {error}",
                    action.buffer_name
                ));
            }
        }
    }

    fn find_buffer_location(app: &ScratchpadApp, buffer_id: BufferId) -> Option<(usize, PathBuf)> {
        app.tabs().iter().enumerate().find_map(|(tab_index, tab)| {
            tab.buffer_by_id(buffer_id)
                .and_then(|buffer| buffer.path.clone())
                .map(|path| (tab_index, path))
        })
    }

    pub(super) fn handle_refreshed_disk_state(
        app: &mut ScratchpadApp,
        index: usize,
        path: PathBuf,
        disk_state: DiskFileState,
    ) -> bool {
        let (buffer_id, is_dirty, known_disk_state, buffer_name) = {
            let buffer = app.tabs()[index].active_buffer();
            (
                buffer.id,
                buffer.is_dirty,
                buffer.disk_state.clone(),
                buffer.name.clone(),
            )
        };

        if known_disk_state.as_ref() == Some(&disk_state) || known_disk_state.is_none() {
            Self::sync_buffer_disk_state(app, index, Some(disk_state));
            return false;
        }
        if is_dirty {
            let buffer = app.tabs_mut()[index].active_buffer_mut();
            buffer.mark_conflict_on_disk(Some(disk_state));
            app.set_warning_status(format!(
                "{} changed on disk. Your tab has unsaved edits.",
                buffer_name
            ));
            app.mark_session_dirty();
            return true;
        }
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

    pub(super) fn mark_buffer_missing_on_disk(app: &mut ScratchpadApp, index: usize) -> bool {
        let buffer_name = app.tabs()[index].active_buffer().name.clone();
        let buffer = app.tabs_mut()[index].active_buffer_mut();
        buffer.disk_state = None;
        buffer.mark_missing_on_disk();
        app.set_warning_status(format!("{buffer_name} is missing on disk."));
        app.mark_session_dirty();
        true
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
                let buffer = app.tabs_mut()[tab_index]
                    .buffer_by_id_mut(action.buffer_id)
                    .expect("buffer location validated");
                buffer.mark_stale_on_disk(disk_state);
                app.set_warning_status(format!(
                    "Detected a newer on-disk version of {} but could not reload it: {error}",
                    action.buffer_name
                ));
                app.mark_session_dirty();
            }
            PendingReloadMode::ExplicitReload => {
                app.set_error_status(format!("Reload failed: {error}"));
            }
        }
    }
}
