use crate::app::domain::{
    EditorViewState, PaneNode, SplitAxis,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SESSION_VERSION: u32 = 6;

#[derive(Serialize, Deserialize)]
pub(crate) struct SessionManifest {
    pub version: u32,
    pub active_tab_index: usize,
    pub font_size: f32,
    pub word_wrap: bool,
    pub logging_enabled: bool,
    pub tabs: Vec<SessionTab>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SessionTab {
    pub buffer_id: u64,
    pub name: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub temp_id: String,
    pub encoding: String,
    pub has_bom: bool,
    pub active_view_id: u64,
    pub views: Vec<SessionView>,
    pub root_pane: SessionPaneNode,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SessionView {
    pub id: u64,
    pub buffer_id: u64,
    pub show_line_numbers: bool,
    pub show_control_chars: bool,
}

impl From<&EditorViewState> for SessionView {
    fn from(view: &EditorViewState) -> Self {
        Self {
            id: view.id,
            buffer_id: view.buffer_id,
            show_line_numbers: view.show_line_numbers,
            show_control_chars: view.show_control_chars,
        }
    }
}

#[derive(Serialize, Deserialize)]
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
