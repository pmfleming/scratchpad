use crate::app::domain::{EditorViewState, PaneNode, SplitAxis};
use crate::app::services::settings_store::{
    AppSettings, DEFAULT_FONT_SIZE, DEFAULT_LOGGING_ENABLED, DEFAULT_WORD_WRAP,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SESSION_VERSION: u32 = 6;

#[derive(Serialize, Deserialize)]
pub(crate) struct SessionManifest {
    pub version: u32,
    pub active_tab_index: usize,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_word_wrap")]
    pub word_wrap: bool,
    #[serde(default = "default_logging_enabled")]
    pub logging_enabled: bool,
    pub tabs: Vec<SessionTab>,
}

impl SessionManifest {
    pub fn legacy_settings(&self) -> AppSettings {
        AppSettings {
            font_size: self.font_size,
            word_wrap: self.word_wrap,
            logging_enabled: self.logging_enabled,
            editor_font: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SessionTab {
    #[serde(default)]
    pub buffers: Vec<SessionBuffer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_dirty: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temp_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_bom: Option<bool>,
    pub active_view_id: u64,
    pub views: Vec<SessionView>,
    pub root_pane: SessionPaneNode,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SessionBuffer {
    pub id: u64,
    pub name: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub temp_id: String,
    pub encoding: String,
    pub has_bom: bool,
}

impl From<&crate::app::domain::BufferState> for SessionBuffer {
    fn from(buffer: &crate::app::domain::BufferState) -> Self {
        Self {
            id: buffer.id,
            name: buffer.name.clone(),
            path: buffer.path.clone(),
            is_dirty: buffer.is_dirty,
            temp_id: buffer.temp_id.clone(),
            encoding: buffer.encoding.clone(),
            has_bom: buffer.has_bom,
        }
    }
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

const fn default_font_size() -> f32 {
    DEFAULT_FONT_SIZE
}

const fn default_word_wrap() -> bool {
    DEFAULT_WORD_WRAP
}

const fn default_logging_enabled() -> bool {
    DEFAULT_LOGGING_ENABLED
}
