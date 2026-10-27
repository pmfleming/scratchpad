use super::{Restorer, SessionStore, model::SessionBuffer};
use crate::app::{
    CanonicalPathKey,
    domain::{BufferState, DiskFileState, WorkspaceTab},
    services::file_service::FileService,
};
use futures_lite::future::block_on;
use std::{collections::HashMap, io, path::PathBuf};

#[cfg(test)]
mod tests;

pub(crate) struct ReopenedArchive {
    pub tab: Option<WorkspaceTab>,
    pub disk_paths: Vec<PathBuf>,
}

#[derive(Default)]
struct DiskComparison {
    changed: bool,
    missing: bool,
    state: Option<DiskFileState>,
    clean_disk: Option<BufferState>,
}

fn compare_disk(
    buffer: &SessionBuffer,
    contents: &HashMap<String, String>,
) -> io::Result<DiskComparison> {
    let Some(path) = &buffer.path else {
        return Ok(DiskComparison::default());
    };
    let file = match FileService::read_file(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(DiskComparison {
                missing: true,
                ..Default::default()
            });
        }
        Err(error) => return Err(error),
    };
    let digest = file.document.content_digest();
    let unchanged = buffer.disk_content_digest.as_ref().map_or_else(
        || {
            !buffer.is_dirty
                && contents
                    .get(&buffer.temp_id)
                    .is_some_and(|text| blake3::hash(text.as_bytes()).to_hex().as_str() == digest)
        },
        |baseline| *baseline == digest,
    );
    let state = FileService::read_disk_state(path).ok();
    let clean_disk = (!unchanged && !buffer.is_dirty)
        .then(|| FileService::build_buffer_from_file_content(path, file, state.clone()));
    Ok(DiskComparison {
        changed: !unchanged,
        missing: false,
        state,
        clean_disk,
    })
}

impl SessionStore {
    /// Non-destructive read. The next committed live session transfers ownership.
    /// Changed clean archives stay archived, forming a separate historical branch.
    pub(crate) fn reopen_archive(&self, token: &str) -> io::Result<ReopenedArchive> {
        let token = token.to_owned();
        let (mut tab, contents) = self
            .worker()?
            .call(move |db| block_on(db.read_closed_tab(&token)))?;
        let disk = tab
            .buffers
            .iter()
            .map(|buffer| compare_disk(buffer, &contents))
            .collect::<io::Result<Vec<_>>>()?;
        let disk_paths = tab
            .buffers
            .iter()
            .zip(&disk)
            .filter_map(|(buffer, disk)| {
                (disk.changed && buffer.is_dirty)
                    .then(|| buffer.path.clone())
                    .flatten()
            })
            .collect();
        if disk.iter().any(|disk| disk.changed)
            && !disk.iter().any(|disk| disk.missing)
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
        // Any clean externally changed file forks the workspace. All identities
        // must be new: sharing even one live ID would consume the whole archive.
        let preserve_archive = disk.iter().any(|disk| disk.clean_disk.is_some());
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
        for (original, disk) in originals.iter().zip(disk) {
            let Some(buffer) = restored
                .buffers_mut()
                .find(|buffer| buffer.temp_id == original.temp_id)
            else {
                continue;
            };
            if let Some(clean) = disk.clean_disk {
                buffer.replace_from_loaded_buffer(clean);
                buffer.show_control_chars = original.show_control_chars;
                buffer.right_to_left_reading_order = original.right_to_left_reading_order;
                buffer.is_settings_file = original.is_settings_file;
            } else if disk.changed || disk.missing {
                buffer.path = None;
                buffer.path_key = None;
                buffer.name = format!("{} (recovered)", original.name);
                buffer.disk_state = None;
                buffer.disk_content_digest = None;
                buffer.is_dirty = true;
            } else {
                buffer.path.clone_from(&original.path);
                buffer.path_key = buffer.path.as_deref().map(CanonicalPathKey::from_path);
                buffer.disk_state = disk.state;
                buffer
                    .disk_content_digest
                    .clone_from(&original.disk_content_digest);
                buffer.is_dirty = original.is_dirty;
            }
        }
        if preserve_archive {
            for buffer in restored.buffers_mut() {
                buffer.fork_recovery_identity();
            }
        }
        Ok(ReopenedArchive {
            tab: Some(restored),
            disk_paths,
        })
    }
}
