use super::{TabAttentionState, WorkspaceTab};
use crate::app::domain::{BufferFreshness, ViewId};

pub(crate) fn display_name(tab: &WorkspaceTab) -> String {
    if tab.distinct_buffer_count() < 2 {
        return tab.active_buffer().display_name();
    }

    let names = tab.distinct_buffer_names_in_view_order();
    let first = names
        .first()
        .cloned()
        .unwrap_or_else(|| tab.active_buffer().name.clone());
    let second = names
        .get(1)
        .cloned()
        .unwrap_or_else(|| tab.active_buffer().name.clone());
    format!("[{}] {} & {}", names.len(), first, second)
}

pub(crate) fn full_display_name(tab: &WorkspaceTab, has_duplicate: bool) -> String {
    let name = display_name(tab);
    if has_duplicate && let Some(context) = overflow_context_label(tab) {
        return format!("{name} ({context})");
    }
    name
}

pub(crate) fn overflow_context_label(tab: &WorkspaceTab) -> Option<String> {
    tab.active_buffer().overflow_context_label()
}

pub(crate) fn can_promote_view(tab: &WorkspaceTab, view_id: ViewId) -> bool {
    tab.layout.view(view_id).is_some() && tab.distinct_buffer_count() > 1
}

pub(crate) fn can_promote_all_files(tab: &WorkspaceTab) -> bool {
    tab.distinct_buffer_count() >= 3
}

pub(crate) fn attention_state(tab: &WorkspaceTab) -> Option<TabAttentionState> {
    let mut has_auto_edit = false;
    let mut has_dirty = false;

    for buffer in tab.buffers() {
        match buffer.freshness {
            BufferFreshness::ConflictOnDisk
            | BufferFreshness::MissingOnDisk
            | BufferFreshness::StaleOnDisk => return Some(TabAttentionState::DiskProblem),
            BufferFreshness::AutoReloaded => has_auto_edit = true,
            BufferFreshness::InSync => {}
        }
        has_dirty |= buffer.is_dirty;
    }

    if has_dirty {
        Some(TabAttentionState::Dirty)
    } else if has_auto_edit {
        Some(TabAttentionState::AutoEdit)
    } else {
        None
    }
}
