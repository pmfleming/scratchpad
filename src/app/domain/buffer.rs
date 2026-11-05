mod analysis;
mod document;
pub mod edit_tree;
mod history;
mod persistent_vector;
mod piece_tree;
pub use persistent_vector::PersistentVector;
pub(crate) use piece_tree::text_cache;
mod snapshot;
mod state;
pub(crate) use state::register_existing_buffer_id;
pub(crate) use state::{BULK_TEXT_BYTES, PrepareTextRequest, PreparedText};

pub(crate) use analysis::display_line_count_from_piece_tree;
pub(crate) use analysis::{
    BufferTextMetadata, accumulate_line_count, buffer_text_metadata,
    buffer_text_metadata_from_piece_tree, detected_text_format_and_metadata,
    normalize_inserted_text_line_endings, try_analyze_piece_tree_text,
};
pub use analysis::{
    EncodingSource, LineEndingCounts, LineEndingStyle, TextArtifactSummary, TextFormatMetadata,
    analyze_line_endings, display_line_count, platform_default_line_ending,
};
pub(crate) use document::HistorySnapshot;
pub use document::TextDocument;
pub(crate) use document::{DocumentRevision, HistoryToken};
pub(crate) use document::{TextReplacementError, TextReplacements};
pub(crate) use history::{
    ByteSpan, PieceHistoryEdit, PieceHistoryEdits, PieceHistoryEntry, PieceHistoryFlags,
    TEXT_HISTORY_COALESCE_WINDOW, TextDocumentOperationRecord, TextHistoryApplyError,
    fingerprint_parts, next_text_history_global_seq, register_text_history_global_seq,
};
pub use history::{
    OperationRecord, PersistedCursorRange, PersistedHistoryEdit, PersistedHistoryEntry,
    PieceSource, TextDocumentEditOperation, TextHistoryBudget, source_label,
};
pub(crate) use piece_tree::ByteSlice;
pub use piece_tree::{
    AnchorBias, AnchorId, AnchorOwner, AnchorOwnerKind, PieceTreeCharCursor, PieceTreeCharPosition,
    PieceTreeInternalNode, PieceTreeLeaf, PieceTreeLineInfo, PieceTreeLite, PieceTreeMetrics,
    PieceTreeSlice, PieceTreeSpan, PieceTreeText, TextPin, TryPieceTreeSlice,
};
pub(crate) use snapshot::SnapshotRevision;
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
