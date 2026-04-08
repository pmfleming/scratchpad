use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::logging::LogLevel;
use crate::app::services::file_service::FileService;
use crate::app::utils::summarize_open_results;
use std::path::{Path, PathBuf};

pub enum OpenPathOutcome {
    Opened { artifact_warning: Option<String> },
    AlreadyOpen,
    Failed,
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

    pub fn save_file(app: &mut ScratchpadApp) {
        let index = app.active_tab_index();
        let _ = Self::save_file_at(app, index);
    }

    pub fn save_file_at(app: &mut ScratchpadApp, index: usize) -> bool {
        if app.tabs().is_empty() {
            return false;
        }

        if app.tabs()[index].buffer.path.is_some() {
            Self::save_existing_path(app, index)
        } else {
            Self::save_file_as_at(app, index);
            !app.tabs()[index].buffer.is_dirty
        }
    }

    pub fn save_file_as(app: &mut ScratchpadApp) {
        let index = app.active_tab_index();
        let _ = Self::save_file_as_at(app, index);
    }

    pub fn save_file_as_at(app: &mut ScratchpadApp, index: usize) -> bool {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&app.tabs()[index].buffer.name)
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
        let mut opened_count = 0usize;
        let mut duplicate_count = 0usize;
        let mut failure_count = 0usize;
        let mut artifact_count = 0usize;
        let mut last_artifact_warning = None;

        for path in paths {
            match Self::open_path(app, path) {
                OpenPathOutcome::Opened { artifact_warning } => {
                    opened_count += 1;
                    if let Some(warning) = artifact_warning {
                        artifact_count += 1;
                        last_artifact_warning = Some(warning);
                    }
                }
                OpenPathOutcome::AlreadyOpen => {
                    duplicate_count += 1;
                }
                OpenPathOutcome::Failed => {
                    failure_count += 1;
                }
            }
        }

        if let Some(message) = summarize_open_results(
            opened_count,
            duplicate_count,
            failure_count,
            artifact_count,
            last_artifact_warning,
        ) {
            if failure_count > 0 || artifact_count > 0 {
                app.set_warning_status(message);
            } else {
                app.set_info_status(message);
            }
        } else {
            app.clear_status_message();
        }

        app.log_event(
            LogLevel::Info,
            format!(
                "Open file batch completed: opened={opened_count}, duplicates={duplicate_count}, failed={failure_count}, artifacts={artifact_count}"
            ),
        );
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
        if let Some(index) = app.find_tab_by_path(path) {
            app.handle_command(crate::app::commands::AppCommand::ActivateTab { index });
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
        let artifact_summary = file_content.artifact_summary.status_text();
        let opened_path = path.display().to_string();
        let encoding = file_content.encoding.clone();
        let has_bom = file_content.has_bom;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut buffer = BufferState::with_encoding(
            name,
            file_content.content,
            Some(path),
            file_content.encoding,
            file_content.has_bom,
        );
        buffer.artifact_summary = file_content.artifact_summary;
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

    fn save_existing_path(app: &mut ScratchpadApp, index: usize) -> bool {
        let path = app.tabs()[index].buffer.path.clone().unwrap();
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
            let buffer = &app.tabs()[index].buffer;
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
                app.log_event(
                    LogLevel::Info,
                    format!("Saved tab index {index}: {existing_tab_description} -> {target_path}"),
                );
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
        let buffer = &mut app.tabs_mut()[index].buffer;
        if update_buffer_path {
            buffer.path = Some(path.clone());
            buffer.name = path.file_name().unwrap().to_string_lossy().into_owned();
        }
        buffer.is_dirty = false;
        app.clear_status_message();
        app.mark_session_dirty();
        let _ = app.persist_session_now();
    }
}
