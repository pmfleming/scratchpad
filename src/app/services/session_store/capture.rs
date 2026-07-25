use super::{
    ColdSessionTab, SessionActiveSurface, SessionBuffer, SessionPaneNode, SessionTab, SessionView,
    WorkspaceTab,
};
use crate::app::domain::DocumentSnapshot;

pub(crate) struct SessionPersistRequest {
    pub(super) active_tab_index: usize,
    pub(super) active_surface: SessionActiveSurface,
    pub(super) font_size: f32,
    pub(super) word_wrap: bool,
    pub(super) tabs: Vec<CapturedSessionTab>,
}

pub(super) struct CapturedSessionTab {
    pub(super) session_tab: SessionTab,
    pub(super) buffer_snapshots: Vec<CapturedSessionBuffer>,
}

pub(super) struct CapturedSessionBuffer {
    pub(super) temp_id: String,
    pub(super) snapshot: DocumentSnapshot,
}

impl SessionPersistRequest {
    pub(crate) fn capture(
        tabs: &[WorkspaceTab],
        active_tab_index: usize,
        font_size: f32,
        word_wrap: bool,
    ) -> Self {
        Self {
            active_tab_index,
            active_surface: SessionActiveSurface::Workspace,
            font_size,
            word_wrap,
            tabs: tabs.iter().map(CapturedSessionTab::capture).collect(),
        }
    }

    pub(crate) fn capture_with_cold_tabs(
        tabs: &[WorkspaceTab],
        cold_tabs: &std::collections::HashMap<usize, ColdSessionTab>,
        active_tab_index: usize,
        active_surface: SessionActiveSurface,
        font_size: f32,
        word_wrap: bool,
    ) -> Self {
        Self {
            active_tab_index,
            active_surface,
            font_size,
            word_wrap,
            tabs: tabs
                .iter()
                .enumerate()
                .map(|(index, tab)| {
                    cold_tabs.get(&index).cloned().map_or_else(
                        || CapturedSessionTab::capture(tab),
                        CapturedSessionTab::capture_cold,
                    )
                })
                .collect(),
        }
    }
}

pub(crate) fn cold_tab_from_workspace_tab(tab: &WorkspaceTab) -> ColdSessionTab {
    CapturedSessionTab::capture(tab).session_tab.into_parts()
}

impl CapturedSessionTab {
    fn capture_cold(session_tab: ColdSessionTab) -> Self {
        Self {
            session_tab: SessionTab::from(session_tab),
            buffer_snapshots: Vec::new(),
        }
    }

    fn capture(tab: &WorkspaceTab) -> Self {
        let mut buffers = Vec::new();
        let mut buffer_snapshots = Vec::new();
        for buffer in tab.buffers() {
            buffers.push(SessionBuffer::from(buffer));
            if !buffer.is_loading_preview
                && (buffer.path.is_none() || buffer.is_dirty || buffer.disk_state.is_none())
            {
                buffer_snapshots.push(CapturedSessionBuffer {
                    temp_id: buffer.temp_id.clone(),
                    snapshot: buffer.document_snapshot(),
                });
            }
        }

        Self {
            session_tab: SessionTab {
                buffers,
                buffer_id: None,
                name: None,
                path: None,
                is_dirty: None,
                temp_id: None,
                encoding: None,
                has_bom: None,
                active_view_id: tab.layout.active_view_id(),
                views: tab.layout.views().iter().map(SessionView::from).collect(),
                root_pane: SessionPaneNode::from(tab.layout.root_pane()),
            },
            buffer_snapshots,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CapturedSessionTab;
    use crate::app::domain::{BufferState, WorkspaceTab};

    #[test]
    fn clean_disk_buffer_is_persisted_as_metadata_not_a_redundant_snapshot() {
        let mut buffer = BufferState::new(
            "large.txt".to_owned(),
            "partial".to_owned(),
            Some(std::path::PathBuf::from("large.txt")),
        );
        buffer.sync_to_disk_state(Some(crate::app::domain::DiskFileState {
            modified_millis: Some(1),
            len: 7,
        }));

        let captured = CapturedSessionTab::capture(&WorkspaceTab::new(buffer));

        assert!(captured.buffer_snapshots.is_empty());
        assert_eq!(captured.session_tab.buffers.len(), 1);

        let mut dirty = BufferState::new(
            "dirty.txt".to_owned(),
            "changed".to_owned(),
            Some(std::path::PathBuf::from("dirty.txt")),
        );
        dirty.mark_dirty_after_local_edit();
        let captured_dirty = CapturedSessionTab::capture(&WorkspaceTab::new(dirty));
        assert_eq!(captured_dirty.buffer_snapshots.len(), 1);
    }
}
