mod analysis;
mod document;
mod piece_tree;
mod snapshot;
mod state;

pub(crate) use analysis::display_line_count_from_piece_tree;
pub(crate) use analysis::{
    BufferTextMetadata, buffer_text_metadata, buffer_text_metadata_from_piece_tree,
    detected_text_format_and_metadata,
};
pub use analysis::{
    EncodingSource, LineEndingCounts, LineEndingStyle, TextArtifactSummary, TextFormatMetadata,
    analyze_line_endings, display_line_count, platform_default_line_ending,
};
pub use document::TextDocument;
pub(crate) use document::{
    TextDocumentEditOperation, TextDocumentOperationRecord, TextHistoryApplyError,
    TextReplacementError, TextReplacements,
};
pub use piece_tree::{
    AnchorBias, AnchorId, AnchorOwner, AnchorOwnerKind, PieceTreeCharPosition,
    PieceTreeInternalNode, PieceTreeLeaf, PieceTreeLineInfo, PieceTreeLite, PieceTreeMetrics,
    PieceTreeSlice, PieceTreeSpan,
};
pub use snapshot::{DocumentChunk, DocumentSnapshot};
pub(crate) use state::TextHistoryEvent;
pub use state::{
    BufferFreshness, BufferId, BufferState, BufferViewStatus, DiskFileState, RestoredBufferState,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BufferLength {
    pub(crate) bytes: usize,
    pub(crate) chars: usize,
    pub(crate) lines: usize,
}

impl BufferLength {
    pub(crate) fn from_metrics(metrics: PieceTreeMetrics, lines: usize) -> Self {
        Self {
            bytes: metrics.bytes,
            chars: metrics.chars,
            lines,
        }
    }
}
