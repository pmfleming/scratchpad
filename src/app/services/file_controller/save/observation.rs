use super::{BufferFreshness, FileController, ScratchpadApp, StatusDomain};
use crate::app::{
    app_state::{DeferredSave, PendingReloadBufferAction},
    services::{
        background_io::LoadedPathResult,
        session_store::activity::{ActivityEvent, ActivityKind},
    },
};

impl FileController {
    pub(super) fn defer_save_for_observation(
        app: &mut ScratchpadApp,
        index: usize,
        encoding: Option<String>,
    ) -> bool {
        let id = app.tab_manager.tabs.as_slice()[index].active_buffer().id;
        app.state
            .background_io
            .defer_save_until_observed(id, DeferredSave { encoding })
    }

    pub(super) fn apply_disk_observation(
        app: &mut ScratchpadApp,
        action: PendingReloadBufferAction,
        results: Vec<LoadedPathResult>,
    ) {
        let deferred = action.resume_save.is_some();
        let name = action.buffer_name.clone();
        if !Self::apply_disk_observation_inner(app, action, results) && deferred {
            app.state.status.set_warning_status_in_domain(
                StatusDomain::File,
                format!("Save of {name} did not complete because its disk check failed or was superseded. Review the file and save again."),
            );
        }
    }

    fn retry_observed_save(app: &mut ScratchpadApp, action: &PendingReloadBufferAction) -> bool {
        let Some(save) = action.resume_save.clone() else {
            return false;
        };
        let Some(buffer) = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .find_map(|tab| tab.buffer_by_id(action.buffer_id))
        else {
            return false;
        };
        if !buffer
            .path
            .as_deref()
            .is_some_and(|path| crate::app::paths_match(path, &action.expected_path))
        {
            return false;
        }
        if app
            .state
            .background_io
            .defer_save_until_observed(action.buffer_id, save.clone())
        {
            return true;
        }
        let Some(mut retry) = PendingReloadBufferAction::new(
            buffer,
            crate::app::app_state::PendingReloadMode::ObserveDiskChange,
        ) else {
            return false;
        };
        retry.resume_save = Some(save);
        app.queue_background_path_loads(
            vec![retry.expected_path.clone()],
            crate::app::app_state::PendingBackgroundAction::ReloadBuffer(retry),
        );
        true
    }

    fn apply_disk_observation_inner(
        app: &mut ScratchpadApp,
        action: PendingReloadBufferAction,
        results: Vec<LoadedPathResult>,
    ) -> bool {
        if action.expected_epoch != app.state.persistence.session_store.history_epoch() {
            return Self::retry_observed_save(app, &action);
        }
        let Some((index, result)) = Self::resolve_background_result(app, &action, results) else {
            return Self::retry_observed_save(app, &action);
        };
        let Some(buffer) = app.tab_manager.tabs.as_slice()[index].buffer_by_id(action.buffer_id)
        else {
            return false;
        };
        if buffer.observed_disk_digest != action.expected_observed_digest
            || buffer.observed_disk_missing != action.expected_missing
        {
            return Self::retry_observed_save(app, &action);
        }
        // A second change may arrive while the first read is in flight. Retry
        // before applying it (or resuming Save), even if that watcher was coalesced.
        if result.result.is_ok()
            && super::FileService::read_disk_state(&action.expected_path).ok() != result.disk_state
        {
            if action.resume_save.is_some() {
                return Self::retry_observed_save(app, &action);
            }
            Self::refresh_buffer_disk_state_by_id(app, action.buffer_id);
            return true;
        }
        let valid = match result.result {
            Ok(loaded) => {
                let digest = loaded.observed_disk_digest.clone();
                let changed = digest != buffer.observed_disk_digest;
                let keep_local =
                    buffer.is_dirty || buffer.document().revision_key() != action.expected_revision;
                if changed && !keep_local {
                    Self::replace_buffer_from_loaded_buffer(
                        app,
                        index,
                        action.buffer_id,
                        loaded,
                        result.disk_state,
                        true,
                    )
                    .is_some()
                } else {
                    let Some(buffer) = app.tab_manager.tabs.as_mut_slice()[index]
                        .buffer_by_id_mut(action.buffer_id)
                    else {
                        return false;
                    };
                    buffer.disk_state = result.disk_state;
                    buffer.observed_disk_digest = digest;
                    buffer.observed_disk_missing = false;
                    if changed {
                        buffer.mark_conflict_on_disk(buffer.disk_state.clone());
                        app.state
                            .persistence
                            .pending_activity
                            .push(ActivityEvent::new(ActivityKind::DiskConflict, buffer));
                        app.state.status.set_warning_status_in_domain(
                            StatusDomain::Disk,
                            format!("{} changed on disk. Local work kept.", action.buffer_name),
                        );
                    } else if buffer.freshness == BufferFreshness::MissingOnDisk {
                        buffer.freshness =
                            if buffer.disk_content_digest == buffer.observed_disk_digest {
                                BufferFreshness::InSync
                            } else {
                                BufferFreshness::ConflictOnDisk
                            };
                    }
                    app.tab_manager.mark_session_dirty();
                    true
                }
            }
            Err(error) => {
                if result.missing {
                    Self::mark_buffer_missing_on_disk(app, index, action.buffer_id);
                    true
                } else {
                    let Some(buffer) = app.tab_manager.tabs.as_mut_slice()[index]
                        .buffer_by_id_mut(action.buffer_id)
                    else {
                        return false;
                    };
                    buffer.mark_stale_on_disk(result.disk_state);
                    app.state.status.set_warning_status_with_detail(
                        StatusDomain::Disk,
                        format!("Could not read {} after a disk change.", action.buffer_name),
                        error,
                    );
                    app.tab_manager.mark_session_dirty();
                    false
                }
            }
        };
        if valid {
            return Self::resume_observed_save(app, index, action);
        }
        false
    }

    fn resume_observed_save(
        app: &mut ScratchpadApp,
        index: usize,
        action: PendingReloadBufferAction,
    ) -> bool {
        if let Some(save) = action.resume_save {
            let tab = &mut app.tab_manager.tabs.as_mut_slice()[index];
            if let Some(view) = tab
                .layout
                .views()
                .iter()
                .find(|v| v.buffer_id == action.buffer_id)
                .map(|v| v.id)
            {
                tab.activate_view(view);
                if let Some(encoding) = save.encoding {
                    Self::save_file_with_encoding_at(app, index, &encoding);
                } else {
                    Self::save_file_at(app, index);
                }
                return true;
            }
            return false;
        }
        true
    }
}
