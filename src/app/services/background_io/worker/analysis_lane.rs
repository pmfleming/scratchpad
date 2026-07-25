use super::{LaneEndpoints, LaneOutcome, spawn_lane};
use crate::app::capacity_metrics::BackgroundIoLane;
use crate::app::domain::buffer::BufferLength;
use crate::app::domain::{DocumentSnapshot, TextArtifactSummary, TextFormatMetadata};
use crate::app::services::background_io::types::{BackgroundIoRequest, BackgroundIoResult};

pub(in crate::app::services::background_io) fn spawn_analysis_lane(endpoints: LaneEndpoints) {
    spawn_lane(
        BackgroundIoLane::Analysis,
        endpoints.request_rx,
        endpoints.result_tx,
        endpoints.lane_depths,
        |request, _| match request {
            BackgroundIoRequest::RefreshTextMetadata {
                request_id,
                buffer_id,
                revision,
                snapshot,
                format,
            } => LaneOutcome::result(BackgroundIoResult::TextMetadataRefreshed {
                request_id,
                buffer_id,
                revision,
                result: Ok(refresh_text_metadata(snapshot, format)),
            }),
            BackgroundIoRequest::RefreshEncodingCompliance {
                request_id,
                buffer_id,
                revision,
                snapshot,
                format,
            } => LaneOutcome::result(BackgroundIoResult::EncodingComplianceRefreshed {
                request_id,
                buffer_id,
                revision,
                result: Ok(refresh_encoding_compliance(snapshot, format)),
            }),
            _ => LaneOutcome::Skip,
        },
    );
}

fn refresh_text_metadata(
    snapshot: DocumentSnapshot,
    mut format: TextFormatMetadata,
) -> (BufferLength, usize, TextArtifactSummary, TextFormatMetadata) {
    let analysis =
        crate::app::domain::buffer::analyze_piece_tree_text(snapshot.piece_tree(), &mut format);
    (
        analysis.length,
        analysis.metadata.line_count,
        analysis.metadata.artifact_summary,
        format,
    )
}

fn refresh_encoding_compliance(snapshot: DocumentSnapshot, format: TextFormatMetadata) -> bool {
    format.has_non_compliant_characters_spans(
        snapshot
            .piece_tree()
            .spans_for_range(0..snapshot.document_length().chars)
            .map(|span| span.text),
    )
}

#[cfg(test)]
mod tests {
    use super::{refresh_encoding_compliance, refresh_text_metadata};
    use crate::app::domain::{BufferState, EncodingSource, TextDocument, TextFormatMetadata};

    #[test]
    fn refresh_text_metadata_reports_lines_and_control_artifacts() {
        let buffer = BufferState::new("sample.txt".to_owned(), "one\n\u{200e}two".to_owned(), None);
        let snapshot = buffer.document_snapshot();
        let format = buffer.format;

        let (length, line_count, artifact_summary, refreshed_format) =
            refresh_text_metadata(snapshot, format);

        assert_eq!(length.lines, 2);
        assert_eq!(length.chars, "one\n\u{200e}two".chars().count());
        assert_eq!(line_count, 2);
        assert!(artifact_summary.has_control_chars());
        assert_eq!(refreshed_format.preferred_line_ending.as_str(), "\n");
    }

    #[test]
    fn refresh_encoding_compliance_detects_unencodable_snapshot_text() {
        let document = TextDocument::new("plain 😀".to_owned());
        let snapshot = document.snapshot();
        let format = TextFormatMetadata::detected(
            "plain",
            "windows-1252".to_owned(),
            false,
            EncodingSource::Heuristic,
            false,
        );

        assert!(refresh_encoding_compliance(snapshot, format));
    }
}
