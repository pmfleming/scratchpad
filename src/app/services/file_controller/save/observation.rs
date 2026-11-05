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
        if action.expected_epoch != app.state.persistence.session_store.history_epoch() {
            return;
        }
        let Some((index, result)) = Self::resolve_background_result(app, &action, results) else {
            return;
        };
        let Some(buffer) = app.tab_manager.tabs.as_slice()[index].buffer_by_id(action.buffer_id)
        else {
            return;
        };
        if buffer.observed_disk_digest != action.expected_observed_digest
            || buffer.observed_disk_missing != action.expected_missing
        {
            return;
        }
        // A second change may arrive while the first read is in flight. Retry
        // before applying it (or resuming Save), even if that watcher was coalesced.
        if result.result.is_ok()
            && super::FileService::read_disk_state(&action.expected_path).ok() != result.disk_state
        {
            Self::refresh_buffer_disk_state_by_id(app, action.buffer_id);
            if let Some(save) = action.resume_save {
                app.state
                    .background_io
                    .defer_save_until_observed(action.buffer_id, save);
            }
            return;
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
                        return;
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
                        return;
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
            Self::resume_observed_save(app, index, action);
        }
    }

    fn resume_observed_save(
        app: &mut ScratchpadApp,
        index: usize,
        action: PendingReloadBufferAction,
    ) {
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
            }
        }
    }
}
