use super::{
    BufferTextMetadata, LineEndingCounts, LineEndingStyle, TextArtifactSummary, TextFormatMetadata,
    TextInspection, buffer_text_metadata_parts, line_ending_style,
};

pub(crate) struct IncrementalMetadataEdit<'a> {
    pub(crate) previous_char: Option<char>,
    pub(crate) deleted_text: &'a str,
    pub(crate) inserted_text: &'a str,
    pub(crate) next_char: Option<char>,
}

pub(crate) struct IncrementalMetadataUpdate {
    pub(crate) metadata: BufferTextMetadata,
    pub(crate) needs_background_rescan: bool,
}

pub(crate) fn buffer_text_metadata_from_edit(
    line_count: usize,
    artifact_summary: &TextArtifactSummary,
    format: &mut TextFormatMetadata,
    edit: IncrementalMetadataEdit<'_>,
) -> Option<IncrementalMetadataUpdate> {
    let deleted_inspection =
        inspect_edit_window(edit.previous_char, edit.deleted_text, edit.next_char);
    let inserted_inspection =
        inspect_edit_window(edit.previous_char, edit.inserted_text, edit.next_char);

    let deleted_breaks = deleted_inspection.line_count.saturating_sub(1);
    let inserted_breaks = inserted_inspection.line_count.saturating_sub(1);
    let line_count = line_count
        .checked_sub(deleted_breaks)?
        .checked_add(inserted_breaks)?;

    let mut line_ending_counts = format.line_ending_counts;
    apply_line_ending_delta(
        &mut line_ending_counts,
        deleted_inspection.line_ending_counts,
        inserted_inspection.line_ending_counts,
    )?;

    format.line_ending_counts = line_ending_counts;
    format.line_endings = line_ending_style(line_ending_counts);
    format.is_ascii_subset &= inserted_inspection.is_ascii_subset;

    let mut artifact_summary = artifact_summary.clone();
    let deleted_artifacts = TextArtifactSummary::from_text(edit.deleted_text);
    let inserted_artifacts = TextArtifactSummary::from_text(edit.inserted_text);
    artifact_summary.apply_non_line_ending_evidence_delta(&deleted_artifacts, &inserted_artifacts);
    artifact_summary.has_carriage_returns =
        format.line_endings != LineEndingStyle::Cr && line_ending_counts.cr > 0;
    let needs_background_rescan = artifact_summary.has_uncertain_non_line_ending_evidence();

    Some(IncrementalMetadataUpdate {
        metadata: buffer_text_metadata_parts(
            line_count,
            artifact_summary,
            format.preferred_line_ending_style(),
            false,
        ),
        needs_background_rescan,
    })
}

fn inspect_edit_window(
    previous_char: Option<char>,
    text: &str,
    next_char: Option<char>,
) -> TextInspection {
    let mut previous_storage = [0_u8; 4];
    let mut next_storage = [0_u8; 4];
    let previous_text = previous_char.map(|ch| ch.encode_utf8(&mut previous_storage) as &str);
    let next_text = next_char.map(|ch| ch.encode_utf8(&mut next_storage) as &str);
    TextInspection::inspect_span_refs(
        previous_text
            .into_iter()
            .chain(std::iter::once(text))
            .chain(next_text),
    )
}

fn apply_line_ending_delta(
    line_ending_counts: &mut LineEndingCounts,
    deleted_counts: LineEndingCounts,
    inserted_counts: LineEndingCounts,
) -> Option<()> {
    line_ending_counts.lf = line_ending_counts
        .lf
        .checked_sub(deleted_counts.lf)?
        .checked_add(inserted_counts.lf)?;
    line_ending_counts.crlf = line_ending_counts
        .crlf
        .checked_sub(deleted_counts.crlf)?
        .checked_add(inserted_counts.crlf)?;
    line_ending_counts.cr = line_ending_counts
        .cr
        .checked_sub(deleted_counts.cr)?
        .checked_add(inserted_counts.cr)?;
    Some(())
}
