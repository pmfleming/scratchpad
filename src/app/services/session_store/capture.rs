use super::{
    ColdSessionTab, SessionActiveSurface, SessionBuffer, SessionPaneNode, SessionTab, SessionView,
    WorkspaceTab,
};
use crate::app::domain::DocumentSnapshot;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);

pub(crate) struct SessionPersistRequest {
    pub(super) generation: u64,
    pub(super) active_tab_index: usize,
    pub(super) active_surface: SessionActiveSurface,
    pub(super) font_size: f32,
    pub(super) word_wrap: bool,
    pub(super) tabs: Vec<CapturedSessionTab>,
    pub(crate) closed_tabs: Vec<CapturedClosedTab>,
    pub(super) version_history: bool,
}

#[derive(Clone)]
pub(crate) struct CapturedClosedTab {
    pub(crate) token: String,
    pub(super) closed_at: i64,
    pub(super) tab: CapturedSessionTab,
}

#[derive(Clone)]
pub(super) struct CapturedSessionTab {
    pub(super) session_tab: SessionTab,
    pub(super) buffer_snapshots: Vec<CapturedSessionBuffer>,
    pub(super) preserve_existing: bool,
}

#[derive(Clone)]
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
            generation: NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed),
            active_tab_index,
            active_surface: SessionActiveSurface::Workspace,
            font_size,
            word_wrap,
            tabs: tabs.iter().map(CapturedSessionTab::capture).collect(),
            closed_tabs: Vec::new(),
            version_history: true,
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
            generation: NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed),
            active_tab_index,
            active_surface,
            font_size,
            word_wrap,
            closed_tabs: Vec::new(),
            version_history: true,
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

impl CapturedClosedTab {
    pub(crate) fn capture(tab: &WorkspaceTab, cold: Option<&ColdSessionTab>) -> Self {
        let mut captured = cold.cloned().map_or_else(
            || CapturedSessionTab::capture(tab),
            CapturedSessionTab::capture_cold,
        );
        if cold.is_none() {
            // A closed disk-backed tab must reopen with its original text/history,
            // even if the file subsequently changes or disappears.
            captured.buffer_snapshots = tab
                .buffers()
                .filter(|buffer| !buffer.is_loading_preview)
                .map(|buffer| CapturedSessionBuffer {
                    temp_id: buffer.temp_id.clone(),
                    snapshot: buffer.document_snapshot(),
                })
                .collect();
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Self {
            token: format!(
                "{}-{}-{}",
                std::process::id(),
                now.as_nanos(),
                NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed)
            ),
            closed_at: now.as_secs().min(i64::MAX as u64) as i64,
            tab: captured,
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
            preserve_existing: true,
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
                active_view_id: tab.layout.active_view_id(),
                views: tab.layout.views().iter().map(SessionView::from).collect(),
                root_pane: SessionPaneNode::from(tab.layout.root_pane()),
            },
            buffer_snapshots,
            preserve_existing: false,
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
