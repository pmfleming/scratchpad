use super::FileController;
use crate::app::app_state::{ScratchpadApp, StatusDomain};
use crate::app::diagnostics;
use crate::app::domain::BufferFreshness;
use crate::app::services::file_service::FileService;
use std::io;
use std::path::{Component, Path, PathBuf};

impl FileController {
    pub(crate) fn rename_tab(app: &mut ScratchpadApp, index: usize, requested_name: &str) -> bool {
        if index >= app.tab_manager.tabs.as_slice().len() {
            return false;
        }

        let normalized_name = match normalize_requested_name(requested_name) {
            Ok(name) => name,
            Err(error) => {
                diagnostics::record_warning(
                    "rename_validate_name",
                    None,
                    "file_controller::rename",
                    error.to_string(),
                );
                app.state.status.set_warning_status_with_detail(
                    StatusDomain::File,
                    "Could not rename this file.",
                    error.to_string(),
                );
                return false;
            }
        };

        let (current_name, current_path, freshness, is_settings_file) = {
            let buffer = app.tab_manager.tabs.as_slice()[index].active_buffer();
            (
                buffer.name.clone(),
                buffer.path.clone(),
                buffer.freshness,
                buffer.is_settings_file,
            )
        };

        if current_name == normalized_name {
            app.clear_status_message();
            return true;
        }

        if is_settings_file {
            app.state.status.set_warning_status_in_domain(
                StatusDomain::Settings,
                "Rename is unavailable for the settings file.",
            );
            return false;
        }

        if current_path.is_some()
            && matches!(
                freshness,
                BufferFreshness::ConflictOnDisk
                    | BufferFreshness::MissingOnDisk
                    | BufferFreshness::StaleOnDisk
            )
        {
            let message = app.tab_manager.tabs.as_slice()[index]
                .active_buffer()
                .disk_status_message()
                .unwrap_or_else(|| {
                    "Resolve the on-disk state before renaming this file.".to_owned()
                });
            app.state
                .status
                .set_warning_status_in_domain(StatusDomain::Disk, message);
            return false;
        }

        let target_path = match current_path.as_ref() {
            Some(path) => match renamed_path(path, &normalized_name) {
                Ok(path) => Some(path),
                Err(error) => {
                    diagnostics::record_io_error(
                        "rename_build_target_path",
                        Some(path),
                        "file_controller::rename",
                        &error,
                    );
                    app.state.status.set_error_status_with_detail(
                        StatusDomain::File,
                        "Could not rename this file.",
                        error.to_string(),
                    );
                    return false;
                }
            },
            None => None,
        };

        if let (Some(current_path), Some(target_path)) =
            (current_path.as_ref(), target_path.as_ref())
            && current_path != target_path
            && let Err(error) = FileService::rename_path(current_path, target_path)
        {
            diagnostics::record_io_error_with_details(
                "rename_file",
                Some(current_path),
                "file_controller::rename",
                &error,
                [("target_path", target_path.display().to_string())],
            );
            app.state.status.set_error_status_with_detail(
                StatusDomain::File,
                "Could not rename this file.",
                error.to_string(),
            );
            return false;
        }

        let settings_path = app.settings_path().to_path_buf();

        {
            let buffer = app.tab_manager.tabs.as_mut_slice()[index].active_buffer_mut();
            buffer.name = normalized_name.clone();
            if let Some(target_path) = target_path {
                buffer.path = Some(target_path.clone());
                buffer.sync_to_disk_state(FileService::read_disk_state(&target_path).ok());
                buffer.is_settings_file = crate::app::paths_match(&target_path, &settings_path);
            }
        }

        app.state.status.set_info_status_in_domain(
            StatusDomain::File,
            format!("Renamed {current_name} to {normalized_name}."),
        );
        app.tab_manager.mark_session_dirty();
        app.apply_current_tab_ordering();
        let _ = app.persist_session_now();
        true
    }
}

fn normalize_requested_name(requested_name: &str) -> io::Result<String> {
    let trimmed = requested_name.trim();
    if trimmed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "tab name cannot be empty",
        ));
    }

    let requested_path = Path::new(trimmed);
    if requested_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "enter a file name, not a path",
        ));
    }

    let mut file_name = requested_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid tab name"))?;

    if Path::new(&file_name).extension().is_none() {
        file_name.push_str(".txt");
    }

    Ok(file_name)
}

fn renamed_path(path: &Path, file_name: &str) -> io::Result<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot rename a file without a parent directory",
        )
    })?;
    Ok(parent.join(file_name))
}
