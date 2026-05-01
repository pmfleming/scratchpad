use super::FileController;
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::BufferState;
use std::path::{Path, PathBuf};

pub(in crate::app::services::file_controller) struct DeferredBufferRefresh {
    pub(in crate::app::services::file_controller) buffer_id: u64,
    pub(in crate::app::services::file_controller) revision: u64,
    pub(in crate::app::services::file_controller) snapshot: crate::app::domain::DocumentSnapshot,
    pub(in crate::app::services::file_controller) format: crate::app::domain::TextFormatMetadata,
}

pub(in crate::app::services::file_controller) struct LoadedFile {
    pub(in crate::app::services::file_controller) artifact_warning: Option<String>,
    pub(in crate::app::services::file_controller) buffer: BufferState,
}

impl LoadedFile {
    pub(in crate::app::services::file_controller) fn from_buffer(buffer: BufferState) -> Self {
        let format_warning = buffer.format.format_warning_text();
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

    pub(super) fn deferred_buffer_refresh(buffer: &BufferState) -> Option<DeferredBufferRefresh> {
        buffer
            .text_metadata_refresh_needed()
            .then(|| DeferredBufferRefresh {
                buffer_id: buffer.id,
                revision: buffer.document_revision(),
                snapshot: buffer.document_snapshot(),
                format: buffer.format.clone(),
            })
    }

    pub(super) fn queue_deferred_buffer_refreshes(
        app: &mut ScratchpadApp,
        refreshes: impl IntoIterator<Item = DeferredBufferRefresh>,
    ) {
        for refresh in refreshes {
            app.queue_background_text_metadata_refresh(
                refresh.buffer_id,
                refresh.revision,
                refresh.snapshot,
                refresh.format,
            );
        }
    }
}
