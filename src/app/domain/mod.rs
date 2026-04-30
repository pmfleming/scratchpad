pub mod buffer;
pub mod panes;
pub mod tab;
pub mod tab_manager;
pub(crate) mod tab_support;
pub mod view;

pub use buffer::{
    AnchorBias, AnchorId, AnchorOwner, AnchorOwnerKind, BufferFreshness, BufferId, BufferState,
    BufferViewStatus, DiskFileState, DocumentChunk, DocumentSnapshot, EncodingSource,
    LineEndingCounts, LineEndingStyle, RestoredBufferState, TextArtifactSummary, TextDocument,
    TextFormatMetadata, analyze_line_endings, display_line_count, platform_default_line_ending,
};
pub use panes::{PaneBranch, PaneNode, SplitAxis, SplitPath};
pub use tab::WorkspaceTab;
pub use tab_manager::{PendingAction, TabManager};
pub use view::{CursorRevealMode, EditorViewState, LayoutCacheKey, SearchHighlightState, ViewId};
