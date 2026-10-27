use super::{Restorer, SessionStore};
use crate::app::{CanonicalPathKey, domain::WorkspaceTab, services::file_service::FileService};
use futures_lite::future::block_on;
use std::{io, path::PathBuf};

#[cfg(test)]
mod tests;

pub(crate) struct ReopenedArchive {
    pub tab: Option<WorkspaceTab>,
    pub disk_paths: Vec<PathBuf>,
}

impl SessionStore {
    /// Non-destructive read. The next committed live session transfers ownership.
    /// Changed clean archives stay archived, forming a separate historical branch.
    pub(crate) fn reopen_archive(&self, token: &str) -> io::Result<ReopenedArchive> {
        let token = token.to_owned();
        let (mut tab, contents) = self
            .worker()?
            .call(move |db| block_on(db.read_closed_tab(&token)))?;
        let mut changed = Vec::new();
        let mut missing = Vec::new();
        let mut disk_states = Vec::new();
        for buffer in &tab.buffers {
            let Some(path) = &buffer.path else {
                changed.push(false);
                missing.push(false);
                disk_states.push(None);
                continue;
            };
            match FileService::read_file(path) {
                Ok(file) => {
                    let digest = file.document.content_digest();
                    let unchanged = buffer.disk_content_digest.as_ref().map_or_else(
                        || {
                            !buffer.is_dirty
                                && contents.get(&buffer.temp_id).is_some_and(|text| {
                                    blake3::hash(text.as_bytes()).to_hex().as_str() == digest
                                })
                        },
                        |baseline| *baseline == digest,
                    );
                    changed.push(!unchanged);
                    missing.push(false);
                    disk_states.push(FileService::read_disk_state(path).ok());
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    changed.push(false);
                    missing.push(true);
                    disk_states.push(None);
                }
                Err(error) => return Err(error),
            }
        }
        let disk_paths = tab
            .buffers
            .iter()
            .zip(&changed)
            .filter_map(|(buffer, changed)| changed.then(|| buffer.path.clone()).flatten())
            .collect();
        if changed.iter().any(|changed| *changed)
            && !missing.iter().any(|missing| *missing)
            && tab
                .buffers
                .iter()
                .all(|buffer| !buffer.is_dirty && buffer.path.is_some())
        {
            return Ok(ReopenedArchive {
                tab: None,
                disk_paths: tab
                    .buffers
                    .iter()
                    .filter_map(|buffer| buffer.path.clone())
                    .collect(),
            });
        }
        let originals = tab.buffers.clone();
        for buffer in &mut tab.buffers {
            if contents.contains_key(&buffer.temp_id) {
                // Restore exactly the archived text/history, without the normal
                // startup disk-refresh path substituting externally changed text.
                buffer.path = None;
                buffer.is_dirty = true;
            }
        }
        let mut restored = Restorer::new(contents).restore_tab_with_summary(tab).0;
        for (index, original) in originals.iter().enumerate() {
            let Some(buffer) = restored
                .buffers_mut()
                .find(|buffer| buffer.temp_id == original.temp_id)
            else {
                continue;
            };
            if changed[index] || missing[index] {
                buffer.path = None;
                buffer.path_key = None;
                buffer.name = format!("{} (recovered)", original.name);
                buffer.disk_state = None;
                buffer.disk_content_digest = None;
                buffer.is_dirty = true;
            } else {
                buffer.path.clone_from(&original.path);
                buffer.path_key = buffer.path.as_deref().map(CanonicalPathKey::from_path);
                buffer.disk_state.clone_from(&disk_states[index]);
                buffer
                    .disk_content_digest
                    .clone_from(&original.disk_content_digest);
                buffer.is_dirty = original.is_dirty;
            }
        }
        Ok(ReopenedArchive {
            tab: Some(restored),
            disk_paths,
        })
    }
}
