mod analysis;
mod document;
mod piece_tree;
mod state;

pub(crate) use analysis::{BufferTextMetadata, buffer_text_metadata};
pub use analysis::{
    EncodingSource, LineEndingCounts, LineEndingStyle, TextArtifactSummary, TextFormatMetadata,
    analyze_line_endings, display_line_count, platform_default_line_ending,
};
pub use document::{TextDocument, TextDocumentUndoState, TextDocumentUndoer};
pub(crate) use document::{TextReplacementError, TextReplacements};
pub use piece_tree::{PieceTreeInternalNode, PieceTreeLeaf, PieceTreeLite, PieceTreeMetrics};
pub use state::{BufferFreshness, BufferId, BufferState, DiskFileState, RestoredBufferState};

use std::sync::Arc;

#[derive(Clone)]
pub struct RenderedLayout {
    pub galley: Arc<eframe::egui::Galley>,
    pub row_line_numbers: Vec<Option<usize>>,
}

impl RenderedLayout {
    pub fn from_galley(galley: Arc<eframe::egui::Galley>) -> Self {
        let row_line_numbers = row_line_numbers_for_galley(&galley);
        Self {
            galley,
            row_line_numbers,
        }
    }

    pub fn visual_row_count(&self) -> usize {
        self.row_line_numbers.len().max(1)
    }
}

fn row_line_numbers_for_galley(galley: &eframe::egui::Galley) -> Vec<Option<usize>> {
    let mut current_line = 1usize;
    let mut starts_new_line = true;
    let mut row_line_numbers = Vec::with_capacity(galley.rows.len());

    for row in &galley.rows {
        row_line_numbers.push(starts_new_line.then_some(current_line));
        starts_new_line = row.ends_with_newline;
        if row.ends_with_newline {
            current_line += 1;
        }
    }

    row_line_numbers
}
