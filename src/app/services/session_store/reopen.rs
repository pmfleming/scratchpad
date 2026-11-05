//! Reopening restores one retained document tree, then reconciles observed disk text.
use super::{Restorer, SessionStore, model::SessionTab};
use crate::app::{CanonicalPathKey, domain::WorkspaceTab, services::file_service::FileService};
use futures_lite::future::block_on;
use std::{io, path::Path};

#[cfg(test)]
mod tests;

pub(crate) enum RetainedTarget {
    Archive(String),
    Document(String),
    Cold(super::ColdSessionTab),
}

pub(super) fn restore_reopened(
    tab: SessionTab,
    claimed: &[CanonicalPathKey],
    store: Option<crate::app::services::chunk_store::ChunkStoreHandle>,
) -> io::Result<WorkspaceTab> {
    let context = crate::app::services::chunk_store::ImportContext {
        store,
        ..Default::default()
    };
    restore_reopened_with_context(tab, claimed, &context)
}
fn restore_reopened_with_context(
    mut tab: SessionTab,
    claimed: &[CanonicalPathKey],
    context: &crate::app::services::chunk_store::ImportContext,
) -> io::Result<WorkspaceTab> {
    context.check()?;
    let paths: Vec<_> = tab
        .buffers
        .iter_mut()
        .filter_map(|buffer| {
            buffer.path.take().map(|path| {
                (
                    buffer.id,
                    path,
                    (
                        buffer.show_control_chars,
                        buffer.right_to_left_reading_order,
                        buffer.is_settings_file,
                    ),
                )
            })
        })
        .collect();
    let mut restored = Restorer::restore_tab_with_summary(tab)?.0;
    for (id, path, preferences) in paths {
        let Some(buffer) = restored.buffer_by_id_mut(id) else {
            continue;
        };
        let key = CanonicalPathKey::from_path(&path);
        if claimed.contains(&key) {
            buffer.recover_as_untitled("recovered");
            continue;
        }
        match FileService::observe_decoded_digest_with_context(&path, context) {
            Ok((state, mut digest)) => {
                let changed = match buffer
                    .observed_disk_digest
                    .as_deref()
                    .or(buffer.disk_content_digest.as_deref())
                {
                    Some(baseline) => digest != baseline,
                    None => digest != buffer.document().try_content_digest()?,
                };
                buffer.disk_state = Some(state);
                if changed {
                    let observed = FileService::read_observed_file_with_context(&path, context)?;
                    digest.clone_from(&observed.digest);
                    let original_document = Box::new(buffer.document().clone());
                    let previous = buffer.document().history_now();
                    let loaded = FileService::buffer_from_observed(&path, observed);
                    if !buffer.replace_from_loaded_buffer_source(
                        loaded,
                        crate::app::domain::buffer::PieceSource::External,
                    ) {
                        return Err(io::Error::other("Reopened text is unreadable"));
                    }
                    if buffer.document().history_now() != previous {
                        buffer.pending_reopen_external = Some(buffer.document().history_now());
                        buffer.pending_reopen_original = Some(original_document);
                    }
                    (
                        buffer.show_control_chars,
                        buffer.right_to_left_reading_order,
                        buffer.is_settings_file,
                    ) = preferences;
                }
                buffer.set_path(Some(path));
                buffer.observed_disk_digest = Some(digest);
                buffer.observed_disk_missing = false;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                buffer.recover_as_untitled("recovered");
            }
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!("Could not reopen {}: {error}", path.display()),
                ));
            }
        }
    }
    Ok(restored)
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
                let tab = self
                    .worker()?
                    .call(move |db| block_on(db.load_retained_document(&id)))?;
                let mut tab = restore_reopened(tab, claimed, self.opened_chunk_store())?;
                self.bind_tab(&mut tab)?;
                Ok(tab)
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
        let tab = self
            .worker()?
            .call(move |db| block_on(db.read_closed_tab(&token)))?;
        let mut tab = restore_reopened(tab, claimed, self.opened_chunk_store())?;
        self.bind_tab(&mut tab)?;
        Ok(tab)
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

    #[cfg(test)]
    pub(crate) fn reopen_path_with_claimed(
        &self,
        path: &Path,
        claimed: &[CanonicalPathKey],
    ) -> io::Result<Option<WorkspaceTab>> {
        self.reopen_path_with_context(
            path,
            claimed,
            &crate::app::services::chunk_store::ImportContext {
                store: self.opened_chunk_store(),
                ..Default::default()
            },
        )
    }
    pub(crate) fn reopen_path_with_context(
        &self,
        path: &Path,
        claimed: &[CanonicalPathKey],
        context: &crate::app::services::chunk_store::ImportContext,
    ) -> io::Result<Option<WorkspaceTab>> {
        let key = CanonicalPathKey::from_path(path).as_str().to_owned();
        let retained = self
            .worker()?
            .call(move |db| block_on(db.load_retained_path(&key)))?;
        retained
            .map(|tab| {
                let mut tab = restore_reopened_with_context(tab, claimed, context)?;
                self.bind_tab(&mut tab)?;
                Ok(tab)
            })
            .transpose()
    }
}
