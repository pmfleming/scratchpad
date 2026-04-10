use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferState, WorkspaceTab};
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
        let artifact_warning = buffer
            .artifact_summary
            .status_text()
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

        app.log_event(
            LogLevel::Info,
            format!("{} {} path(s)", log_prefix, paths.len()),
        );
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

        if app.tabs()[index].active_buffer().path.is_some() {
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

    fn open_selected_paths(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        let summary = paths
            .into_iter()
            .fold(OpenBatchSummary::default(), |summary, path| {
                summary.record(Self::open_path(app, path))
            });

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
                path_display
            ),
        );
        let _ = app.persist_session_now();
        artifact_warning
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
        let settings_path = app.settings_path();
        let buffer = app.tabs_mut()[index].active_buffer_mut();
        if update_buffer_path {
            buffer.path = Some(path.clone());
            buffer.name = path.file_name().unwrap().to_string_lossy().into_owned();
        }
        buffer.is_dirty = false;
        buffer.is_settings_file = buffer
            .path
            .as_ref()
            .is_some_and(|path| crate::app::paths_match(path, &settings_path));
        app.clear_status_message();
        app.mark_session_dirty();
        let _ = app.persist_session_now();
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

#[cfg(test)]
mod tests {
    use super::FileController;
    use crate::app::app_state::ScratchpadApp;
    use crate::app::domain::PaneNode;
    use crate::app::services::session_store::SessionStore;
    use crate::app::startup::{StartupOpenTarget, StartupOptions};
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
    fn open_file_flags_settings_toml_buffer() {
        let mut app = test_app();
        app.persist_settings_now().expect("write settings file");
        let settings_path = app.settings_path();

        FileController::open_selected_paths(&mut app, vec![settings_path.clone()]);

        assert_eq!(
            app.tabs()[app.active_tab_index()]
                .active_buffer()
                .path
                .as_deref(),
            Some(settings_path.as_path())
        );
        assert!(
            app.tabs()[app.active_tab_index()]
                .active_buffer()
                .is_settings_file
        );
    }

    #[test]
    fn open_here_flags_settings_toml_buffer() {
        let mut app = test_app();
        app.persist_settings_now().expect("write settings file");
        let settings_path = app.settings_path();

        FileController::open_selected_paths_here(&mut app, vec![settings_path.clone()]);

        assert_eq!(
            app.tabs()[app.active_tab_index()]
                .active_buffer()
                .path
                .as_deref(),
            Some(settings_path.as_path())
        );
        assert!(
            app.tabs()[app.active_tab_index()]
                .active_buffer()
                .is_settings_file
        );
    }

    #[test]
    fn open_file_from_dirty_settings_tab_refreshes_settings_on_focus_loss() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let path = temp_dir.path().join("opened.txt");
        fs::write(&path, "alpha\n").expect("write temp file");

        let mut app = test_app();
        app.handle_command(crate::app::commands::AppCommand::OpenSettings);
        app.open_settings_file_tab();
        let settings_tab_index = app.active_tab_index();

        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .content = [
            "font_size = 24.0",
            "word_wrap = false",
            "logging_enabled = false",
            "editor_font = \"roboto\"",
            "settings_tab_open = true",
            "settings_tab_index = 1",
            "",
        ]
        .join("\n");
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .is_dirty = true;
        app.note_settings_toml_edit(settings_tab_index);

        FileController::open_selected_paths(&mut app, vec![path.clone()]);

        assert_eq!(app.font_size(), 24.0);
        assert!(!app.word_wrap());
        assert!(!app.logging_enabled());
        assert_eq!(
            app.editor_font(),
            crate::app::fonts::EditorFontPreset::Roboto
        );
        assert_eq!(
            app.tabs()[app.active_tab_index()]
                .active_buffer()
                .path
                .as_deref(),
            Some(path.as_path())
        );
    }

    #[test]
    fn open_here_from_dirty_settings_tab_refreshes_settings_on_focus_loss() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let path = temp_dir.path().join("opened-here.txt");
        fs::write(&path, "beta\n").expect("write temp file");

        let mut app = test_app();
        app.handle_command(crate::app::commands::AppCommand::OpenSettings);
        app.open_settings_file_tab();
        let settings_tab_index = app.active_tab_index();

        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .content = [
            "font_size = 25.0",
            "word_wrap = false",
            "logging_enabled = false",
            "editor_font = \"roboto\"",
            "settings_tab_open = true",
            "settings_tab_index = 1",
            "",
        ]
        .join("\n");
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .is_dirty = true;
        app.note_settings_toml_edit(settings_tab_index);

        FileController::open_selected_paths_here(&mut app, vec![path.clone()]);

        assert_eq!(app.font_size(), 25.0);
        assert!(!app.word_wrap());
        assert!(!app.logging_enabled());
        assert_eq!(
            app.editor_font(),
            crate::app::fonts::EditorFontPreset::Roboto
        );
        assert_eq!(
            app.tabs()[app.active_tab_index()]
                .active_buffer()
                .path
                .as_deref(),
            Some(path.as_path())
        );
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

    #[test]
    fn startup_clean_launch_skips_restored_session() {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());

        let mut original = ScratchpadApp::with_session_store(session_store);
        original.tabs_mut()[0].buffer.name = "restored.txt".to_owned();
        original.create_untitled_tab();
        original.tabs_mut()[1].buffer.name = "second.txt".to_owned();
        original.persist_session_now().expect("persist session");

        let clean_store = SessionStore::new(session_root.path().to_path_buf());
        let clean_options = StartupOptions {
            restore_session: false,
            ..Default::default()
        };
        let clean = ScratchpadApp::with_session_store_and_startup(clean_store, clean_options);

        assert_eq!(clean.tabs().len(), 1);
        assert_eq!(clean.tabs()[0].buffer.name, "Untitled");
    }

    #[test]
    fn startup_active_target_adds_files_into_current_workspace() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let path = temp_dir.path().join("startup-here.txt");
        fs::write(&path, "hello\nworld\n").expect("write temp file");

        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());
        let options = StartupOptions {
            open_target: StartupOpenTarget::ActiveTab,
            files: vec![path.clone()],
            ..Default::default()
        };
        let app = ScratchpadApp::with_session_store_and_startup(session_store, options);

        assert_eq!(app.tabs().len(), 1);
        let tab = &app.tabs()[app.active_tab_index()];
        assert_eq!(tab.views.len(), 2);
        assert_eq!(tab.active_buffer().path.as_deref(), Some(path.as_path()));
    }
}
