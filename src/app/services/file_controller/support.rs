use super::FileController;
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::BufferState;
use crate::app::services::file_service::{FileContent, FileService};
use std::path::{Path, PathBuf};

pub(in crate::app::services::file_controller) struct LoadedFile {
    pub(in crate::app::services::file_controller) artifact_warning: Option<String>,
    pub(in crate::app::services::file_controller) buffer: BufferState,
}

impl LoadedFile {
    pub(in crate::app::services::file_controller) fn from_file_content(
        path: PathBuf,
        file_content: FileContent,
    ) -> Self {
        let format_warning = file_content.format.format_warning_text();
        let buffer = FileController::buffer_from_file_content(path, file_content);
        let artifact_summary = buffer.artifact_summary.status_text();
        let artifact_warning =
            combine_open_warning(format_warning.as_deref(), artifact_summary.as_deref());

        Self {
            artifact_warning,
            buffer,
        }
    }

    pub(in crate::app::services::file_controller) fn into_parts(
        self,
    ) -> (BufferState, Option<String>) {
        (self.buffer, self.artifact_warning)
    }
}

fn combine_open_warning(
    format_warning: Option<&str>,
    artifact_summary: Option<&str>,
) -> Option<String> {
    let mut warnings = Vec::new();
    if let Some(format_warning) = format_warning {
        warnings.push(format_warning.to_owned());
    }
    if let Some(artifact_summary) = artifact_summary {
        warnings.push(format!(
            "Opened file with control characters: {artifact_summary}"
        ));
    }

    if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("; "))
    }
}

impl FileController {
    pub(super) fn prepare_to_open_paths(app: &mut ScratchpadApp) {
        app.reload_settings_before_workspace_change();
    }

    pub(super) fn handle_open_dialog<F>(app: &mut ScratchpadApp, action_name: &str, open_action: F)
    where
        F: FnOnce(&mut ScratchpadApp, Vec<PathBuf>),
    {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            open_action(app, paths);
        } else {
            app.set_info_status(format!("{action_name} cancelled."));
        }
    }

    pub(super) fn handle_external_paths<F>(
        app: &mut ScratchpadApp,
        paths: Vec<PathBuf>,
        _log_prefix: &str,
        open_action: F,
    ) where
        F: FnOnce(&mut ScratchpadApp, Vec<PathBuf>),
    {
        if paths.is_empty() {
            return;
        }
        open_action(app, paths);
    }

    pub(super) fn affected_item_labels(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            })
            .collect()
    }

    pub(super) fn apply_open_status(
        app: &mut ScratchpadApp,
        status_message: Option<String>,
        should_warn: bool,
        _log_message: String,
    ) {
        match status_message {
            Some(message) if should_warn => app.set_warning_status(message),
            Some(message) => app.set_info_status(message),
            None => app.clear_status_message(),
        }
    }

    pub(super) fn buffer_from_file_content(
        path: PathBuf,
        file_content: FileContent,
    ) -> BufferState {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let disk_state = FileService::read_disk_state(&path).ok();
        let mut buffer =
            BufferState::with_format(name, file_content.content, Some(path), file_content.format);
        buffer.artifact_summary = file_content.artifact_summary;
        buffer.sync_to_disk_state(disk_state);
        buffer
    }

    pub(super) fn mark_settings_buffer(app: &ScratchpadApp, buffer: &mut BufferState) {
        buffer.is_settings_file = buffer
            .path
            .as_ref()
            .is_some_and(|path| app.is_settings_file_path(path));
    }

    pub(super) fn assign_saved_path(buffer: &mut BufferState, path: &Path) {
        buffer.path = Some(path.to_path_buf());
        buffer.name = path.file_name().unwrap().to_string_lossy().into_owned();
    }
}
