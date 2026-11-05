//! Reopening restores one retained document tree, then reconciles observed disk text.
use super::{Restorer, SessionStore, model::SessionTab};
use crate::app::{CanonicalPathKey, domain::WorkspaceTab, services::file_service::FileService};
use futures_lite::future::block_on;
use std::{collections::HashMap, io, path::Path};

#[cfg(test)]
mod tests;

pub(crate) enum RetainedTarget {
    Archive(String),
    Document(String),
    Cold(super::ColdSessionTab),
}

pub(super) fn restore_reopened(
    mut tab: SessionTab,
    contents: HashMap<String, String>,
    claimed: &[CanonicalPathKey],
) -> WorkspaceTab {
    let originals = tab.buffers.clone();
    for buffer in &mut tab.buffers {
        buffer.path = None;
    }
    let mut restored = Restorer::new(contents).restore_tab_with_summary(tab).0;
    for original in originals {
        let Some(buffer) = restored
            .buffers_mut()
            .find(|b| b.temp_id == original.temp_id)
        else {
            continue;
        };
        let Some(path) = original.path else {
            continue;
        };
        let key = CanonicalPathKey::from_path(&path);
        if claimed.contains(&key) {
            buffer.recover_as_untitled("recovered");
            continue;
        }
        match FileService::read_observed_file(&path) {
            Ok(observed) => {
                let digest = observed.digest.clone();
                let baseline = original
                    .observed_disk_digest
                    .as_ref()
                    .or(original.disk_content_digest.as_ref())
                    .cloned()
                    .unwrap_or_else(|| buffer.document().content_digest());
                buffer.set_path(Some(path.clone()));
                buffer
                    .disk_content_digest
                    .clone_from(&original.disk_content_digest);
                buffer.disk_state = Some(observed.state.clone());
                buffer.is_dirty = original.is_dirty;
                if digest != baseline {
                    let original_document = Box::new(buffer.document().clone());
                    let previous = buffer.document().history_now();
                    let loaded = FileService::buffer_from_observed(&path, observed);
                    buffer.replace_from_loaded_buffer_source(
                        loaded,
                        crate::app::domain::buffer::PieceSource::External,
                    );
                    if buffer.document().history_now() != previous {
                        buffer.pending_reopen_external = Some(buffer.document().history_now());
                        buffer.pending_reopen_original = Some(original_document);
                    }
                    buffer.show_control_chars = original.show_control_chars;
                    buffer.right_to_left_reading_order = original.right_to_left_reading_order;
                    buffer.is_settings_file = original.is_settings_file;
                }
                buffer.observed_disk_digest = Some(digest);
                buffer.observed_disk_missing = false;
            }
            Err(_) => buffer.recover_as_untitled("recovered"),
        }
    }
    restored
}

impl SessionStore {
    pub(crate) fn reopen_target(
        &self,
        target: RetainedTarget,
        claimed: &[CanonicalPathKey],
    ) -> io::Result<WorkspaceTab> {
        match target {
            RetainedTarget::Cold(tab) => self.restore_cold_session_tab(tab).map(|r| r.0),
            RetainedTarget::Archive(token) => self.reopen_archive_with_claimed(&token, claimed),
            RetainedTarget::Document(id) => {
                let (tab, contents) = self
                    .worker()?
                    .call(move |db| block_on(db.load_retained_document(&id)))?;
                Ok(restore_reopened(tab, contents, claimed))
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn reopen_archive(&self, token: &str) -> io::Result<WorkspaceTab> {
        self.reopen_archive_with_claimed(token, &[])
    }

    pub(crate) fn reopen_archive_with_claimed(
        &self,
        token: &str,
        claimed: &[CanonicalPathKey],
    ) -> io::Result<WorkspaceTab> {
        let token = token.to_owned();
        let (tab, contents) = self
            .worker()?
            .call(move |db| block_on(db.read_closed_tab(&token)))?;
        Ok(restore_reopened(tab, contents, claimed))
    }

    pub(crate) fn cold_tab_for_path(
        &self,
        path: &Path,
    ) -> io::Result<Option<(WorkspaceTab, super::ColdSessionTab)>> {
        let key = CanonicalPathKey::from_path(path).as_str().to_owned();
        let tab = self
            .worker()?
            .call(move |db| block_on(db.retained_tab_by_path(&key)))?;
        Ok(tab.map(|tab| {
            let mut cold = tab.into_parts();
            cold.reopened = true;
            (Restorer::cold_tab_shell(&cold), cold)
        }))
    }

    pub(crate) fn reopen_path_with_claimed(
        &self,
        path: &Path,
        claimed: &[CanonicalPathKey],
    ) -> io::Result<Option<WorkspaceTab>> {
        let key = CanonicalPathKey::from_path(path).as_str().to_owned();
        let retained = self
            .worker()?
            .call(move |db| block_on(db.load_retained_path(&key)))?;
        Ok(retained.map(|(tab, contents)| restore_reopened(tab, contents, claimed)))
    }
}
