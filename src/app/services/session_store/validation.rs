use super::model::{SessionBuffer, SessionPaneNode, SessionTab};
use std::{collections::HashSet, io};

pub(super) fn needs_snapshot(buffer: &SessionBuffer) -> bool {
    buffer.path.is_none() || buffer.is_dirty
}
fn valid_temp_id(id: &str) -> bool {
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
    node: &SessionPaneNode,
    views: &HashSet<u64>,
    leaves: &mut HashSet<u64>,
) -> io::Result<()> {
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
