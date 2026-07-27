use super::FileController;
use crate::app::CanonicalPathKey;
use crate::app::app_state::{PendingSavePathAction, ScratchpadApp};
use crate::app::diagnostics;
use crate::app::domain::DiskFileState;
use std::path::{Path, PathBuf};

impl FileController {
    fn finalize_save(
        app: &mut ScratchpadApp,
        index: usize,
        path: PathBuf,
        disk_state: Option<DiskFileState>,
        action: PendingSavePathAction,
    ) {
        let settings_path = crate::app::app_state::settings_state::settings_path(app).to_path_buf();
        let old_key = app.tab_manager.tabs.as_slice()[index]
            .buffer_by_id(action.buffer_id)
            .and_then(|buffer| buffer.path_key.clone());
        let new_key = action
            .update_buffer_path
            .then(|| CanonicalPathKey::from_path(&path));
        {
            let Some(buffer) =
                app.tab_manager.tabs.as_mut_slice()[index].buffer_by_id_mut(action.buffer_id)
            else {
                return;
            };
            if let Some(format) = action.format_override {
                buffer.replace_format_without_text_change(format);
            }
            if action.update_buffer_path {
                Self::assign_saved_path(buffer, &path);
            }
            if buffer.document_revision() == action.saved_revision {
                buffer.is_dirty = false;
            }
            buffer.sync_to_disk_state(disk_state);
            buffer.is_settings_file = buffer
                .path
                .as_ref()
                .is_some_and(|path| crate::app::paths_match(path, &settings_path));
        }
        app.tab_manager.rebuild_buffer_tab_index();
        validate_save_path_index(
            app,
            action.buffer_id,
            old_key,
            new_key,
            action.update_buffer_path,
        );
        crate::app::app_state::workspace::accessors::clear_status_message(app);
        app.tab_manager.mark_session_dirty();
        app.apply_current_tab_ordering();
        let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
    }

    pub(crate) fn apply_async_save_result(
        app: &mut ScratchpadApp,
        action: PendingSavePathAction,
        path: PathBuf,
        disk_state: Option<DiskFileState>,
        result: Result<(), String>,
    ) {
        if !crate::app::paths_match(&path, &action.expected_path) {
            return;
        }

        let Some((index, current_path)) =
            Self::find_buffer_location_for_save(app, action.buffer_id)
        else {
            return;
        };

        if !paths_match_optional(current_path.as_deref(), action.previous_path.as_deref()) {
            return;
        }

        match result {
            Ok(()) => {
                Self::finalize_save(app, index, path, disk_state, action);
            }
            Err(error) => {
                diagnostics::record_background_failure(
                    if action.update_buffer_path {
                        "save_as_result"
                    } else {
                        "save_file_result"
                    },
                    "file_controller::save",
                    &error,
                    [
                        ("path", action.expected_path.display().to_string()),
                        ("buffer", action.buffer_name),
                    ],
                );
                app.state.status.report_save_failed(error);
            }
        }
    }
}

fn validate_save_path_index(
    app: &ScratchpadApp,
    buffer_id: crate::app::domain::BufferId,
    old_key: Option<CanonicalPathKey>,
    new_key: Option<CanonicalPathKey>,
    update_buffer_path: bool,
) {
    if !update_buffer_path {
        return;
    }

    if let Some(old_key) = old_key {
        debug_assert_ne!(
            app.tab_manager.path_owner(&old_key).map(|owner| owner.0),
            Some(buffer_id),
            "old save-as path key was not removed"
        );
    }
    if let Some(new_key) = new_key {
        debug_assert_eq!(
            app.tab_manager.path_owner(&new_key).map(|owner| owner.0),
            Some(buffer_id),
            "new save-as path key was not inserted exactly once"
        );
    }
}

fn paths_match_optional(left: Option<&Path>, right: Option<&Path>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => crate::app::paths_match(left, right),
        (None, None) => true,
        _ => false,
    }
}
