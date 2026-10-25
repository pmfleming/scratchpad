//! Read-only v7 importer. Original JSON and snapshots are independent backups.
#[cfg(test)]
mod tests;
use super::capture::{CapturedSessionBuffer, CapturedSessionTab, SessionPersistRequest};
use super::model::{SessionBuffer, SessionManifest, SessionTab};
use crate::app::domain::TextDocument;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

pub(super) fn read(
    root: &Path,
    fallback: Option<&Path>,
) -> io::Result<Option<SessionPersistRequest>> {
    let source = if root.join("session.json").try_exists()? {
        Some(root)
    } else {
        fallback.filter(|path| path.join("session.json").is_file())
    };
    let Some(source) = source else {
        return Ok(None);
    };
    let manifest: SessionManifest =
        serde_json::from_slice(&fs::read(source.join("session.json"))?).map_err(invalid)?;
    if manifest.version != super::SESSION_VERSION {
        return Err(invalid(
            "unsupported legacy session version; original retained",
        ));
    }
    let tabs = manifest
        .tabs
        .into_iter()
        .map(|tab| read_tab(source, tab))
        .collect::<io::Result<Vec<_>>>()?;
    Ok(Some(SessionPersistRequest {
        generation: 0,
        active_tab_index: manifest.active_tab_index,
        active_surface: manifest.active_surface,
        font_size: manifest.font_size,
        word_wrap: manifest.word_wrap,
        tabs,
    }))
}
fn read_tab(root: &Path, tab: SessionTab) -> io::Result<CapturedSessionTab> {
    let mut tab = SessionTab::from(tab.into_parts());
    validate_tab(&tab)?;
    // v7 also accepted this older per-view flag; normalize before serializing.
    for buffer in &mut tab.buffers {
        buffer.show_control_chars |= tab
            .views
            .iter()
            .any(|view| view.buffer_id == buffer.id && view.show_control_chars);
    }
    let snapshots = tab
        .buffers
        .iter()
        .map(|buffer| read_buffer(root, buffer))
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok(CapturedSessionTab {
        session_tab: tab,
        buffer_snapshots: snapshots,
    })
}
fn read_buffer(root: &Path, buffer: &SessionBuffer) -> io::Result<Option<CapturedSessionBuffer>> {
    if !valid_temp_id(&buffer.temp_id) {
        return Err(invalid("unsafe legacy snapshot identifier"));
    }
    let path = root.join(format!("{}.tmp", buffer.temp_id));
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound && !needs_snapshot(buffer) => {
            return Ok(None);
        }
        Err(error) => return Err(error),
        _ => return Err(invalid("legacy snapshot is not a regular file")),
    }
    let document = TextDocument::new(fs::read_to_string(path)?);
    Ok(Some(CapturedSessionBuffer {
        temp_id: buffer.temp_id.clone(),
        snapshot: document.snapshot(),
    }))
}
pub(super) fn needs_snapshot(buffer: &SessionBuffer) -> bool {
    buffer.path.is_none() || buffer.is_dirty
}
pub(super) fn valid_temp_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
pub(super) fn validate_tab(tab: &SessionTab) -> io::Result<()> {
    let buffers: HashSet<_> = tab.buffers.iter().map(|buffer| buffer.id).collect();
    let views: HashSet<_> = tab.views.iter().map(|view| view.id).collect();
    let snapshots: HashSet<_> = tab.buffers.iter().map(|buffer| &buffer.temp_id).collect();
    if snapshots.len() != tab.buffers.len() || snapshots.iter().any(|id| !valid_temp_id(id)) {
        return Err(invalid("invalid session snapshot identifiers"));
    }
    if buffers.is_empty()
        || buffers.len() != tab.buffers.len()
        || views.len() != tab.views.len()
        || !views.contains(&tab.active_view_id)
        || tab
            .views
            .iter()
            .any(|view| !buffers.contains(&view.buffer_id))
    {
        return Err(invalid("invalid session buffer/view references"));
    }
    let mut leaves = HashSet::new();
    validate_pane(&tab.root_pane, &views, &mut leaves)?;
    if leaves != views {
        return Err(invalid("pane layout does not cover session views"));
    }
    Ok(())
}
fn validate_pane(
    node: &super::model::SessionPaneNode,
    views: &HashSet<u64>,
    leaves: &mut HashSet<u64>,
) -> io::Result<()> {
    use super::model::SessionPaneNode;
    match node {
        SessionPaneNode::Leaf { view_id } => {
            if !views.contains(view_id) || !leaves.insert(*view_id) {
                return Err(invalid("invalid pane view reference"));
            }
        }
        SessionPaneNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            if !ratio.is_finite() || !(0.0..=1.0).contains(ratio) {
                return Err(invalid("invalid pane ratio"));
            }
            validate_pane(first, views, leaves)?;
            validate_pane(second, views, leaves)?;
        }
    }
    Ok(())
}
fn invalid(cause: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, cause.to_string())
}
