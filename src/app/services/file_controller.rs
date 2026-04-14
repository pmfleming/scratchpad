use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferFreshness, BufferState, WorkspaceTab};
use crate::app::logging::LogLevel;
use crate::app::services::file_service::{FileContent, FileService};
use crate::app::utils::summarize_open_results;
use std::path::{Path, PathBuf};

mod open_here;

pub enum OpenPathOutcome {
    Opened { artifact_warning: Option<String> },
    AlreadyOpen,
    Failed,
}

struct LoadedFile {
    path_display: String,
    encoding: String,
    has_bom: bool,
    artifact_summary: Option<String>,
    artifact_warning: Option<String>,
    buffer: BufferState,
}

impl LoadedFile {
    fn from_file_content(path: PathBuf, file_content: FileContent) -> Self {
        let path_display = path.display().to_string();
        let encoding = file_content.encoding.clone();
        let has_bom = file_content.has_bom;
        let buffer = FileController::buffer_from_file_content(path, file_content);
        let artifact_summary = buffer.artifact_summary.status_text();
        let artifact_warning = artifact_summary
            .as_ref()
            .map(|message| format!("Opened with formatting artifacts: {message}"));

        Self {
            path_display,
            encoding,
            has_bom,
            artifact_summary,
            artifact_warning,
            buffer,
        }
    }

    fn into_parts(self) -> (BufferState, PendingOpenLogEntry) {
        let log_entry = PendingOpenLogEntry {
            path_display: self.path_display,
            encoding: self.encoding,
            has_bom: self.has_bom,
            artifact_summary: self.artifact_summary,
            artifact_warning: self.artifact_warning,
        };
        (self.buffer, log_entry)
    }
}

struct PendingOpenLogEntry {
    path_display: String,
    encoding: String,
    has_bom: bool,
    artifact_summary: Option<String>,
    artifact_warning: Option<String>,
}

pub struct FileController;

impl FileController {
    fn open_path_count(app: &ScratchpadApp, log_prefix: &str, path_count: usize) {
        app.log_event(LogLevel::Info, format!("{log_prefix} {path_count} path(s)"));
    }

    fn prepare_to_open_paths(app: &mut ScratchpadApp) {
        app.reload_settings_before_workspace_change();
    }

    fn handle_open_dialog<F>(app: &mut ScratchpadApp, action_name: &str, open_action: F)
    where
        F: FnOnce(&mut ScratchpadApp, Vec<PathBuf>),
    {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            app.log_event(
                LogLevel::Info,
                format!("{} selected {} path(s)", action_name, paths.len()),
            );
            open_action(app, paths);
        } else {
            app.log_event(LogLevel::Info, format!("{} cancelled", action_name));
        }
    }

    fn handle_external_paths<F>(
        app: &mut ScratchpadApp,
        paths: Vec<PathBuf>,
        log_prefix: &str,
        open_action: F,
    ) where
        F: FnOnce(&mut ScratchpadApp, Vec<PathBuf>),
    {
        if paths.is_empty() {
            return;
        }

        Self::open_path_count(app, log_prefix, paths.len());
        open_action(app, paths);
    }

    pub fn open_file(app: &mut ScratchpadApp) {
        Self::handle_open_dialog(app, "Open file dialog", Self::open_selected_paths);
    }

    pub fn open_file_here(app: &mut ScratchpadApp) {
        Self::handle_open_dialog(app, "Open Here dialog", Self::open_selected_paths_here);
    }

    pub fn open_paths(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::handle_external_paths(app, paths, "Open requested for", Self::open_selected_paths);
    }

    pub fn open_external_paths(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::handle_external_paths(
            app,
            paths,
            "Startup open requested for",
            Self::open_selected_paths,
        );
    }

    pub fn open_external_paths_here(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::handle_external_paths(
            app,
            paths,
            "Startup workspace-open requested for",
            Self::open_selected_paths_here,
        );
    }

    pub fn open_external_paths_into_tab(
        app: &mut ScratchpadApp,
        target_index: usize,
        paths: Vec<PathBuf>,
    ) {
        if paths.is_empty() {
            return;
        }

        if target_index >= app.tabs().len() {
            app.log_event(
                LogLevel::Error,
                format!(
                    "Startup add-to target index {} is out of range (tab count={}).",
                    target_index + 1,
                    app.tabs().len()
                ),
            );
            app.set_error_status(format!(
                "Startup /addto:index:{} target does not exist.",
                target_index + 1
            ));
            return;
        }

        app.handle_command(AppCommand::ActivateTab {
            index: target_index,
        });
        Self::open_external_paths_here(app, paths);
    }

    pub fn save_file(app: &mut ScratchpadApp) {
        let index = app.active_tab_index();
        let _ = Self::save_file_at(app, index);
    }

    pub fn save_file_at(app: &mut ScratchpadApp, index: usize) -> bool {
        if app.tabs().is_empty() {
            return false;
        }

        let _ = Self::refresh_buffer_disk_state(app, index);

        if Self::has_existing_save_path(app, index) {
            Self::save_existing_path(app, index)
        } else {
            Self::save_file_as_at(app, index)
        }
    }

    pub fn save_file_as(app: &mut ScratchpadApp) {
        let index = app.active_tab_index();
        let _ = Self::save_file_as_at(app, index);
    }

    pub fn save_file_as_at(app: &mut ScratchpadApp, index: usize) -> bool {
        if app.tabs().is_empty() {
            return false;
        }

        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&app.tabs()[index].active_buffer().name)
            .save_file()
        {
            app.log_event(
                LogLevel::Info,
                format!(
                    "Save As selected destination for tab index {index}: {}",
                    path.display()
                ),
            );
            Self::save_buffer_to_path(app, index, path, true)
        } else {
            app.set_info_status("Save cancelled.");
            false
        }
    }

    pub(crate) fn refresh_active_buffer_disk_state(app: &mut ScratchpadApp) -> bool {
        let index = app.active_tab_index();
        Self::refresh_buffer_disk_state(app, index)
    }

    pub(crate) fn reload_buffer_from_disk(app: &mut ScratchpadApp, index: usize) -> bool {
        if index >= app.tabs().len() {
            return false;
        }

        let Some(path) = app.tabs()[index].active_buffer().path.clone() else {
            return false;
        };

        match FileService::read_file(&path) {
            Ok(file_content) => {
                let disk_state = FileService::read_disk_state(&path).ok();
                let buffer_name = {
                    let buffer = app.tabs_mut()[index].active_buffer_mut();
                    buffer.replace_text(file_content.content);
                    buffer.encoding = file_content.encoding;
                    buffer.has_bom = file_content.has_bom;
                    buffer.is_dirty = false;
                    buffer.sync_to_disk_state(disk_state);
                    buffer.name.clone()
                };
                app.mark_session_dirty();
                app.set_info_status(format!("Reloaded {buffer_name} because it changed on disk."));
                true
            }
            Err(error) => {
                app.set_error_status(format!("Reload failed: {error}"));
                false
            }
        }
    }

    pub(crate) fn save_conflict_overwrite(app: &mut ScratchpadApp, index: usize) -> bool {
        if index >= app.tabs().len() || !Self::has_existing_save_path(app, index) {
            return false;
        }

        let path = app.tabs()[index].active_buffer().path.clone().unwrap();
        Self::save_buffer_to_path(app, index, path, false)
    }

    fn open_selected_paths(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::prepare_to_open_paths(app);
        let snapshot = app.capture_transaction_snapshot();
        let affected_items = paths
            .iter()
            .map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            })
            .collect::<Vec<_>>();
        let summary = paths
            .into_iter()
            .fold(OpenBatchSummary::default(), |summary, path| {
                summary.record(Self::open_path(app, path))
            });

        if summary.opened_count > 0 {
            let title = if summary.opened_count == 1 {
                "Open file"
            } else {
                "Open files"
            };
            app.record_transaction(title, affected_items, None, snapshot);
        }

        Self::apply_open_summary(app, summary);
    }

    fn open_path(app: &mut ScratchpadApp, path: PathBuf) -> OpenPathOutcome {
        if Self::activate_existing_path(app, &path).is_some() {
            app.log_event(
                LogLevel::Info,
                format!(
                    "File already open, activating existing tab: {}",
                    path.display()
                ),
            );
            return OpenPathOutcome::AlreadyOpen;
        }

        match FileService::read_file(&path) {
            Ok(file_content) => OpenPathOutcome::Opened {
                artifact_warning: Self::open_loaded_file(app, path, file_content),
            },
            Err(error) => {
                app.log_event(
                    LogLevel::Error,
                    format!("Failed to open file {}: {error}", path.display()),
                );
                OpenPathOutcome::Failed
            }
        }
    }

    fn activate_existing_path(app: &mut ScratchpadApp, path: &Path) -> Option<String> {
        if let Some((index, view_id)) = app.find_tab_by_path(path) {
            app.handle_command(AppCommand::ActivateTab { index });
            app.handle_command(AppCommand::ActivateView { view_id });
            if app.is_settings_file_path(path) {
                app.mark_active_buffer_as_settings_file();
            }
            Some(
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
            )
        } else {
            None
        }
    }

    fn open_loaded_file(
        app: &mut ScratchpadApp,
        path: PathBuf,
        file_content: FileContent,
    ) -> Option<String> {
        let LoadedFile {
            path_display,
            encoding,
            has_bom,
            artifact_summary,
            artifact_warning,
            mut buffer,
        } = LoadedFile::from_file_content(path, file_content);
        Self::mark_settings_buffer(app, &mut buffer);
        app.tab_manager_mut().append_tab(WorkspaceTab::new(buffer));
        app.request_focus_for_active_view();
        let tab_index = app.active_tab_index();
        let tab_description = app.describe_active_tab();
        app.log_event(
            LogLevel::Info,
            format!(
                "Opened file into tab index {tab_index}: {tab_description} [encoding={}, bom={}, artifact_status={}] from {}",
                encoding,
                has_bom,
                artifact_summary.unwrap_or_else(|| "none".to_owned()),
                path_display
            ),
        );
        let _ = app.persist_session_now();
        artifact_warning
    }

    fn refresh_buffer_disk_state(app: &mut ScratchpadApp, index: usize) -> bool {
        if index >= app.tabs().len() {
            return false;
        }

        let Some(path) = app.tabs()[index].active_buffer().path.clone() else {
            return false;
        };

        match FileService::read_disk_state(&path) {
            Ok(disk_state) => {
                let (is_dirty, known_disk_state, buffer_name) = {
                    let buffer = app.tabs()[index].active_buffer();
                    (
                        buffer.is_dirty,
                        buffer.disk_state.clone(),
                        buffer.name.clone(),
                    )
                };

                if known_disk_state.as_ref() == Some(&disk_state) {
                    let buffer = app.tabs_mut()[index].active_buffer_mut();
                    buffer.sync_to_disk_state(Some(disk_state));
                    return false;
                }

                if known_disk_state.is_none() {
                    let buffer = app.tabs_mut()[index].active_buffer_mut();
                    buffer.sync_to_disk_state(Some(disk_state));
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

                match FileService::read_file(&path) {
                    Ok(file_content) => {
                        let buffer = app.tabs_mut()[index].active_buffer_mut();
                        buffer.replace_text(file_content.content);
                        buffer.encoding = file_content.encoding;
                        buffer.has_bom = file_content.has_bom;
                        buffer.is_dirty = false;
                        buffer.sync_to_disk_state(Some(disk_state));
                        app.set_info_status(format!(
                            "Reloaded {} because it changed on disk.",
                            buffer_name
                        ));
                        app.mark_session_dirty();
                        true
                    }
                    Err(error) => {
                        let buffer = app.tabs_mut()[index].active_buffer_mut();
                        buffer.mark_stale_on_disk(Some(disk_state));
                        app.set_warning_status(format!(
                            "Detected a newer on-disk version of {} but could not reload it: {error}",
                            buffer_name
                        ));
                        app.mark_session_dirty();
                        true
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let buffer_name = app.tabs()[index].active_buffer().name.clone();
                let buffer = app.tabs_mut()[index].active_buffer_mut();
                buffer.disk_state = None;
                buffer.mark_missing_on_disk();
                app.set_warning_status(format!("{buffer_name} is missing on disk."));
                app.mark_session_dirty();
                true
            }
            Err(_) => false,
        }
    }

    fn save_existing_path(app: &mut ScratchpadApp, index: usize) -> bool {
        let freshness = app.tabs()[index].active_buffer().freshness;
        if matches!(
            freshness,
            BufferFreshness::ConflictOnDisk | BufferFreshness::MissingOnDisk | BufferFreshness::StaleOnDisk
        ) {
            app.set_pending_action(Some(crate::app::domain::PendingAction::SaveConflict(index)));
            if let Some(message) = app.tabs()[index].active_buffer().disk_status_message() {
                app.set_warning_status(message);
            }
            return false;
        }

        let path = app.tabs()[index].active_buffer().path.clone().unwrap();
        Self::save_buffer_to_path(app, index, path, false)
    }

    fn has_existing_save_path(app: &ScratchpadApp, index: usize) -> bool {
        app.tabs()[index].active_buffer().path.is_some()
    }

    fn save_buffer_to_path(
        app: &mut ScratchpadApp,
        index: usize,
        path: PathBuf,
        update_buffer_path: bool,
    ) -> bool {
        let existing_tab_description = app.describe_tab_at(index);
        let target_path = path.display().to_string();
        let save_result = {
            let buffer = app.tabs()[index].active_buffer();
            FileService::write_file_with_bom(&path, buffer.text(), &buffer.encoding, buffer.has_bom)
        };

        match save_result {
            Ok(()) => {
                Self::finalize_save(app, index, path, update_buffer_path);
                Self::log_save_success(app, index, &existing_tab_description, &target_path);
                true
            }
            Err(error) => {
                app.log_event(
                    LogLevel::Error,
                    format!(
                        "Save failed for tab index {index}: {existing_tab_description} -> {target_path}: {error}"
                    ),
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
    ) {
        let settings_path = app.settings_path().to_path_buf();
        let buffer = app.tabs_mut()[index].active_buffer_mut();
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

    fn assign_saved_path(buffer: &mut BufferState, path: &Path) {
        buffer.path = Some(path.to_path_buf());
        buffer.name = path.file_name().unwrap().to_string_lossy().into_owned();
    }

    fn mark_settings_buffer(app: &ScratchpadApp, buffer: &mut BufferState) {
        buffer.is_settings_file = buffer
            .path
            .as_ref()
            .is_some_and(|path| app.is_settings_file_path(path));
    }

    fn apply_open_summary(app: &mut ScratchpadApp, summary: OpenBatchSummary) {
        Self::apply_open_status(
            app,
            summarize_open_results(
                summary.opened_count,
                summary.duplicate_count,
                summary.failure_count,
                summary.artifact_count,
                summary.last_artifact_warning.clone(),
            ),
            summary.failure_count > 0 || summary.artifact_count > 0,
            summary.log_message(),
        );
    }

    fn apply_open_status(
        app: &mut ScratchpadApp,
        status_message: Option<String>,
        should_warn: bool,
        log_message: String,
    ) {
        match status_message {
            Some(message) if should_warn => app.set_warning_status(message),
            Some(message) => app.set_info_status(message),
            None => app.clear_status_message(),
        }
        app.log_event(LogLevel::Info, log_message);
    }

    fn buffer_from_file_content(path: PathBuf, file_content: FileContent) -> BufferState {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let disk_state = FileService::read_disk_state(&path).ok();
        let mut buffer = BufferState::with_encoding(
            name,
            file_content.content,
            Some(path),
            file_content.encoding,
            file_content.has_bom,
        );
        buffer.artifact_summary = file_content.artifact_summary;
        buffer.sync_to_disk_state(disk_state);
        buffer
    }

    fn log_save_success(
        app: &ScratchpadApp,
        index: usize,
        existing_tab_description: &str,
        target_path: &str,
    ) {
        app.log_event(
            LogLevel::Info,
            format!("Saved tab index {index}: {existing_tab_description} -> {target_path}"),
        );
    }
}

#[derive(Default)]
struct OpenBatchSummary {
    opened_count: usize,
    duplicate_count: usize,
    failure_count: usize,
    artifact_count: usize,
    last_artifact_warning: Option<String>,
}

impl OpenBatchSummary {
    fn record(mut self, outcome: OpenPathOutcome) -> Self {
        match outcome {
            OpenPathOutcome::Opened { artifact_warning } => {
                self.opened_count += 1;
                if let Some(warning) = artifact_warning {
                    self.artifact_count += 1;
                    self.last_artifact_warning = Some(warning);
                }
            }
            OpenPathOutcome::AlreadyOpen => {
                self.duplicate_count += 1;
            }
            OpenPathOutcome::Failed => {
                self.failure_count += 1;
            }
        }

        self
    }

    fn log_message(&self) -> String {
        format!(
            "Open file batch completed: opened={}, duplicates={}, failed={}, artifacts={}",
            self.opened_count, self.duplicate_count, self.failure_count, self.artifact_count
        )
    }
}
