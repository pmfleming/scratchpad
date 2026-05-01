use super::FileController;
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::BufferFreshness;
use crate::app::services::file_service::FileService;
use std::io;
use std::path::{Component, Path, PathBuf};

impl FileController {
    pub(crate) fn rename_tab(app: &mut ScratchpadApp, index: usize, requested_name: &str) -> bool {
        if index >= app.tabs().len() {
            return false;
        }

        let normalized_name = match normalize_requested_name(requested_name) {
            Ok(name) => name,
            Err(error) => {
                app.set_warning_status(format!("Rename failed: {error}"));
                return false;
            }
        };

        let (current_name, current_path, freshness, is_settings_file) = {
            let buffer = app.tabs()[index].active_buffer();
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
            app.set_warning_status("Rename is unavailable for the settings file.");
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
            let message = app.tabs()[index]
                .active_buffer()
                .disk_status_message()
                .unwrap_or_else(|| {
                    "Resolve the on-disk state before renaming this file.".to_owned()
                });
            app.set_warning_status(message);
            return false;
        }

        let target_path = match current_path.as_ref() {
            Some(path) => match renamed_path(path, &normalized_name) {
                Ok(path) => Some(path),
                Err(error) => {
                    app.set_error_status(format!("Rename failed: {error}"));
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
            app.set_error_status(format!("Rename failed: {error}"));
            return false;
        }

        let settings_path = app.settings_path().to_path_buf();

        {
            let buffer = app.tabs_mut()[index].active_buffer_mut();
            buffer.name = normalized_name.clone();
            if let Some(target_path) = target_path {
                buffer.path = Some(target_path.clone());
                buffer.sync_to_disk_state(FileService::read_disk_state(&target_path).ok());
                buffer.is_settings_file = crate::app::paths_match(&target_path, &settings_path);
            }
        }

        app.set_info_status(format!("Renamed {current_name} to {normalized_name}."));
        app.mark_session_dirty();
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

#[cfg(test)]
mod tests {
    use super::FileController;
    use crate::app::app_state::ScratchpadApp;
    use crate::app::domain::BufferState;
    use crate::app::services::session_store::SessionStore;
    use std::fs;

    fn test_app() -> ScratchpadApp {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());
        ScratchpadApp::with_session_store(session_store)
    }

    #[test]
    fn renaming_unsaved_tab_adds_txt_extension() {
        let mut app = test_app();
        app.tabs_mut()[0].buffer.name = "Untitled".to_owned();

        assert!(FileController::rename_tab(&mut app, 0, "notes"));

        let buffer = app.tabs()[0].active_buffer();
        assert_eq!(buffer.name, "notes.txt");
        assert!(buffer.path.is_none());
    }

    #[test]
    fn renaming_file_backed_tab_moves_the_underlying_file() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let old_path = temp_dir.path().join("draft.md");
        fs::write(&old_path, "hello").expect("write source file");

        let mut app = test_app();
        app.tabs_mut()[0].buffer = BufferState::new(
            "draft.md".to_owned(),
            "hello".to_owned(),
            Some(old_path.clone()),
        );

        assert!(FileController::rename_tab(&mut app, 0, "renamed"));

        let new_path = temp_dir.path().join("renamed.txt");
        let buffer = app.tabs()[0].active_buffer();
        assert_eq!(buffer.name, "renamed.txt");
        assert_eq!(buffer.path.as_deref(), Some(new_path.as_path()));
        assert!(!old_path.exists());
        assert!(new_path.exists());
        assert_eq!(
            fs::read_to_string(new_path).expect("read renamed file"),
            "hello"
        );
    }
}
