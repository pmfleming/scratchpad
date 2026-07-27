use super::FileController;
use crate::app::CanonicalPathKey;
use crate::app::app_state::{OpenFileDialogState, ScratchpadApp, StatusDomain};
use crate::app::domain::BufferState;
use crate::app::platform_file::{self, OpenFileDialogKind};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

const OPEN_FILE_DIALOG_POLL_INTERVAL: Duration = Duration::from_millis(100);

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

pub(in crate::app::services::file_controller) enum OpenPathDecision<T> {
    Resolved(T),
    Unresolved(PathBuf),
}

pub(in crate::app::services::file_controller) struct PreparedOpenBatch<T> {
    pub pending_paths: Vec<PathBuf>,
    pub resolved: Vec<T>,
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

    pub(super) fn prepare_open_batch<T>(
        app: &mut ScratchpadApp,
        paths: Vec<PathBuf>,
        mut resolve_existing: impl FnMut(&mut ScratchpadApp, PathBuf) -> OpenPathDecision<T>,
        mut pending_conflict: impl FnMut() -> T,
    ) -> PreparedOpenBatch<T> {
        let mut batch = PreparedOpenBatch {
            pending_paths: Vec::new(),
            resolved: Vec::new(),
        };
        for path in paths {
            match resolve_existing(app, path) {
                OpenPathDecision::Resolved(outcome) => batch.resolved.push(outcome),
                OpenPathDecision::Unresolved(path) => {
                    if Self::reserve_pending_open_path(app, &path) {
                        batch.pending_paths.push(path);
                    } else {
                        batch.resolved.push(pending_conflict());
                    }
                }
            }
        }
        batch
    }

    pub(super) fn reserve_pending_open_path(app: &mut ScratchpadApp, path: &Path) -> bool {
        let key = CanonicalPathKey::from_path(path);
        if Self::is_open_or_pending_path(app, &key) {
            return false;
        }

        app.state.pending_open_file_paths.insert(key)
    }

    pub(super) fn release_pending_open_path(app: &mut ScratchpadApp, path: &Path) {
        let key = CanonicalPathKey::from_path(path);
        app.state.pending_open_file_paths.remove(&key);
    }

    fn is_open_or_pending_path(app: &ScratchpadApp, key: &CanonicalPathKey) -> bool {
        app.tab_manager.find_tab_by_path_key(key).is_some()
            || app.state.pending_open_file_paths.contains(key)
    }

    pub(super) fn handle_open_dialog(app: &mut ScratchpadApp, dialog_kind: OpenFileDialogKind) {
        if app.state.pending_open_file_dialog.is_some() {
            app.state
                .status
                .set_info_status_in_domain(StatusDomain::File, "Open file dialog is already open.");
            return;
        }

        match platform_file::spawn_pick_open_files(dialog_kind) {
            Ok(rx) => {
                app.state.pending_open_file_dialog = Some(OpenFileDialogState {
                    kind: dialog_kind,
                    rx,
                });
                app.state.status.set_info_status_in_domain(
                    StatusDomain::File,
                    format!("{} opened.", dialog_kind.action_name()),
                );
            }
            Err(error) => app.state.status.set_error_status_with_detail(
                StatusDomain::File,
                format!(
                    "Could not open {}.",
                    dialog_kind.action_name().to_ascii_lowercase()
                ),
                error.to_string(),
            ),
        }
    }

    pub(crate) fn poll_open_file_dialog(app: &mut ScratchpadApp, ctx: &egui::Context) {
        let result = match app
            .state
            .pending_open_file_dialog
            .as_ref()
            .map(|pending| pending.rx.try_recv())
        {
            Some(Ok(paths)) => paths,
            Some(Err(TryRecvError::Disconnected)) => None,
            Some(Err(TryRecvError::Empty)) => {
                ctx.request_repaint_after(OPEN_FILE_DIALOG_POLL_INTERVAL);
                return;
            }
            None => return,
        };

        let Some(pending) = app.state.pending_open_file_dialog.take() else {
            return;
        };

        if let Some(paths) = result {
            match pending.kind {
                OpenFileDialogKind::OpenFile => Self::open_selected_paths_async(app, paths),
                OpenFileDialogKind::OpenFileHere => {
                    Self::open_selected_paths_here_async(app, paths);
                }
            }
        } else {
            app.state.status.set_info_status_in_domain(
                StatusDomain::File,
                format!("{} cancelled.", pending.kind.action_name()),
            );
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
            Some(message) if should_warn => app
                .state
                .status
                .set_warning_status_in_domain(StatusDomain::File, message),
            Some(message) => app
                .state
                .status
                .set_info_status_in_domain(StatusDomain::File, message),
            None => crate::app::app_state::workspace::accessors::clear_status_message(app),
        }
    }

    pub(super) fn mark_settings_buffer(app: &ScratchpadApp, buffer: &mut BufferState) {
        buffer.is_settings_file = buffer.path.as_ref().is_some_and(|path| {
            crate::app::app_state::settings_state::is_settings_file_path(app, path)
        });
    }

    pub(super) fn assign_saved_path(buffer: &mut BufferState, path: &Path) {
        buffer.set_path(Some(path.to_path_buf()));
        buffer.name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned();
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
