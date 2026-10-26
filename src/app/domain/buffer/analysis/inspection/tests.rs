use super::{
    LineEndingStyle, TextInspection, TextScanSummary, normalize_inserted_text_line_endings,
};
use std::borrow::Cow;

#[test]
fn inserted_line_endings_preserve_unicode_lone_cr_and_borrowed_fast_paths() {
    for (style, ending) in [
        (LineEndingStyle::Lf, "\n"),
        (LineEndingStyle::Crlf, "\r\n"),
        (LineEndingStyle::Cr, "\r"),
        (LineEndingStyle::None, "\n"),
        (LineEndingStyle::Mixed, "\n"),
    ] {
        for input in ["\r", "\n", "\r\n"] {
            assert_eq!(normalize_inserted_text_line_endings(input, style), ending);
        }
        for input in ["", "λ🦀", "a\rb\r"] {
            assert!(
                matches!(normalize_inserted_text_line_endings(input, style), Cow::Borrowed(text) if text == input)
            );
        }
        for (input, expected) in [
            ("λ\r\n🦀\n尾\r", format!("λ{ending}🦀{ending}尾\r")),
            ("\r\r\n\n", format!("\r{ending}{ending}")),
            ("a\rb\nc", format!("a\rb{ending}c")),
        ] {
            assert_eq!(
                normalize_inserted_text_line_endings(input, style),
                expected,
                "{style:?} {input:?}"
            );
        }
    }
    assert!(matches!(
        normalize_inserted_text_line_endings("λ\n🦀", LineEndingStyle::Lf),
        Cow::Borrowed(_)
    ));
}

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
    let span_refs = ["a\r", "\nb\r", "\nc"].repeat(16);

    let parallel = TextScanSummary::scan_span_slice_parallel_with_workers(&span_refs, 2)
        .expect("parallel scan");
    let serial = TextScanSummary::scan_spans(span_refs.iter().copied());

    assert_eq!(parallel.line_ending_counts, serial.line_ending_counts);
}
