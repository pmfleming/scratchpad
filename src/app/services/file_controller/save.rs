use super::FileController;
use crate::app::CanonicalPathKey;
use crate::app::app_state::{
    PendingBackgroundAction, PendingSavePathAction, ScratchpadApp, StatusDomain,
};
use crate::app::diagnostics;
use crate::app::domain::{
    BufferFreshness, BufferId, DiskFileState, DocumentSnapshot, EncodingSource, PendingAction,
    TextFormatMetadata,
};
use crate::app::services::file_service::FileService;
use std::path::{Path, PathBuf};

mod disk_state;
mod reload;

struct SaveWriteRequest {
    buffer_id: BufferId,
    path: PathBuf,
    previous_path: Option<PathBuf>,
    buffer_name: String,
    revision: u64,
    snapshot: DocumentSnapshot,
    format: TextFormatMetadata,
}

struct ActiveBufferPath {
    buffer_id: BufferId,
    path: PathBuf,
    buffer_name: String,
    disk_state: Option<DiskFileState>,
}

impl FileController {
    pub fn save_file(app: &mut ScratchpadApp) {
        let index = app.tab_manager.active_tab_index;
        let _ = Self::save_file_at(app, index);
    }

    pub fn save_file_at(app: &mut ScratchpadApp, index: usize) -> bool {
        if app.tab_manager.tabs.as_slice().is_empty()
            || index >= app.tab_manager.tabs.as_slice().len()
        {
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
        let index = app.tab_manager.active_tab_index;
        let _ = Self::save_file_as_at(app, index);
    }

    pub fn save_file_with_encoding_at(
        app: &mut ScratchpadApp,
        index: usize,
        encoding_name: &str,
    ) -> bool {
        if app.tab_manager.tabs.as_slice().is_empty()
            || index >= app.tab_manager.tabs.as_slice().len()
        {
            return false;
        }

        let _ = Self::refresh_buffer_disk_state(app, index);
        let format = match Self::format_with_selected_encoding(app, index, encoding_name) {
            Ok(format) => format,
            Err(error) => {
                diagnostics::record_io_error_with_details(
                    "save_with_encoding",
                    Self::buffer_path(app, index).as_deref(),
                    "file_controller::save",
                    &error,
                    [("encoding", encoding_name.to_owned())],
                );
                app.state.status.set_error_status_with_detail(
                    StatusDomain::Encoding,
                    "Could not save this file with that encoding.",
                    error.to_string(),
                );
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
        if app.tab_manager.tabs.as_slice().is_empty()
            || index >= app.tab_manager.tabs.as_slice().len()
        {
            return false;
        }

        Self::save_new_path(app, index, "Save As", None)
    }

    pub(crate) fn save_conflict_overwrite(app: &mut ScratchpadApp, index: usize) -> bool {
        let Some(target) = Self::active_buffer_path(app, index) else {
            return false;
        };

        Self::save_buffer_to_path(app, index, target.path, false, None)
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
        let freshness = app.tab_manager.tabs.as_slice()[index]
            .active_buffer()
            .freshness;
        if matches!(
            freshness,
            BufferFreshness::ConflictOnDisk
                | BufferFreshness::MissingOnDisk
                | BufferFreshness::StaleOnDisk
        ) {
            let status_message = app.tab_manager.tabs.as_slice()[index]
                .active_buffer()
                .disk_status_message();
            crate::app::app_state::workspace::accessors::set_pending_action(
                app,
                Some(PendingAction::SaveConflict {
                    tab_index: index,
                    view_id: app.tab_manager.tabs.as_slice()[index].layout.active_view_id,
                }),
            );
            if let Some(message) = status_message {
                app.state
                    .status
                    .set_warning_status_in_domain(StatusDomain::Disk, message);
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
        let file_name =
            default_save_as_file_name(&app.tab_manager.tabs.as_slice()[index].active_buffer().name);
        if let Some(path) = crate::app::platform_file::pick_save_as_path(&file_name) {
            Self::save_buffer_to_path(app, index, path, true, format_override)
        } else {
            app.state
                .status
                .set_info_status_in_domain(StatusDomain::File, "Save cancelled.");
            false
        }
    }

    fn buffer_path(app: &ScratchpadApp, index: usize) -> Option<PathBuf> {
        app.tab_manager
            .tabs
            .as_slice()
            .get(index)?
            .active_buffer()
            .path
            .clone()
    }

    fn active_buffer_path(app: &ScratchpadApp, index: usize) -> Option<ActiveBufferPath> {
        let buffer = app.tab_manager.tabs.as_slice().get(index)?.active_buffer();
        Some(ActiveBufferPath {
            buffer_id: buffer.id,
            path: buffer.path.clone()?,
            buffer_name: buffer.name.clone(),
            disk_state: buffer.disk_state.clone(),
        })
    }

    fn sync_buffer_disk_state(
        app: &mut ScratchpadApp,
        index: usize,
        buffer_id: BufferId,
        disk_state: Option<DiskFileState>,
    ) {
        let buffer = app.tab_manager.tabs.as_mut_slice()[index]
            .buffer_by_id_mut(buffer_id)
            .expect("buffer location validated");
        buffer.sync_to_disk_state(disk_state);
    }

    fn save_buffer_to_path(
        app: &mut ScratchpadApp,
        index: usize,
        path: PathBuf,
        update_buffer_path: bool,
        format_override: Option<TextFormatMetadata>,
    ) -> bool {
        if update_buffer_path && Self::activate_conflicting_open_path(app, index, &path) {
            app.state
                .status
                .set_warning_status_in_domain(StatusDomain::File, "That file is already open.");
            return false;
        }

        let request = {
            let buffer = app.tab_manager.tabs.as_slice()[index].active_buffer();
            SaveWriteRequest {
                buffer_id: buffer.id,
                path: path.clone(),
                previous_path: buffer.path.clone(),
                buffer_name: buffer.name.clone(),
                revision: buffer.document_revision(),
                snapshot: buffer.document_snapshot(),
                format: format_override
                    .clone()
                    .unwrap_or_else(|| buffer.format.clone()),
            }
        };
        app.queue_background_path_save(
            request.path.clone(),
            request.snapshot,
            request.format.clone(),
            PendingSavePathAction {
                buffer_id: request.buffer_id,
                expected_path: request.path,
                previous_path: request.previous_path,
                buffer_name: request.buffer_name,
                saved_revision: request.revision,
                update_buffer_path,
                format_override,
            },
        )
    }

    fn activate_conflicting_open_path(app: &mut ScratchpadApp, index: usize, path: &Path) -> bool {
        let key = CanonicalPathKey::from_path(path);
        let Some((owner_buffer_id, _, _)) = app.tab_manager.path_owner(&key) else {
            return false;
        };
        let active_buffer_id = app.tab_manager.tabs.as_slice()[index].active_buffer().id;
        if owner_buffer_id == active_buffer_id {
            return false;
        }

        Self::activate_existing_path(app, path).is_some()
    }

    fn finalize_save(
        app: &mut ScratchpadApp,
        index: usize,
        path: PathBuf,
        disk_state: Option<DiskFileState>,
        action: PendingSavePathAction,
    ) {
        let settings_path = crate::app::app_state::settings_state::settings_path(app).to_path_buf();
        let old_key = app.tab_manager.tabs.as_slice()[index]
            .buffer_by_id(action.buffer_id)
            .and_then(|buffer| buffer.path_key.clone());
        let new_key = action
            .update_buffer_path
            .then(|| CanonicalPathKey::from_path(&path));
        {
            let buffer = app.tab_manager.tabs.as_mut_slice()[index]
                .buffer_by_id_mut(action.buffer_id)
                .expect("buffer location validated");
            if let Some(format) = action.format_override {
                buffer.replace_format_without_text_change(format);
            }
            if action.update_buffer_path {
                Self::assign_saved_path(buffer, &path);
            }
            if buffer.document_revision() == action.saved_revision {
                buffer.is_dirty = false;
            }
            buffer.sync_to_disk_state(disk_state);
            buffer.is_settings_file = buffer
                .path
                .as_ref()
                .is_some_and(|path| crate::app::paths_match(path, &settings_path));
        }
        app.tab_manager.rebuild_buffer_tab_index();
        if action.update_buffer_path {
            if let Some(old_key) = old_key {
                debug_assert_ne!(
                    app.tab_manager.path_owner(&old_key).map(|owner| owner.0),
                    Some(action.buffer_id),
                    "old save-as path key was not removed"
                );
            }
            if let Some(new_key) = new_key {
                debug_assert_eq!(
                    app.tab_manager.path_owner(&new_key).map(|owner| owner.0),
                    Some(action.buffer_id),
                    "new save-as path key was not inserted exactly once"
                );
            }
        }
        crate::app::app_state::workspace::accessors::clear_status_message(app);
        app.tab_manager.mark_session_dirty();
        app.apply_current_tab_ordering();
        let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
    }

    pub(crate) fn apply_async_save_result(
        app: &mut ScratchpadApp,
        action: PendingSavePathAction,
        path: PathBuf,
        disk_state: Option<DiskFileState>,
        result: Result<(), String>,
    ) {
        if !crate::app::paths_match(&path, &action.expected_path) {
            return;
        }

        let Some((index, current_path)) =
            Self::find_buffer_location_for_save(app, action.buffer_id)
        else {
            return;
        };

        if !paths_match_optional(current_path.as_deref(), action.previous_path.as_deref()) {
            return;
        }

        match result {
            Ok(()) => {
                Self::finalize_save(app, index, path, disk_state, action);
            }
            Err(error) => {
                diagnostics::record_background_failure(
                    if action.update_buffer_path {
                        "save_as_result"
                    } else {
                        "save_file_result"
                    },
                    "file_controller::save",
                    &error,
                    [
                        ("path", action.expected_path.display().to_string()),
                        ("buffer", action.buffer_name),
                    ],
                );
                app.state.status.report_save_failed(error);
            }
        }
    }

    fn format_with_selected_encoding(
        app: &ScratchpadApp,
        index: usize,
        encoding_name: &str,
    ) -> std::io::Result<TextFormatMetadata> {
        let canonical = FileService::canonical_encoding_name(encoding_name)?;
        let mut format = app.tab_manager.tabs.as_slice()[index]
            .active_buffer()
            .format
            .clone();
        format.encoding_name = canonical;
        format.encoding_source = EncodingSource::ExplicitUserChoice;
        if !FileService::encoding_supports_bom(&format.encoding_name)? {
            format.has_bom = false;
        }
        Ok(format)
    }

    fn has_pending_reload_for_buffer(app: &ScratchpadApp, buffer_id: BufferId) -> bool {
        app.state
            .background_io
            .has_pending_reload_for_buffer(buffer_id)
    }

    fn has_pending_reopen_with_encoding_for_buffer(
        app: &ScratchpadApp,
        buffer_id: BufferId,
    ) -> bool {
        app.state
            .background_io
            .has_pending_reopen_with_encoding_for_buffer(buffer_id)
    }

    fn find_buffer_location(
        app: &mut ScratchpadApp,
        buffer_id: BufferId,
    ) -> Option<(usize, PathBuf)> {
        let tab_index =
            crate::app::app_state::workspace::mutation::tab_index_for_buffer(app, buffer_id)?;
        app.tab_manager
            .tabs
            .as_slice()
            .get(tab_index)?
            .buffer_by_id(buffer_id)
            .and_then(|buffer| buffer.path.clone())
            .map(|path| (tab_index, path))
    }

    fn find_buffer_location_for_save(
        app: &mut ScratchpadApp,
        buffer_id: BufferId,
    ) -> Option<(usize, Option<PathBuf>)> {
        let tab_index =
            crate::app::app_state::workspace::mutation::tab_index_for_buffer(app, buffer_id)?;
        app.tab_manager
            .tabs
            .as_slice()
            .get(tab_index)?
            .buffer_by_id(buffer_id)
            .map(|buffer| (tab_index, buffer.path.clone()))
    }
}

fn default_save_as_file_name(buffer_name: &str) -> String {
    let trimmed = buffer_name.trim();
    let mut file_name = if trimmed.is_empty() {
        "untitled".to_owned()
    } else {
        trimmed.to_owned()
    };

    if Path::new(&file_name).extension().is_none() {
        file_name.push_str(".txt");
    }

    file_name
}

fn paths_match_optional(left: Option<&Path>, right: Option<&Path>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => crate::app::paths_match(left, right),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
