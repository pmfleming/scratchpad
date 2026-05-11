use super::FileController;
use crate::app::app_state::{
    PendingBackgroundAction, PendingReloadBufferAction, PendingReloadMode,
    PendingReopenWithEncodingAction, ScratchpadApp,
};
use crate::app::diagnostics;
use crate::app::domain::{
    BufferFreshness, BufferId, DiskFileState, DocumentSnapshot, EncodingSource, PendingAction,
    TextFormatMetadata,
};
use crate::app::services::background_io::LoadedPathResult;
use crate::app::services::file_service::FileService;
use std::path::{Path, PathBuf};

mod reload;

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
                diagnostics::record_io_error_with_details(
                    "save_with_encoding",
                    Self::buffer_path(app, index).as_deref(),
                    "file_controller::save",
                    &error,
                    [("encoding", encoding_name.to_owned())],
                );
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
        let file_name = default_save_as_file_name(&app.tabs()[index].active_buffer().name);
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt"])
            .set_file_name(&file_name)
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
        let buffer_id = app.tabs()[index].active_buffer().id;
        app.tabs_mut()[index].clear_view_state_for_buffer_replacement(buffer_id);
        let (buffer_name, deferred_refresh) = {
            let buffer = app.tabs_mut()[index].active_buffer_mut();
            buffer.replace_from_loaded_buffer(loaded);
            buffer.is_dirty = false;
            buffer.sync_to_disk_state(disk_state);
            (buffer.name.clone(), Self::deferred_buffer_refresh(buffer))
        };
        app.mark_search_dirty();
        app.mark_session_dirty();
        Self::queue_deferred_buffer_refreshes(app, deferred_refresh);
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
                diagnostics::record_io_error(
                    if update_buffer_path {
                        "save_as"
                    } else {
                        "save_file"
                    },
                    Some(&request.path),
                    "file_controller::save",
                    &error,
                );
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
        app.apply_current_tab_ordering();
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
