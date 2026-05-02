pub mod buffer;
pub mod panes;
pub mod tab;
pub mod tab_manager;
pub(crate) mod tab_support;
pub mod view;

pub use buffer::{
    AnchorBias, AnchorId, AnchorOwner, AnchorOwnerKind, BufferFreshness, BufferId, BufferState,
    BufferViewStatus, DiskFileState, DocumentChunk, DocumentSnapshot, EncodingSource,
    LineEndingCounts, LineEndingStyle, PersistedHistoryEntry, PieceSource, RestoredBufferState,
    TextArtifactSummary, TextDocument, TextFormatMetadata, TextHistoryBudget, analyze_line_endings,
    display_line_count, platform_default_line_ending, source_label,
};
pub use panes::{PaneBranch, PaneNode, SplitAxis, SplitPath};
pub use tab::WorkspaceTab;
pub use tab_manager::{PendingAction, TabManager};
pub use view::{CursorRevealMode, EditorViewState, LayoutCacheKey, SearchHighlightState, ViewId};
