use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferState, ViewId, WorkspaceTab};
use crate::app::logging::LogLevel;
use crate::app::services::file_service::FileService;
use crate::app::utils::{file_count_label, summarize_open_results};
use std::path::{Path, PathBuf};

pub enum OpenPathOutcome {
    Opened { artifact_warning: Option<String> },
    AlreadyOpen,
    Failed,
}

struct PendingOpenHereFile {
    path_display: String,
    encoding: String,
    has_bom: bool,
    artifact_summary: Option<String>,
    artifact_warning: Option<String>,
    buffer: BufferState,
}

struct PendingOpenLogEntry {
    path_display: String,
    encoding: String,
    has_bom: bool,
    artifact_summary: Option<String>,
    artifact_warning: Option<String>,
}

enum OpenHerePathOutcome {
    Opened { artifact_warning: Option<String> },
    Migrated,
    AlreadyInCurrentTab,
    Queued,
    Failed,
}

enum ExistingOpenHerePath {
    AlreadyInCurrentTab { view_id: ViewId },
    NeedsMigration { source_index: usize },
}

pub struct FileController;

impl FileController {
    pub fn open_file(app: &mut ScratchpadApp) {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            app.log_event(
                LogLevel::Info,
                format!("Open file dialog selected {} path(s)", paths.len()),
            );
            Self::open_selected_paths(app, paths);
        } else {
            app.log_event(LogLevel::Info, "Open file dialog cancelled");
        }
    }

    pub fn open_file_here(app: &mut ScratchpadApp) {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            app.log_event(
                LogLevel::Info,
                format!("Open Here selected {} path(s)", paths.len()),
            );
            Self::open_selected_paths_here(app, paths);
        } else {
            app.log_event(LogLevel::Info, "Open Here dialog cancelled");
        }
    }

    pub fn save_file(app: &mut ScratchpadApp) {
        let index = app.active_tab_index();
        let _ = Self::save_file_at(app, index);
    }

    pub fn save_file_at(app: &mut ScratchpadApp, index: usize) -> bool {
        if app.tabs().is_empty() {
            return false;
        }

        if app.tabs()[index].active_buffer().path.is_some() {
            Self::save_existing_path(app, index)
        } else {
            Self::save_file_as_at(app, index);
            !app.tabs()[index].active_buffer().is_dirty
        }
    }

    pub fn save_file_as(app: &mut ScratchpadApp) {
        let index = app.active_tab_index();
        let _ = Self::save_file_as_at(app, index);
    }

    pub fn save_file_as_at(app: &mut ScratchpadApp, index: usize) -> bool {
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

    fn open_selected_paths(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        let summary = paths
            .into_iter()
            .fold(OpenBatchSummary::default(), |summary, path| {
                summary.record(Self::open_path(app, path))
            });

        Self::apply_open_summary(app, summary);
    }

    fn open_selected_paths_here(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        let anchor_view_id = app
            .tabs()
            .get(app.active_tab_index())
            .map(|tab| tab.active_view_id);
        let mut pending_files = Vec::new();
        let summary = paths
            .into_iter()
            .fold(OpenHereBatchSummary::default(), |summary, path| {
                summary.record(Self::prepare_open_path_here(app, path, &mut pending_files))
            });

        let summary = Self::open_pending_files_here(app, anchor_view_id, pending_files)
            .into_iter()
            .fold(summary, |summary, outcome| summary.record(outcome));

        if summary.opened_count > 0 || summary.migrated_count > 0 {
            Self::rebalance_open_here_layout(app);
        }

        Self::apply_open_here_summary(app, summary);
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

    fn prepare_open_path_here(
        app: &mut ScratchpadApp,
        path: PathBuf,
        pending_files: &mut Vec<PendingOpenHereFile>,
    ) -> OpenHerePathOutcome {
        if let Some(existing_path) = Self::find_existing_open_here_path(app, &path) {
            return Self::resolve_existing_open_here_path(app, path, existing_path);
        }

        Self::queue_open_here_path(app, path, pending_files)
    }

    fn open_pending_files_here(
        app: &mut ScratchpadApp,
        anchor_view_id: Option<ViewId>,
        pending_files: Vec<PendingOpenHereFile>,
    ) -> Vec<OpenHerePathOutcome> {
        let Some((pending_workspace, log_entries)) =
            Self::build_pending_open_here_workspace(app, pending_files)
        else {
            return Vec::new();
        };

        if !Self::attach_open_here_workspace(app, anchor_view_id, pending_workspace) {
            return Self::failed_open_here_outcomes(log_entries.len());
        }

        Self::log_open_here_success(app, log_entries)
    }

    fn rebalance_open_here_layout(app: &mut ScratchpadApp) {
        let rebalanced = if let Some(tab) = app.active_tab_mut() {
            tab.rebalance_views_equally()
        } else {
            false
        };

        if !rebalanced {
            app.log_event(
                LogLevel::Error,
                "Open Here could not rebalance the workspace layout equally.",
            );
            return;
        }

        app.mark_session_dirty();
        let _ = app.persist_session_now();
        app.log_event(
            LogLevel::Info,
            format!(
                "Rebalanced Open Here layout in current workspace to equal tile shares (views={}).",
                app.tabs()
                    .get(app.active_tab_index())
                    .map(|tab| tab.views.len())
                    .unwrap_or_default()
            ),
        );
    }

    fn activate_existing_path(app: &mut ScratchpadApp, path: &Path) -> Option<String> {
        if let Some((index, view_id)) = app.find_tab_by_path(path) {
            app.handle_command(AppCommand::ActivateTab { index });
            app.handle_command(AppCommand::ActivateView { view_id });
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
        file_content: crate::app::services::file_service::FileContent,
    ) -> Option<String> {
        let opened_path = path.display().to_string();
        let encoding = file_content.encoding.clone();
        let has_bom = file_content.has_bom;
        let buffer = Self::buffer_from_file_content(path, file_content);
        let artifact_summary = buffer.artifact_summary.status_text();
        let artifact_warning = buffer
            .artifact_summary
            .status_text()
            .map(|message| format!("Opened with formatting artifacts: {message}"));
        app.append_tab(WorkspaceTab::new(buffer));
        let tab_index = app.active_tab_index();
        let tab_description = app.describe_active_tab();
        app.log_event(
            LogLevel::Info,
            format!(
                "Opened file into tab index {tab_index}: {tab_description} [encoding={}, bom={}, artifact_status={}] from {}",
                encoding,
                has_bom,
                artifact_summary.unwrap_or_else(|| "none".to_owned()),
                opened_path
            ),
        );
        let _ = app.persist_session_now();
        artifact_warning
    }

    fn pending_open_here_file(
        path: PathBuf,
        file_content: crate::app::services::file_service::FileContent,
    ) -> PendingOpenHereFile {
        let path_display = path.display().to_string();
        let encoding = file_content.encoding.clone();
        let has_bom = file_content.has_bom;
        let buffer = Self::buffer_from_file_content(path, file_content);
        let artifact_summary = buffer.artifact_summary.status_text();
        let artifact_warning = buffer
            .artifact_summary
            .status_text()
            .map(|message| format!("Opened with formatting artifacts: {message}"));

        PendingOpenHereFile {
            path_display,
            encoding,
            has_bom,
            artifact_summary,
            artifact_warning,
            buffer,
        }
    }

    fn find_existing_open_here_path(
        app: &ScratchpadApp,
        path: &Path,
    ) -> Option<ExistingOpenHerePath> {
        let target_index = app.active_tab_index();
        app.find_tab_by_path(path)
            .map(|(existing_tab_index, view_id)| {
                if existing_tab_index == target_index {
                    ExistingOpenHerePath::AlreadyInCurrentTab { view_id }
                } else {
                    ExistingOpenHerePath::NeedsMigration {
                        source_index: existing_tab_index,
                    }
                }
            })
    }

    fn resolve_existing_open_here_path(
        app: &mut ScratchpadApp,
        path: PathBuf,
        existing_path: ExistingOpenHerePath,
    ) -> OpenHerePathOutcome {
        match existing_path {
            ExistingOpenHerePath::AlreadyInCurrentTab { view_id } => {
                app.handle_command(AppCommand::ActivateView { view_id });
                app.log_event(
                    LogLevel::Info,
                    format!(
                        "Open Here found file already in current workspace, activating view: {}",
                        path.display()
                    ),
                );
                OpenHerePathOutcome::AlreadyInCurrentTab
            }
            ExistingOpenHerePath::NeedsMigration { source_index } => {
                Self::migrate_open_here_path(app, path, source_index)
            }
        }
    }

    fn migrate_open_here_path(
        app: &mut ScratchpadApp,
        path: PathBuf,
        source_index: usize,
    ) -> OpenHerePathOutcome {
        let target_index = app.active_tab_index();
        app.handle_command(AppCommand::CombineTabIntoTab {
            source_index,
            target_index,
        });

        if let Some((current_index, current_view_id)) = app.find_tab_by_path(&path)
            && current_index == app.active_tab_index()
        {
            app.handle_command(AppCommand::ActivateView {
                view_id: current_view_id,
            });
            app.log_event(
                LogLevel::Info,
                format!(
                    "Open Here migrated existing tab into current workspace: {}",
                    path.display()
                ),
            );
            return OpenHerePathOutcome::Migrated;
        }

        app.log_event(
            LogLevel::Error,
            format!(
                "Open Here could not migrate existing tab into current workspace: {}",
                path.display()
            ),
        );
        OpenHerePathOutcome::Failed
    }

    fn queue_open_here_path(
        app: &mut ScratchpadApp,
        path: PathBuf,
        pending_files: &mut Vec<PendingOpenHereFile>,
    ) -> OpenHerePathOutcome {
        match FileService::read_file(&path) {
            Ok(file_content) => {
                pending_files.push(Self::pending_open_here_file(path, file_content));
                OpenHerePathOutcome::Queued
            }
            Err(error) => {
                app.log_event(
                    LogLevel::Error,
                    format!("Open Here failed for {}: {error}", path.display()),
                );
                OpenHerePathOutcome::Failed
            }
        }
    }

    fn build_pending_open_here_workspace(
        app: &mut ScratchpadApp,
        pending_files: Vec<PendingOpenHereFile>,
    ) -> Option<(WorkspaceTab, Vec<PendingOpenLogEntry>)> {
        let mut pending_iter = pending_files.into_iter();
        let first_file = pending_iter.next()?;
        let PendingOpenHereFile {
            path_display,
            encoding,
            has_bom,
            artifact_summary,
            artifact_warning,
            buffer,
        } = first_file;
        let mut pending_workspace = WorkspaceTab::new(buffer);
        let mut log_entries = vec![PendingOpenLogEntry {
            path_display,
            encoding,
            has_bom,
            artifact_summary,
            artifact_warning,
        }];

        for pending_file in pending_iter {
            if !Self::append_pending_file_to_workspace(
                app,
                &mut pending_workspace,
                &mut log_entries,
                pending_file,
            ) {
                return None;
            }
        }

        Some((pending_workspace, log_entries))
    }

    fn append_pending_file_to_workspace(
        app: &mut ScratchpadApp,
        pending_workspace: &mut WorkspaceTab,
        log_entries: &mut Vec<PendingOpenLogEntry>,
        pending_file: PendingOpenHereFile,
    ) -> bool {
        let PendingOpenHereFile {
            path_display,
            encoding,
            has_bom,
            artifact_summary,
            artifact_warning,
            buffer,
        } = pending_file;
        let log_entry = PendingOpenLogEntry {
            path_display,
            encoding,
            has_bom,
            artifact_summary,
            artifact_warning,
        };
        let failed_path = log_entry.path_display.clone();
        log_entries.push(log_entry);

        if pending_workspace
            .open_buffer_with_balanced_layout(buffer)
            .is_some()
        {
            true
        } else {
            app.log_event(
                LogLevel::Error,
                format!("Open Here could not build balanced layout for {failed_path}"),
            );
            app.set_error_status("Open Here failed to create a balanced tile layout.");
            false
        }
    }

    fn attach_open_here_workspace(
        app: &mut ScratchpadApp,
        anchor_view_id: Option<ViewId>,
        pending_workspace: WorkspaceTab,
    ) -> bool {
        let opened = if let Some(tab) = app.active_tab_mut() {
            if let Some(anchor_view_id) = anchor_view_id {
                let _ = tab.activate_view(anchor_view_id);
            }
            tab.combine_with_tab(
                pending_workspace,
                crate::app::domain::SplitAxis::Vertical,
                false,
                0.5,
            )
            .is_some()
        } else {
            false
        };

        if opened {
            true
        } else {
            app.log_event(
                LogLevel::Error,
                "Open Here could not insert the balanced tile layout into the current workspace.",
            );
            app.set_error_status("Open Here failed to create a new tile layout.");
            false
        }
    }

    fn log_open_here_success(
        app: &mut ScratchpadApp,
        log_entries: Vec<PendingOpenLogEntry>,
    ) -> Vec<OpenHerePathOutcome> {
        let tab_index = app.active_tab_index();
        let tab_description = app.describe_active_tab();

        log_entries
            .into_iter()
            .map(|entry| {
                app.log_event(
                    LogLevel::Info,
                    format!(
                        "Opened file into balanced current workspace layout at tab index {tab_index}: {tab_description} [encoding={}, bom={}, artifact_status={}] from {}",
                        entry.encoding,
                        entry.has_bom,
                        entry.artifact_summary.unwrap_or_else(|| "none".to_owned()),
                        entry.path_display
                    ),
                );
                OpenHerePathOutcome::Opened {
                    artifact_warning: entry.artifact_warning,
                }
            })
            .collect()
    }

    fn failed_open_here_outcomes(file_count: usize) -> Vec<OpenHerePathOutcome> {
        (0..file_count).map(|_| OpenHerePathOutcome::Failed).collect()
    }

    fn save_existing_path(app: &mut ScratchpadApp, index: usize) -> bool {
        let path = app.tabs()[index].active_buffer().path.clone().unwrap();
        Self::save_buffer_to_path(app, index, path, false)
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
            FileService::write_file_with_bom(
                &path,
                &buffer.content,
                &buffer.encoding,
                buffer.has_bom,
            )
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
        let buffer = app.tabs_mut()[index].active_buffer_mut();
        if update_buffer_path {
            buffer.path = Some(path.clone());
            buffer.name = path.file_name().unwrap().to_string_lossy().into_owned();
        }
        buffer.is_dirty = false;
        app.clear_status_message();
        app.mark_session_dirty();
        let _ = app.persist_session_now();
    }

    fn apply_open_summary(app: &mut ScratchpadApp, summary: OpenBatchSummary) {
        if let Some(message) = summarize_open_results(
            summary.opened_count,
            summary.duplicate_count,
            summary.failure_count,
            summary.artifact_count,
            summary.last_artifact_warning.clone(),
        ) {
            if summary.failure_count > 0 || summary.artifact_count > 0 {
                app.set_warning_status(message);
            } else {
                app.set_info_status(message);
            }
        } else {
            app.clear_status_message();
        }

        app.log_event(LogLevel::Info, summary.log_message());
    }

    fn apply_open_here_summary(app: &mut ScratchpadApp, summary: OpenHereBatchSummary) {
        if let Some(message) = summary.status_message() {
            if summary.failure_count > 0 || summary.artifact_count > 0 {
                app.set_warning_status(message);
            } else {
                app.set_info_status(message);
            }
        } else {
            app.clear_status_message();
        }

        app.log_event(LogLevel::Info, summary.log_message());
    }

    fn buffer_from_file_content(
        path: PathBuf,
        file_content: crate::app::services::file_service::FileContent,
    ) -> BufferState {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut buffer = BufferState::with_encoding(
            name,
            file_content.content,
            Some(path),
            file_content.encoding,
            file_content.has_bom,
        );
        buffer.artifact_summary = file_content.artifact_summary;
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

#[derive(Default)]
struct OpenHereBatchSummary {
    opened_count: usize,
    migrated_count: usize,
    already_here_count: usize,
    failure_count: usize,
    artifact_count: usize,
    last_artifact_warning: Option<String>,
}

impl OpenHereBatchSummary {
    fn record(mut self, outcome: OpenHerePathOutcome) -> Self {
        match outcome {
            OpenHerePathOutcome::Opened { artifact_warning } => {
                self.opened_count += 1;
                if let Some(warning) = artifact_warning {
                    self.artifact_count += 1;
                    self.last_artifact_warning = Some(warning);
                }
            }
            OpenHerePathOutcome::Migrated => {
                self.migrated_count += 1;
            }
            OpenHerePathOutcome::AlreadyInCurrentTab => {
                self.already_here_count += 1;
            }
            OpenHerePathOutcome::Queued => {}
            OpenHerePathOutcome::Failed => {
                self.failure_count += 1;
            }
        }

        self
    }

    fn status_message(&self) -> Option<String> {
        if self.opened_count == 1
            && self.migrated_count == 0
            && self.already_here_count == 0
            && self.failure_count == 0
        {
            return self
                .last_artifact_warning
                .clone()
                .or_else(|| Some("Opened 1 file in the current tab.".to_owned()));
        }

        let parts = [
            self.opened_message(),
            self.migrated_message(),
            self.already_here_message(),
            self.failure_message(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        (!parts.is_empty()).then(|| parts.join("; "))
    }

    fn log_message(&self) -> String {
        format!(
            "Open Here batch completed: opened={}, migrated={}, already_here={}, failed={}, artifacts={}",
            self.opened_count,
            self.migrated_count,
            self.already_here_count,
            self.failure_count,
            self.artifact_count
        )
    }

    fn opened_message(&self) -> Option<String> {
        if self.opened_count == 0 {
            None
        } else if self.artifact_count > 0 {
            Some(format!(
                "Opened {} here ({} with formatting artifacts)",
                file_count_label(self.opened_count),
                file_count_label(self.artifact_count)
            ))
        } else {
            Some(format!(
                "Opened {} here",
                file_count_label(self.opened_count)
            ))
        }
    }

    fn migrated_message(&self) -> Option<String> {
        (self.migrated_count > 0).then(|| {
            format!(
                "Migrated {} into the current tab",
                file_count_label(self.migrated_count)
            )
        })
    }

    fn already_here_message(&self) -> Option<String> {
        (self.already_here_count > 0).then(|| {
            format!(
                "{} already in the current tab",
                file_count_label(self.already_here_count)
            )
        })
    }

    fn failure_message(&self) -> Option<String> {
        (self.failure_count > 0).then(|| {
            format!(
                "{} failed to open here",
                file_count_label(self.failure_count)
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FileController;
    use crate::app::app_state::ScratchpadApp;
    use crate::app::domain::PaneNode;
    use crate::app::services::session_store::SessionStore;
    use std::fs;

    fn collect_leaf_area_fractions(node: &PaneNode, area_fraction: f32, output: &mut Vec<f32>) {
        match node {
            PaneNode::Leaf { .. } => output.push(area_fraction),
            PaneNode::Split {
                ratio,
                first,
                second,
                ..
            } => {
                collect_leaf_area_fractions(first, area_fraction * ratio, output);
                collect_leaf_area_fractions(second, area_fraction * (1.0 - ratio), output);
            }
        }
    }

    fn test_app() -> ScratchpadApp {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());
        ScratchpadApp::with_session_store(session_store)
    }

    #[test]
    fn open_here_splits_file_into_current_workspace() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let path = temp_dir.path().join("split-target.txt");
        fs::write(&path, "alpha\nbeta\n").expect("write temp file");

        let mut app = test_app();

        FileController::open_selected_paths_here(&mut app, vec![path.clone()]);

        assert_eq!(app.tabs().len(), 1);
        let tab = &app.tabs()[app.active_tab_index()];
        assert_eq!(tab.views.len(), 2);
        assert_eq!(tab.active_buffer().path.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn open_here_migrates_existing_tab_into_current_workspace() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let path = temp_dir.path().join("migrate-me.txt");
        fs::write(&path, "gamma\ndelta\n").expect("write temp file");

        let mut app = test_app();
        FileController::open_selected_paths(&mut app, vec![path.clone()]);
        app.create_untitled_tab();

        FileController::open_selected_paths_here(&mut app, vec![path.clone()]);

        assert_eq!(app.tabs().len(), 2);
        let tab = &app.tabs()[app.active_tab_index()];
        assert_eq!(tab.views.len(), 2);
        assert_eq!(tab.active_buffer().path.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn open_here_batches_multiple_new_files_into_equal_tile_shares() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let first_path = temp_dir.path().join("first.txt");
        let second_path = temp_dir.path().join("second.txt");
        let third_path = temp_dir.path().join("third.txt");
        fs::write(&first_path, "first\n").expect("write first temp file");
        fs::write(&second_path, "second\n").expect("write second temp file");
        fs::write(&third_path, "third\n").expect("write third temp file");

        let mut app = test_app();
        FileController::open_selected_paths_here(
            &mut app,
            vec![first_path.clone(), second_path.clone(), third_path.clone()],
        );

        let tab = &app.tabs()[app.active_tab_index()];
        assert_eq!(tab.views.len(), 4);

        let mut areas = Vec::new();
        collect_leaf_area_fractions(&tab.root_pane, 1.0, &mut areas);
        assert!(areas.iter().all(|area| (area - 0.25).abs() < f32::EPSILON));
    }
}
