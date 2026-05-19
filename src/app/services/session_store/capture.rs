use super::{
    CapturedSessionBuffer, CapturedSessionTab, ColdSessionTab, SessionActiveSurface, SessionBuffer,
    SessionPaneNode, SessionPersistRequest, SessionTab, SessionView, WorkspaceTab,
};

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
            buffer_snapshots.push(CapturedSessionBuffer {
                temp_id: buffer.temp_id.clone(),
                snapshot: buffer.document_snapshot(),
            });
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
