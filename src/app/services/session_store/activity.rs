//! Durable observations; none carries an editor state or node target.
use crate::app::{app_state::ScratchpadApp, domain::BufferState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActivityKind {
    Created,
    Opened,
    Closed,
    DiskConflict,
    Missing,
}

impl ActivityKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Opened => "opened",
            Self::Closed => "closed",
            Self::DiskConflict => "disk_conflict",
            Self::Missing => "missing",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Opened => "Opened",
            Self::Closed => "Closed",
            Self::DiskConflict => "Changed on disk — local work kept",
            Self::Missing => "File missing on disk",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ActivityEvent {
    pub sequence: u64,
    pub timestamp: u64,
    pub kind: ActivityKind,
    pub document: String,
    pub name: String,
    pub path: Option<String>,
}

impl ActivityEvent {
    pub fn new(kind: ActivityKind, buffer: &BufferState) -> Self {
        Self {
            sequence: crate::app::domain::buffer::next_text_history_global_seq(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            kind,
            document: buffer.temp_id.clone(),
            name: buffer.name.clone(),
            path: buffer
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
        }
    }
}

pub(crate) fn record_installed(app: &mut ScratchpadApp, documents: &[String], kind: ActivityKind) {
    let events: Vec<_> = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|t| t.buffers())
        .filter(|b| !b.is_loading_preview && documents.contains(&b.temp_id))
        .map(|b| ActivityEvent::new(kind, b))
        .collect();
    if !events.is_empty() {
        app.state.persistence.pending_activity.extend(events);
        if kind == ActivityKind::Opened {
            for buffer in app
                .tab_manager
                .tabs
                .as_mut_slice()
                .iter_mut()
                .flat_map(|tab| tab.buffers_mut())
                .filter(|b| documents.contains(&b.temp_id))
            {
                buffer.pending_reopen_original = None;
                if let Some(node) = buffer.pending_reopen_external.take() {
                    buffer.document_mut().order_external_edit_after_open(node);
                }
            }
        }
        app.tab_manager.mark_session_dirty();
    }
}
