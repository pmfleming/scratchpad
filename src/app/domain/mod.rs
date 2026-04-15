pub mod buffer;
pub mod panes;
pub mod tab;
pub mod tab_manager;
pub(crate) mod tab_support;
pub mod view;

pub use buffer::{
    BufferFreshness, BufferId, BufferState, DiskFileState, EncodingSource, LineEndingCounts,
    LineEndingStyle, RenderedLayout, RestoredBufferState, TextArtifactSummary, TextDocument,
    TextDocumentUndoState, TextDocumentUndoer, TextFormatMetadata, analyze_line_endings,
    display_line_count, platform_default_line_ending,
};
pub use panes::{PaneBranch, PaneNode, SplitAxis, SplitPath};
pub use tab::WorkspaceTab;
pub use tab_manager::{PendingAction, TabManager};
pub use view::{EditorViewState, SearchHighlightState, ViewId};
