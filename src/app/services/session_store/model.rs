use crate::app::domain::{PaneNode, PersistedHistoryEntry, SplitAxis, TextFormatMetadata};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SESSION_VERSION: u32 = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionActiveSurface {
    #[default]
    Workspace,
    Settings,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SessionManifest {
    pub version: u32,
    pub tab_count: usize,
    pub active_tab_index: usize,
    pub active_surface: SessionActiveSurface,
    pub tabs: Vec<SessionTab>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SessionTab {
    pub layout_id: u64,
    pub buffers: Vec<SessionBuffer>,
    pub active_view_id: u64,
    pub views: Vec<SessionView>,
    pub root_pane: SessionPaneNode,
}

#[derive(Clone)]
pub(crate) struct SessionTabShell {
    pub layout_id: u64,
    pub active_view_id: u64,
    pub views: Vec<SessionView>,
    pub root_pane: SessionPaneNode,
}

#[derive(Clone)]
pub(crate) struct SessionTabParts {
    pub shell: SessionTabShell,
    pub buffers: Vec<SessionBuffer>,
    pub reopened: bool,
}

impl SessionTab {
    pub(crate) fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    pub(crate) fn into_parts(self) -> SessionTabParts {
        let SessionTab {
            layout_id,
            buffers,
            active_view_id,
            views,
            root_pane,
        } = self;
        SessionTabParts {
            reopened: false,
            shell: SessionTabShell {
                layout_id: if layout_id == 0 {
                    crate::app::domain::tab::next_layout_id()
                } else {
                    layout_id
                },
                active_view_id,
                views,
                root_pane,
            },
            buffers,
        }
    }
}

impl From<SessionTabParts> for SessionTab {
    fn from(parts: SessionTabParts) -> Self {
        Self {
            layout_id: parts.shell.layout_id,
            buffers: parts.buffers,
            active_view_id: parts.shell.active_view_id,
            views: parts.shell.views,
            root_pane: parts.shell.root_pane,
        }
    }
}

impl SessionTabParts {
    /// Keep unloaded document metadata while updating the workspace layout.
    pub(crate) fn retain_workspace_layout(&mut self, tab: &crate::app::domain::WorkspaceTab) {
        self.buffers
            .retain(|buffer| tab.buffer_by_id(buffer.id).is_some());
        self.shell.layout_id = tab.layout_id;
        self.shell.active_view_id = tab.layout.active_view_id();
        self.shell.views = tab.layout.views().iter().map(SessionView::from).collect();
        self.shell.root_pane = SessionPaneNode::from(tab.layout.root_pane());
    }

    pub(crate) fn buffer_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.buffers.iter().map(|buffer| buffer.id)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SessionBuffer {
    pub id: u64,
    pub name: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub text_is_empty: bool,
    pub has_recorded_edits: bool,
    pub is_settings_file: bool,
    pub show_control_chars: bool,
    pub right_to_left_reading_order: bool,
    pub temp_id: String,
    pub format: TextFormatMetadata,
    pub line_count: usize,
    #[serde(skip)]
    pub text_metadata: Option<crate::app::domain::buffer::BufferTextMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_modified_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_len: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_content_digest: Option<String>,
    pub observed_disk_digest: Option<String>,
    pub observed_disk_missing: bool,
    #[serde(skip)]
    pub pending_disk_observation: bool,
    // Capture/hydration transport (and explicit recovery export) only. The DB
    // writer drains this into history rows before serializing tab metadata.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_history: Vec<PersistedHistoryEntry>,
    #[serde(skip)]
    pub history_document: Option<crate::app::domain::TextDocument>,
    #[serde(skip)]
    pub history_capture: Option<crate::app::domain::buffer::HistorySnapshot>,
}

#[cfg(test)]
impl From<&crate::app::domain::BufferState> for SessionBuffer {
    fn from(buffer: &crate::app::domain::BufferState) -> Self {
        let mut metadata = Self::capture(buffer);
        metadata.text_history = buffer.document().exported_history();
        metadata.history_capture = None;
        metadata
    }
}

impl SessionBuffer {
    pub(crate) fn capture(buffer: &crate::app::domain::BufferState) -> Self {
        Self {
            id: buffer.id,
            name: buffer.name.clone(),
            path: buffer.path.clone(),
            is_dirty: buffer.is_dirty,
            text_is_empty: buffer.document().piece_tree().len_chars() == 0,
            has_recorded_edits: buffer
                .document()
                .history_entries()
                .iter()
                .any(|entry| entry.summary != "Initial contents"),
            is_settings_file: buffer.is_settings_file,
            show_control_chars: buffer.show_control_chars,
            right_to_left_reading_order: buffer.right_to_left_reading_order,
            temp_id: buffer.temp_id.clone(),
            format: buffer.format.clone(),
            line_count: buffer.line_count,
            text_metadata: Some(crate::app::domain::buffer::BufferTextMetadata {
                line_count: buffer.line_count,
                artifact_summary: buffer.artifact_summary.clone(),
                preferred_line_ending: buffer.format.preferred_line_ending,
                has_non_compliant_characters: buffer.has_non_compliant_characters,
            }),
            disk_modified_millis: buffer
                .disk_state
                .as_ref()
                .and_then(|state| state.modified_millis),
            disk_len: buffer.disk_state.as_ref().map(|state| state.len),
            disk_content_digest: buffer.disk_content_digest.clone(),
            observed_disk_digest: buffer.observed_disk_digest.clone(),
            observed_disk_missing: buffer.observed_disk_missing,
            pending_disk_observation: false,
            text_history: Vec::new(),
            history_document: None,
            history_capture: Some(buffer.document().history_snapshot()),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SessionView {
    pub id: u64,
    pub buffer_id: u64,
    pub selection: Option<crate::app::domain::CursorRange>,
    pub show_line_numbers: bool,
    pub zoom_factor: f32,
}

impl From<&crate::app::domain::EditorViewState> for SessionView {
    fn from(view: &crate::app::domain::EditorViewState) -> Self {
        Self {
            id: view.id,
            buffer_id: view.buffer_id,
            selection: view.pending_cursor_range.or(view.cursor_range),
            show_line_numbers: view.show_line_numbers,
            zoom_factor: view.zoom_factor(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum SessionPaneNode {
    Leaf {
        view_id: u64,
    },
    Split {
        axis: SessionSplitAxis,
        ratio: f32,
        first: Box<SessionPaneNode>,
        second: Box<SessionPaneNode>,
    },
}

impl From<&PaneNode> for SessionPaneNode {
    fn from(node: &PaneNode) -> Self {
        match node {
            PaneNode::Leaf { view_id } => SessionPaneNode::Leaf { view_id: *view_id },
            PaneNode::Split {
                axis,
                ratio,
                first,
                second,
            } => SessionPaneNode::Split {
                axis: (*axis).into(),
                ratio: *ratio,
                first: Box::new(first.as_ref().into()),
                second: Box::new(second.as_ref().into()),
            },
        }
    }
}

impl From<SessionPaneNode> for PaneNode {
    fn from(node: SessionPaneNode) -> Self {
        match node {
            SessionPaneNode::Leaf { view_id } => PaneNode::Leaf { view_id },
            SessionPaneNode::Split {
                axis,
                ratio,
                first,
                second,
            } => PaneNode::Split {
                axis: axis.into(),
                ratio,
                first: Box::new((*first).into()),
                second: Box::new((*second).into()),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub(crate) enum SessionSplitAxis {
    Horizontal,
    Vertical,
}

impl From<SplitAxis> for SessionSplitAxis {
    fn from(axis: SplitAxis) -> Self {
        match axis {
            SplitAxis::Horizontal => SessionSplitAxis::Horizontal,
            SplitAxis::Vertical => SessionSplitAxis::Vertical,
        }
    }
}

impl From<SessionSplitAxis> for SplitAxis {
    fn from(axis: SessionSplitAxis) -> Self {
        match axis {
            SessionSplitAxis::Horizontal => SplitAxis::Horizontal,
            SessionSplitAxis::Vertical => SplitAxis::Vertical,
        }
    }
}
