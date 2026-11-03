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
        let mut restored =
            EditorViewState::restored(view.id, view.buffer_id, view.show_line_numbers);
        restored.set_zoom_factor(view.zoom_factor);
        if let Some(mut selection) = view.selection
            && let Some(buffer) = buffers
                .iter_mut()
                .find(|buffer| buffer.id == view.buffer_id)
        {
            let len = buffer.document().piece_tree().len_chars();
            selection.primary.index = selection.primary.index.min(len);
            selection.secondary.index = selection.secondary.index.min(len);
            restored.set_cursor_range_anchored(buffer, selection);
            restored.set_pending_cursor_range_anchored(buffer, selection);
        }
        views.push(restored);
    }

    for buffer in buffers.iter_mut() {
        buffer.show_control_chars = buffer.artifact_summary.has_control_chars()
            && (buffer.show_control_chars || visible_control_char_buffer_ids.contains(&buffer.id));
    }
    let Some(active_buffer_id) =
        active_buffer_id.or_else(|| buffers.first().map(|buffer| buffer.id))
    else {
        return WorkspaceTab::untitled();
    };
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
