//! Reopening restores one retained document tree, then reconciles observed disk text.
use super::{Restorer, SessionStore, model::SessionTab};
use crate::app::{CanonicalPathKey, domain::WorkspaceTab, services::file_service::FileService};
use futures_lite::future::block_on;
use std::{collections::HashMap, io, path::Path};

#[cfg(test)]
mod tests;

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
        match FileService::read_file(&path) {
            Ok(file) => {
                let digest = file.document.content_digest();
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
                buffer.disk_state = FileService::read_disk_state(&path).ok();
                buffer.is_dirty = original.is_dirty;
                if digest != baseline {
                    let loaded = FileService::build_buffer_from_file_content(
                        &path,
                        file,
                        buffer.disk_state.clone(),
                    );
                    buffer.replace_from_loaded_buffer_source(
                        loaded,
                        crate::app::domain::buffer::PieceSource::External,
                    );
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
