use super::{BufferLength, piece_tree::PieceTreeLite};
mod artifacts;
mod format;
pub(crate) use incremental::{
    IncrementalMetadataEdit, IncrementalMetadataUpdate, buffer_text_metadata_from_edit,
};
pub(crate) use inspection::normalize_inserted_text_line_endings;
use inspection::{TextInspection, line_ending_style};
mod line_endings;

mod incremental;
mod inspection;
#[cfg(test)]
mod tests;
pub use artifacts::TextArtifactSummary;
pub use format::{EncodingSource, TextFormatMetadata};
pub use line_endings::{LineEndingCounts, LineEndingStyle, platform_default_line_ending};

pub(crate) fn display_line_count_from_piece_tree(tree: &PieceTreeLite) -> usize {
    tree.display_line_count()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BufferTextMetadata {
    pub(crate) line_count: usize,
    pub(crate) artifact_summary: TextArtifactSummary,
    pub(crate) preferred_line_ending: LineEndingStyle,
    pub(crate) has_non_compliant_characters: bool,
}

/// Inspects imported UTF-8 incrementally without retaining or rereading bodies.
pub(crate) struct StreamingTextInspector(inspection::StreamingInspectionState);
impl StreamingTextInspector {
    pub(crate) fn new() -> Self {
        Self(inspection::StreamingInspectionState::new())
    }
    pub(crate) fn observe(&mut self, text: &str) {
        self.0.observe(text);
    }
    pub(crate) fn finish(self) -> StreamedTextInspection {
        let (inspection, digest) = self.0.finish();
        StreamedTextInspection { inspection, digest }
    }
}
pub(crate) struct StreamedTextInspection {
    inspection: TextInspection,
    pub(crate) digest: String,
}
impl StreamedTextInspection {
    pub(crate) fn into_metadata(
        self,
        encoding_name: String,
        has_bom: bool,
        source: EncodingSource,
        warnings: bool,
    ) -> (TextFormatMetadata, BufferTextMetadata, String) {
        let format = TextFormatMetadata::from_inspection(
            self.inspection.clone(),
            encoding_name,
            has_bom,
            source,
            warnings,
        );
        let metadata = build_buffer_text_metadata(self.inspection, &format, false);
        (format, metadata, self.digest)
    }
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
) -> std::io::Result<BufferTextMetadata> {
    Ok(try_analyze_piece_tree_text(tree, format)?.metadata)
}

pub(crate) fn try_analyze_piece_tree_text(
    tree: &PieceTreeLite,
    format: &mut TextFormatMetadata,
) -> std::io::Result<PieceTreeTextAnalysis> {
    let metrics = tree.metrics();
    let spans = tree.try_spans_for_range(0..metrics.chars);
    let inspection = TextInspection::try_inspect_span_refs(spans.map(|s| s.map(|s| s.text)))?;
    let length = BufferLength::from_metrics(
        metrics,
        display_line_count_from_piece_tree_inspection(&inspection, metrics.newlines),
    );
    format.apply_inspection(&inspection);
    let metadata = build_buffer_text_metadata(inspection, format, false);
    Ok(PieceTreeTextAnalysis { metadata, length })
}

/// Inspect the logical replacement result using the original spans and the
/// already-owned normalized input. Newly sealed bodies need not be read back.
pub(crate) fn metadata_after_replacements(
    tree: &PieceTreeLite,
    replacements: &[(std::ops::Range<usize>, String)],
    format: &mut TextFormatMetadata,
) -> std::io::Result<BufferTextMetadata> {
    let end = tree.len_chars();
    let mut previous_end = 0;
    for (range, _) in replacements.iter().rev() {
        if range.start < previous_end || range.start > range.end || range.end > end {
            return Err(std::io::Error::other("invalid replacement metadata ranges"));
        }
        previous_end = range.end;
    }
    let final_range = end..end;
    let mut previous_end = 0;
    let spans = replacements
        .iter()
        .rev()
        .map(|(range, text)| (range, text.as_str()))
        .chain(std::iter::once((&final_range, "")))
        .flat_map(|(range, text)| {
            let original = tree
                .try_spans_for_range(previous_end..range.start)
                .map(|span| span.map(|span| span.text));
            previous_end = range.end;
            original.chain(std::iter::once(Ok(super::PieceTreeText::borrowed(text))))
        });
    let inspection = TextInspection::try_inspect_span_refs(spans)?;
    format.apply_inspection(&inspection);
    Ok(build_buffer_text_metadata(inspection, format, false))
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
