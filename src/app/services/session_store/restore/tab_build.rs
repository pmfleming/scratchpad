use super::super::model::SessionTabShell;
use crate::app::domain::{BufferState, EditorViewState, PaneNode, WorkspaceTab};
use std::collections::HashSet;

pub(super) fn workspace_tab_from_restored_buffers(
    shell: SessionTabShell,
    buffers: &mut Vec<BufferState>,
) -> WorkspaceTab {
    let root_pane = PaneNode::from(shell.root_pane);
    let active_view_id = if root_pane.contains_view(shell.active_view_id) {
        shell.active_view_id
    } else {
        root_pane.first_view_id()
    };
    let mut visible_control_char_buffer_ids = HashSet::new();
    let mut active_buffer_id = None;
    let mut views = Vec::with_capacity(shell.views.len());
    for view in shell.views {
        if view.show_control_chars {
            visible_control_char_buffer_ids.insert(view.buffer_id);
        }
        if view.id == active_view_id {
            active_buffer_id = Some(view.buffer_id);
        }
        views.push(EditorViewState::restored(
            view.id,
            view.buffer_id,
            view.show_line_numbers,
        ));
    }

    for buffer in buffers.iter_mut() {
        buffer.show_control_chars = buffer.artifact_summary.has_control_chars()
            && (buffer.show_control_chars || visible_control_char_buffer_ids.contains(&buffer.id));
    }
    let active_buffer_id = active_buffer_id
        .or_else(|| buffers.first().map(|buffer| buffer.id))
        .expect("restored workspace should contain at least one buffer");
    let active_buffer_index = buffers
        .iter()
        .position(|buffer| buffer.id == active_buffer_id)
        .unwrap_or(0);
    let active_buffer = buffers.remove(active_buffer_index);
    WorkspaceTab::restored_with_buffers(
        active_buffer,
        std::mem::take(buffers),
        views,
        root_pane,
        active_view_id,
    )
}
