use super::FileController;
use crate::app::app_state::{
    PendingBackgroundAction, PendingReloadBufferAction, PendingReloadMode,
    PendingReopenWithEncodingAction, ScratchpadApp,
};
use crate::app::domain::{
    BufferFreshness, BufferId, DiskFileState, DocumentSnapshot, EncodingSource, PendingAction,
    TextFormatMetadata,
};
use crate::app::services::background_io::LoadedPathResult;
use crate::app::services::file_service::FileService;
use std::path::PathBuf;

struct SaveWriteRequest {
    path: PathBuf,
    snapshot: DocumentSnapshot,
    format: TextFormatMetadata,
}

impl FileController {
    pub fn save_file(app: &mut ScratchpadApp) {
        let index = app.active_tab_index();
        let _ = Self::save_file_at(app, index);
    }

    pub fn save_file_at(app: &mut ScratchpadApp, index: usize) -> bool {
        if app.tabs().is_empty() || index >= app.tabs().len() {
            return false;
        }

        let _ = Self::refresh_buffer_disk_state(app, index);

        if let Some(path) = Self::buffer_path(app, index) {
            Self::save_existing_path(app, index, path, None)
        } else {
            Self::save_file_as_at(app, index)
        }
    }

    pub fn save_file_as(app: &mut ScratchpadApp) {
        let index = app.active_tab_index();
        let _ = Self::save_file_as_at(app, index);
    }

    pub fn save_file_with_encoding_at(
        app: &mut ScratchpadApp,
        index: usize,
        encoding_name: &str,
    ) -> bool {
        if app.tabs().is_empty() || index >= app.tabs().len() {
            return false;
        }

        let _ = Self::refresh_buffer_disk_state(app, index);
        let format = match Self::format_with_selected_encoding(app, index, encoding_name) {
            Ok(format) => format,
            Err(error) => {
                app.set_error_status(format!("Save with encoding failed: {error}"));
                return false;
            }
        };

        if let Some(path) = Self::buffer_path(app, index) {
            Self::save_existing_path(app, index, path, Some(format))
        } else {
            Self::save_new_path(app, index, "Save with encoding", Some(format))
        }
    }

    pub fn save_file_as_at(app: &mut ScratchpadApp, index: usize) -> bool {
        if app.tabs().is_empty() || index >= app.tabs().len() {
            return false;
        }

        Self::save_new_path(app, index, "Save As", None)
    }

    pub(crate) fn refresh_active_buffer_disk_state(app: &mut ScratchpadApp) -> bool {
        let index = app.active_tab_index();
        Self::refresh_buffer_disk_state(app, index)
    }

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

    pub(crate) fn save_conflict_overwrite(app: &mut ScratchpadApp, index: usize) -> bool {
        if index >= app.tabs().len() {
            return false;
        }

        let Some(path) = Self::buffer_path(app, index) else {
            return false;
        };

        Self::save_buffer_to_path(app, index, path, false, None)
    }

    fn refresh_buffer_disk_state(app: &mut ScratchpadApp, index: usize) -> bool {
        if index >= app.tabs().len() {
            return false;
        }

        let Some(path) = Self::buffer_path(app, index) else {
            return false;
        };

        match FileService::read_disk_state(&path) {
            Ok(disk_state) => Self::handle_refreshed_disk_state(app, index, path, disk_state),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Self::mark_buffer_missing_on_disk(app, index)
            }
            Err(_) => false,
        }
    }

    fn save_existing_path(
        app: &mut ScratchpadApp,
        index: usize,
        path: PathBuf,
        format_override: Option<TextFormatMetadata>,
    ) -> bool {
        if !Self::can_save_existing_path(app, index) {
            return false;
        }

        Self::save_buffer_to_path(app, index, path, false, format_override)
    }

    fn can_save_existing_path(app: &mut ScratchpadApp, index: usize) -> bool {
        let freshness = app.tabs()[index].active_buffer().freshness;
        if matches!(
            freshness,
            BufferFreshness::ConflictOnDisk
                | BufferFreshness::MissingOnDisk
                | BufferFreshness::StaleOnDisk
        ) {
            let status_message = app.tabs()[index].active_buffer().disk_status_message();
            app.set_pending_action(Some(PendingAction::SaveConflict {
                tab_index: index,
                view_id: app.tabs()[index].active_view_id,
            }));
            if let Some(message) = status_message {
                app.set_warning_status(message);
            }
            return false;
        }

        true
    }

    fn save_new_path(
        app: &mut ScratchpadApp,
        index: usize,
        _action_name: &str,
        format_override: Option<TextFormatMetadata>,
    ) -> bool {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&app.tabs()[index].active_buffer().name)
            .save_file()
        {
            Self::save_buffer_to_path(app, index, path, true, format_override)
        } else {
            app.set_info_status("Save cancelled.");
            false
        }
    }

    fn buffer_path(app: &ScratchpadApp, index: usize) -> Option<PathBuf> {
        app.tabs().get(index)?.active_buffer().path.clone()
    }

    fn sync_buffer_disk_state(
        app: &mut ScratchpadApp,
        index: usize,
        disk_state: Option<DiskFileState>,
    ) {
        let buffer = app.tabs_mut()[index].active_buffer_mut();
        buffer.sync_to_disk_state(disk_state);
    }

    fn replace_buffer_from_loaded_buffer(
        app: &mut ScratchpadApp,
        index: usize,
        loaded: crate::app::domain::BufferState,
        disk_state: Option<DiskFileState>,
    ) -> String {
        let buffer = app.tabs_mut()[index].active_buffer_mut();
        buffer.replace_from_loaded_buffer(loaded);
        buffer.is_dirty = false;
        buffer.sync_to_disk_state(disk_state);
        let buffer_name = buffer.name.clone();
        app.mark_search_dirty();
        app.mark_session_dirty();
        buffer_name
    }

    fn save_buffer_to_path(
        app: &mut ScratchpadApp,
        index: usize,
        path: PathBuf,
        update_buffer_path: bool,
        format_override: Option<TextFormatMetadata>,
    ) -> bool {
        let request = {
            let buffer = app.tabs()[index].active_buffer();
            SaveWriteRequest {
                path: path.clone(),
                snapshot: buffer.document_snapshot(),
                format: format_override
                    .clone()
                    .unwrap_or_else(|| buffer.format.clone()),
            }
        };
        let save_result = FileService::write_snapshot_with_format(
            &request.path,
            &request.snapshot,
            &request.format,
        );

        match save_result {
            Ok(()) => {
                Self::finalize_save(app, index, path, update_buffer_path, format_override);
                true
            }
            Err(error) => {
                app.set_error_status(format!("Save failed: {error}"));
                false
            }
        }
    }

    fn finalize_save(
        app: &mut ScratchpadApp,
        index: usize,
        path: PathBuf,
        update_buffer_path: bool,
        format_override: Option<TextFormatMetadata>,
    ) {
        let settings_path = app.settings_path().to_path_buf();
        let buffer = app.tabs_mut()[index].active_buffer_mut();
        if let Some(format) = format_override {
            buffer.replace_format_without_text_change(format);
        }
        if update_buffer_path {
            Self::assign_saved_path(buffer, &path);
        }
        buffer.is_dirty = false;
        buffer.sync_to_disk_state(FileService::read_disk_state(&path).ok());
        buffer.is_settings_file = buffer
            .path
            .as_ref()
            .is_some_and(|path| crate::app::paths_match(path, &settings_path));
        app.clear_status_message();
        app.mark_session_dirty();
        let _ = app.persist_session_now();
    }

    fn format_with_selected_encoding(
        app: &ScratchpadApp,
        index: usize,
        encoding_name: &str,
    ) -> std::io::Result<TextFormatMetadata> {
        let canonical = FileService::canonical_encoding_name(encoding_name)?;
        let mut format = app.tabs()[index].active_buffer().format.clone();
        format.encoding_name = canonical;
        format.encoding_source = EncodingSource::ExplicitUserChoice;
        if !FileService::encoding_supports_bom(&format.encoding_name)? {
            format.has_bom = false;
        }
        Ok(format)
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

    fn handle_refreshed_disk_state(
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

    fn mark_buffer_missing_on_disk(app: &mut ScratchpadApp, index: usize) -> bool {
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
        expected_path: &PathBuf,
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
