use super::FileController;
use crate::app::CanonicalPathKey;
use crate::app::app_state::{ScratchpadApp, StatusDomain};
use crate::app::commands::{AppCommand, WorkspaceCommand};
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

        let (buffer_id, current_name, current_path, freshness, is_settings_file) = {
            let buffer = app.tab_manager.tabs.as_slice()[index].active_buffer();
            (
                buffer.id,
                buffer.name.clone(),
                buffer.path.clone(),
                buffer.freshness,
                buffer.is_settings_file,
            )
        };

        if current_name == normalized_name {
            crate::app::app_state::workspace::accessors::clear_status_message(app);
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

        let Ok(target_path) =
            target_path_for_rename(app, current_path.as_deref(), &normalized_name)
        else {
            return false;
        };
        if activate_conflicting_rename_target(app, buffer_id, target_path.as_deref())
            || !rename_path_on_disk(app, current_path.as_deref(), target_path.as_deref())
        {
            return false;
        }

        finish_rename(
            app,
            index,
            buffer_id,
            current_name,
            normalized_name,
            target_path,
        );
        true
    }
}

fn target_path_for_rename(
    app: &mut ScratchpadApp,
    current_path: Option<&Path>,
    normalized_name: &str,
) -> Result<Option<PathBuf>, ()> {
    let Some(path) = current_path else {
        return Ok(None);
    };
    renamed_path(path, normalized_name)
        .map(Some)
        .map_err(|error| {
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
        })
}

fn activate_conflicting_rename_target(
    app: &mut ScratchpadApp,
    buffer_id: crate::app::domain::BufferId,
    target_path: Option<&Path>,
) -> bool {
    let Some(target_path) = target_path else {
        return false;
    };
    let target_key = CanonicalPathKey::from_path(target_path);
    let Some((owner_buffer_id, tab_index, view_id)) = app.tab_manager.path_owner(&target_key)
    else {
        return false;
    };
    if owner_buffer_id == buffer_id {
        return false;
    }

    crate::app::commands::handle_command(
        app,
        AppCommand::Workspace(WorkspaceCommand::ActivateTab { index: tab_index }),
    );
    crate::app::commands::handle_command(
        app,
        AppCommand::Workspace(WorkspaceCommand::ActivateView { view_id }),
    );
    app.state
        .status
        .set_warning_status_in_domain(StatusDomain::File, "That file is already open.");
    true
}

fn rename_path_on_disk(
    app: &mut ScratchpadApp,
    current_path: Option<&Path>,
    target_path: Option<&Path>,
) -> bool {
    let (Some(current_path), Some(target_path)) = (current_path, target_path) else {
        return true;
    };
    if current_path == target_path {
        return true;
    }
    if let Err(error) = FileService::rename_path(current_path, target_path) {
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
    true
}

fn finish_rename(
    app: &mut ScratchpadApp,
    index: usize,
    buffer_id: crate::app::domain::BufferId,
    current_name: String,
    normalized_name: String,
    target_path: Option<PathBuf>,
) {
    let settings_path = crate::app::app_state::settings_state::settings_path(app).to_path_buf();
    let old_key = app.tab_manager.tabs.as_slice()[index]
        .active_buffer()
        .path_key
        .clone();
    let new_key = target_path.as_deref().map(CanonicalPathKey::from_path);
    {
        let buffer = app.tab_manager.tabs.as_mut_slice()[index].active_buffer_mut();
        buffer.name.clone_from(&normalized_name);
        if let Some(target_path) = target_path {
            buffer.set_path(Some(target_path.clone()));
            buffer.sync_to_disk_state(FileService::read_disk_state(&target_path).ok());
            buffer.is_settings_file = crate::app::paths_match(&target_path, &settings_path);
        }
    }
    app.tab_manager.rebuild_buffer_tab_index();
    if let Some(old_key) = old_key {
        debug_assert_ne!(
            app.tab_manager.path_owner(&old_key).map(|owner| owner.0),
            Some(buffer_id),
            "old rename path key was not removed"
        );
    }
    if let Some(new_key) = new_key {
        debug_assert_eq!(
            app.tab_manager.path_owner(&new_key).map(|owner| owner.0),
            Some(buffer_id),
            "new rename path key was not inserted exactly once"
        );
    }

    app.state.status.set_info_status_in_domain(
        StatusDomain::File,
        format!("Renamed {current_name} to {normalized_name}."),
    );
    app.tab_manager.mark_session_dirty();
    app.apply_current_tab_ordering();
    let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
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

#[cfg(test)]
mod tests {
    use super::FileController;
    use crate::app::ScratchpadApp;
    use crate::app::domain::{BufferState, TabManager, WorkspaceTab};
    use crate::app::services::file_service::FileService;
    use crate::app::services::session_store::SessionStore;

    #[test]
    fn rename_tab_moves_disk_file_and_updates_buffer_identity() {
        let directory = tempfile::tempdir().unwrap();
        let old_path = directory.path().join("old.txt");
        let new_path = directory.path().join("new.txt");
        std::fs::write(&old_path, "content").unwrap();
        let mut buffer = BufferState::new(
            "old.txt".to_owned(),
            "content".to_owned(),
            Some(old_path.clone()),
        );
        buffer.sync_to_disk_state(FileService::read_disk_state(&old_path).ok());
        let mut app =
            ScratchpadApp::with_session_store(SessionStore::new(directory.path().join("session")));
        app.set_session_persist_on_drop(false);
        app.tab_manager = TabManager::for_test_tabs(vec![WorkspaceTab::new(buffer)]);

        assert!(FileController::rename_tab(&mut app, 0, "new.txt"));

        assert!(!old_path.exists());
        assert_eq!(std::fs::read_to_string(&new_path).unwrap(), "content");
        let buffer = app.tab_manager.tabs.as_slice()[0].active_buffer();
        assert_eq!(buffer.name, "new.txt");
        assert_eq!(buffer.path.as_deref(), Some(new_path.as_path()));
    }
}
