use super::{LineEndingStyle, TextInspection, TextScanSummary};

#[test]
fn byte_inspection_preserves_crlf_across_spans() {
    let inspection = TextInspection::inspect_span_refs(["a\r", "\nb\rc\n"].into_iter());

    assert_eq!(inspection.line_count, 4);
    assert_eq!(inspection.line_ending_counts.crlf, 1);
    assert_eq!(inspection.line_ending_counts.cr, 1);
    assert_eq!(inspection.line_ending_counts.lf, 1);
    assert_eq!(inspection.line_endings, LineEndingStyle::Mixed);
}

#[test]
fn byte_inspection_detects_unicode_and_c1_controls() {
    let inspection = TextInspection::inspect("a\u{200B}b\u{0085}c\u{061C}");

    assert!(!inspection.is_ascii_subset);
    assert!(inspection.artifact_summary.has_unicode_format_controls);
    assert_eq!(inspection.artifact_summary.other_control_count, 1);
}

#[test]
fn parallel_text_inspection_matches_serial_for_boundary_cases() {
    let text = format!(
        "{}\r\n{}\r{}",
        "alpha\n".repeat(512 * 1024),
        "beta\u{200B}".repeat(512 * 1024),
        "gamma\u{0085}\n".repeat(128 * 1024)
    );

    let parallel =
        TextScanSummary::scan_text_parallel_with_workers(&text, 2).expect("parallel scan");
    let serial = TextScanSummary::scan_text_serial(&text);

    assert_eq!(parallel.line_ending_counts, serial.line_ending_counts);
    assert_eq!(parallel.is_ascii_subset, serial.is_ascii_subset);
    assert_eq!(
        parallel.artifact_summary.has_unicode_format_controls,
        serial.artifact_summary.has_unicode_format_controls
    );
    assert_eq!(
        parallel.artifact_summary.other_control_count,
        serial.artifact_summary.other_control_count
    );
}

#[test]
fn parallel_span_inspection_corrects_crlf_worker_boundaries() {
    let spans = ["a\r", "\nb\r", "\nc"].repeat(16);
    let span_refs = spans.to_vec();

    let parallel = TextScanSummary::scan_span_slice_parallel_with_workers(&span_refs, 2)
        .expect("parallel scan");
    let serial = TextScanSummary::scan_spans(span_refs.iter().copied());

    assert_eq!(parallel.line_ending_counts, serial.line_ending_counts);
}
