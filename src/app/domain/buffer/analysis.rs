use super::{BufferLength, piece_tree::PieceTreeLite};
mod artifacts;
mod format;
pub(crate) use incremental::{
    IncrementalMetadataEdit, IncrementalMetadataUpdate, buffer_text_metadata_from_edit,
};
pub(crate) use inspection::normalize_inserted_text_line_endings;
use inspection::{TextInspection, line_ending_style};
pub(crate) use line_endings::accumulate_line_count;
mod line_endings;

mod incremental;
mod inspection;
#[cfg(test)]
mod tests;
pub use artifacts::TextArtifactSummary;
pub use format::{EncodingSource, TextFormatMetadata};
pub use line_endings::{
    LineEndingCounts, LineEndingStyle, analyze_line_endings, platform_default_line_ending,
};

#[must_use]
pub fn display_line_count(text: &str) -> usize {
    TextInspection::inspect(text).line_count
}

pub(crate) fn display_line_count_from_piece_tree(tree: &PieceTreeLite) -> usize {
    let metrics = tree.metrics();
    if metrics.chars == 0 {
        return 0;
    }

    metrics.newlines + usize::from(tree.char_at(metrics.chars - 1) != Some('\n'))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BufferTextMetadata {
    pub(crate) line_count: usize,
    pub(crate) artifact_summary: TextArtifactSummary,
    pub(crate) preferred_line_ending: LineEndingStyle,
    pub(crate) has_non_compliant_characters: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PieceTreeTextAnalysis {
    pub(crate) metadata: BufferTextMetadata,
    pub(crate) length: BufferLength,
}

pub(crate) fn buffer_text_metadata(
    text: &str,
    format: &mut TextFormatMetadata,
) -> BufferTextMetadata {
    let inspection = TextInspection::inspect(text);
    format.apply_inspection(&inspection);
    build_buffer_text_metadata(
        inspection,
        format,
        format.has_non_compliant_characters(text),
    )
}

pub(crate) fn detected_text_format_and_metadata(
    text: &str,
    encoding_name: String,
    has_bom: bool,
    encoding_source: EncodingSource,
    has_decoding_warnings: bool,
) -> (TextFormatMetadata, BufferTextMetadata) {
    let inspection = TextInspection::inspect(text);
    let format = TextFormatMetadata::from_inspection(
        inspection.clone(),
        encoding_name,
        has_bom,
        encoding_source,
        has_decoding_warnings,
    );
    let metadata = build_buffer_text_metadata(
        inspection,
        &format,
        format.has_non_compliant_characters(text),
    );
    (format, metadata)
}

pub(crate) fn buffer_text_metadata_from_piece_tree(
    tree: &PieceTreeLite,
    format: &mut TextFormatMetadata,
) -> BufferTextMetadata {
    analyze_piece_tree_text(tree, format).metadata
}

pub(crate) fn analyze_piece_tree_text(
    tree: &PieceTreeLite,
    format: &mut TextFormatMetadata,
) -> PieceTreeTextAnalysis {
    let metrics = tree.metrics();
    let spans = tree.spans_for_range(0..metrics.chars);
    let inspection = TextInspection::inspect_span_refs(spans.map(|s| s.text));
    let length = BufferLength::from_metrics(
        metrics,
        display_line_count_from_piece_tree_inspection(&inspection, metrics.newlines),
    );
    format.apply_inspection(&inspection);
    let metadata = build_buffer_text_metadata(inspection, format, false);
    PieceTreeTextAnalysis { metadata, length }
}

fn build_buffer_text_metadata(
    inspection: TextInspection,
    format: &TextFormatMetadata,
    has_non_compliant_characters: bool,
) -> BufferTextMetadata {
    buffer_text_metadata_parts(
        inspection.line_count,
        inspection.artifact_summary,
        format.preferred_line_ending_style(),
        has_non_compliant_characters,
    )
}

fn buffer_text_metadata_parts(
    line_count: usize,
    artifact_summary: TextArtifactSummary,
    preferred_line_ending: LineEndingStyle,
    has_non_compliant_characters: bool,
) -> BufferTextMetadata {
    BufferTextMetadata {
        line_count,
        artifact_summary,
        preferred_line_ending,
        has_non_compliant_characters,
    }
}

fn display_line_count_from_piece_tree_inspection(
    inspection: &TextInspection,
    newline_count: usize,
) -> usize {
    if !inspection.has_bytes {
        return 0;
    }

    newline_count + usize::from(!inspection.ends_with_lf)
}
