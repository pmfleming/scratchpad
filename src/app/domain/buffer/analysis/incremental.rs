use super::{
    BufferTextMetadata, LineEndingCounts, LineEndingStyle, TextArtifactSummary, TextFormatMetadata,
    TextInspection, buffer_text_metadata_parts, has_non_line_ending_artifacts, line_ending_style,
};

pub(crate) struct IncrementalMetadataEdit<'a> {
    pub(crate) previous_char: Option<char>,
    pub(crate) deleted_text: &'a str,
    pub(crate) inserted_text: &'a str,
    pub(crate) next_char: Option<char>,
}

pub(crate) fn buffer_text_metadata_from_edit(
    line_count: usize,
    artifact_summary: &TextArtifactSummary,
    format: &mut TextFormatMetadata,
    edit: IncrementalMetadataEdit<'_>,
) -> Option<BufferTextMetadata> {
    let deleted_window = boundary_window(edit.previous_char, edit.deleted_text, edit.next_char);
    let inserted_window = boundary_window(edit.previous_char, edit.inserted_text, edit.next_char);
    let deleted_inspection = TextInspection::inspect(&deleted_window);
    let inserted_inspection = TextInspection::inspect(&inserted_window);
    if !can_update_metadata_incrementally(
        artifact_summary,
        &deleted_inspection,
        &inserted_inspection,
    ) {
        return None;
    }

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
    artifact_summary.has_carriage_returns =
        format.line_endings != LineEndingStyle::Cr && line_ending_counts.cr > 0;

    Some(buffer_text_metadata_parts(
        line_count,
        artifact_summary,
        format.preferred_line_ending_style(),
        false,
    ))
}

fn can_update_metadata_incrementally(
    current_summary: &TextArtifactSummary,
    deleted_inspection: &TextInspection,
    inserted_inspection: &TextInspection,
) -> bool {
    !has_non_line_ending_artifacts(current_summary)
        && !has_non_line_ending_artifacts(&deleted_inspection.artifact_summary)
        && !has_non_line_ending_artifacts(&inserted_inspection.artifact_summary)
}

fn boundary_window(previous_char: Option<char>, text: &str, next_char: Option<char>) -> String {
    let mut window = String::with_capacity(
        text.len()
            + usize::from(previous_char.is_some()) * 4
            + usize::from(next_char.is_some()) * 4,
    );
    if let Some(previous_char) = previous_char {
        window.push(previous_char);
    }
    window.push_str(text);
    if let Some(next_char) = next_char {
        window.push(next_char);
    }
    window
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
