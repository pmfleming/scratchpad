use super::{
    ColdSessionTab, SessionActiveSurface, SessionBuffer, SessionPaneNode, SessionTab, SessionView,
    WorkspaceTab,
};
use crate::app::domain::DocumentSnapshot;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);

pub(crate) struct SessionPersistRequest {
    pub(crate) activity: Vec<super::activity::ActivityEvent>,
    pub(crate) forgotten: Vec<(String, u64)>,
    pub(super) generation: u64,
    pub(super) active_tab_index: usize,
    pub(super) active_surface: SessionActiveSurface,
    pub(super) font_size: f32,
    pub(super) word_wrap: bool,
    pub(super) tabs: Vec<CapturedSessionTab>,
    pub(crate) closed_tabs: Vec<CapturedClosedTab>,
    pub(super) removed_documents: Vec<String>,
}

#[derive(Clone, Default)]
pub(crate) struct SessionCaptureAck {
    pub closed_tokens: Vec<String>,
    pub activity_sequences: Vec<u64>,
    pub forgotten: Vec<(String, u64)>,
}

#[derive(Clone)]
pub(crate) struct CapturedClosedTab {
    pub(crate) token: String,
    pub(super) closed_at: i64,
    pub(super) tab: CapturedSessionTab,
    pub(super) discarded_documents: Vec<String>,
}

#[derive(Clone)]
pub(super) struct CapturedSessionTab {
    pub(super) loading_preview_only: bool,
    pub(super) session_tab: SessionTab,
    pub(super) buffer_snapshots: Vec<CapturedSessionBuffer>,
    pub(super) preserve_existing: bool,
    pub(super) discard_history: bool,
}

#[derive(Clone)]
pub(super) struct CapturedSessionBuffer {
    pub(super) temp_id: String,
    pub(super) snapshot: DocumentSnapshot,
}

impl SessionPersistRequest {
    pub(crate) fn acknowledgement(&self) -> SessionCaptureAck {
        SessionCaptureAck {
            closed_tokens: self.closed_tabs.iter().map(|c| c.token.clone()).collect(),
            activity_sequences: self.activity.iter().map(|e| e.sequence).collect(),
            forgotten: self.forgotten.clone(),
        }
    }
    pub(crate) fn close_view(
        &mut self,
        tabs: &[WorkspaceTab],
        index: usize,
        view: crate::app::domain::ViewId,
    ) -> bool {
        let Some(original) = tabs.get(index) else {
            return false;
        };
        let removed = original
            .buffer_for_view(view)
            .filter(|_| original.is_last_view_for_buffer(view) == Some(true))
            .cloned();
        let mut remaining = original.clone();
        if !remaining.close_view(view) {
            return false;
        }
        let cold = self.tabs[index]
            .preserve_existing
            .then(|| self.tabs[index].session_tab.clone().into_parts());
        self.tabs[index] = if let Some(mut cold) = cold.clone() {
            cold.retain_workspace_layout(&remaining);
            CapturedSessionTab::capture_cold(cold)
        } else {
            CapturedSessionTab::capture(&remaining)
        };
        if let Some(buffer) = removed {
            let closed = WorkspaceTab::new(buffer);
            let cold = cold.map(|mut cold| {
                cold.retain_workspace_layout(&closed);
                cold
            });
            self.add_close(CapturedClosedTab::capture(&closed, cold.as_ref()));
        }
        true
    }

    pub(crate) fn close_tab(&mut self, index: usize, closed: CapturedClosedTab) {
        self.tabs.remove(index);
        self.active_tab_index = self
            .active_tab_index
            .saturating_sub(usize::from(index < self.active_tab_index))
            .min(self.tabs.len().saturating_sub(1));
        self.add_close(closed);
    }

    fn add_close(&mut self, mut closed: CapturedClosedTab) {
        use super::activity::{ActivityEvent, ActivityKind};
        self.removed_documents
            .append(&mut closed.discarded_documents);
        let discarded: std::collections::HashSet<_> = self.removed_documents.iter().collect();
        closed
            .tab
            .session_tab
            .buffers
            .retain(|b| !discarded.contains(&b.temp_id));
        closed
            .tab
            .buffer_snapshots
            .retain(|b| !discarded.contains(&b.temp_id));
        if closed.tab.session_tab.buffers.is_empty() {
            return;
        }
        let buffer_ids: std::collections::HashSet<_> = closed
            .tab
            .session_tab
            .buffers
            .iter()
            .map(|b| b.id)
            .collect();
        closed
            .tab
            .session_tab
            .views
            .retain(|v| buffer_ids.contains(&v.buffer_id));
        let views: std::collections::HashSet<_> =
            closed.tab.session_tab.views.iter().map(|v| v.id).collect();
        let mut pane = crate::app::domain::PaneNode::from(closed.tab.session_tab.root_pane.clone());
        pane.retain_views(&views);
        closed.tab.session_tab.root_pane = SessionPaneNode::from(&pane);
        if !views.contains(&closed.tab.session_tab.active_view_id) {
            closed.tab.session_tab.active_view_id = pane.first_view_id();
        }
        for buffer in &closed.tab.session_tab.buffers {
            self.activity.push(ActivityEvent {
                sequence: crate::app::domain::buffer::next_text_history_global_seq(),
                timestamp: closed.closed_at as u64,
                kind: ActivityKind::Closed,
                document: buffer.temp_id.clone(),
                name: buffer.name.clone(),
                path: buffer
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
            });
        }
        self.closed_tabs.push(closed);
    }

    pub(crate) fn capture(
        tabs: &[WorkspaceTab],
        active_tab_index: usize,
        font_size: f32,
        word_wrap: bool,
    ) -> Self {
        Self {
            activity: Vec::new(),
            forgotten: Vec::new(),
            generation: NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed),
            active_tab_index,
            active_surface: SessionActiveSurface::Workspace,
            font_size,
            word_wrap,
            tabs: tabs.iter().map(CapturedSessionTab::capture).collect(),
            closed_tabs: Vec::new(),
            removed_documents: Vec::new(),
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
            activity: Vec::new(),
            forgotten: Vec::new(),
            generation: NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed),
            active_tab_index,
            active_surface,
            font_size,
            word_wrap,
            closed_tabs: Vec::new(),
            removed_documents: Vec::new(),
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
            discarded_documents: if cold.is_none() {
                tab.buffers()
                    .filter(|b| {
                        b.is_loading_preview
                            || (b.path.is_none()
                                && b.document().history_entries().is_empty()
                                && b.document().piece_tree().len_chars() == 0)
                    })
                    .map(|b| b.temp_id.clone())
                    .collect()
            } else {
                cold.into_iter()
                    .flat_map(|tab| &tab.buffers)
                    .filter(|buffer| {
                        buffer.path.is_none()
                            && !buffer.is_dirty
                            && buffer.text_is_empty
                            && !buffer.has_recorded_edits
                    })
                    .map(|buffer| buffer.temp_id.clone())
                    .collect()
            },
        }
    }
}

pub(crate) fn cold_tab_from_workspace_tab(tab: &WorkspaceTab) -> ColdSessionTab {
    CapturedSessionTab::capture(tab).session_tab.into_parts()
}

impl CapturedSessionTab {
    fn capture_cold(session_tab: ColdSessionTab) -> Self {
        Self {
            loading_preview_only: false,
            session_tab: SessionTab::from(session_tab),
            buffer_snapshots: Vec::new(),
            preserve_existing: true,
            discard_history: false,
        }
    }

    fn capture(tab: &WorkspaceTab) -> Self {
        let mut buffers = Vec::new();
        let mut buffer_snapshots = Vec::new();
        for buffer in tab.buffers().filter(|buffer| !buffer.is_loading_preview) {
            buffers.push(SessionBuffer::capture(buffer));
            if !buffer.is_loading_preview
                && (buffer.path.is_none()
                    || buffer.is_dirty
                    || buffer.disk_state.is_none()
                    || !buffer.document().history_entries().is_empty())
            {
                buffer_snapshots.push(CapturedSessionBuffer {
                    temp_id: buffer.temp_id.clone(),
                    snapshot: buffer.document_snapshot(),
                });
            }
        }

        let views: Vec<_> = tab
            .layout
            .views()
            .iter()
            .filter(|view| buffers.iter().any(|buffer| buffer.id == view.buffer_id))
            .map(SessionView::from)
            .collect();
        let view_ids = views.iter().map(|view| view.id).collect();
        let mut pane = tab.layout.root_pane().clone();
        pane.retain_views(&view_ids);
        let active_view_id = views
            .iter()
            .find(|view| view.id == tab.layout.active_view_id())
            .or(views.first())
            .map_or(tab.layout.active_view_id(), |view| view.id);
        Self {
            loading_preview_only: tab.buffers().all(|buffer| buffer.is_loading_preview),
            session_tab: SessionTab {
                buffers,
                active_view_id,
                views,
                root_pane: SessionPaneNode::from(&pane),
            },
            buffer_snapshots,
            preserve_existing: false,
            discard_history: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CapturedSessionTab;
    use crate::app::domain::{BufferState, WorkspaceTab};

    #[test]
    fn clean_disk_buffer_retains_initial_edit_and_current_content_cache() {
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

        assert_eq!(captured.buffer_snapshots.len(), 1);
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
