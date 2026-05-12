use super::{LineEndingCounts, LineEndingStyle, TextArtifactSummary};
use std::borrow::Cow;
use std::ops::Range;
use std::thread;

const PARALLEL_TEXT_INSPECTION_MIN_BYTES: usize = 4 * 1024 * 1024;
const PARALLEL_TEXT_INSPECTION_MAX_WORKERS: usize = 8;

pub(crate) fn normalize_inserted_text_line_endings(
    text: &str,
    preferred_line_ending: LineEndingStyle,
) -> Cow<'_, str> {
    match text {
        "\r" | "\r\n" | "\n" => Cow::Borrowed(preferred_line_ending.as_str()),
        _ if !text.contains('\n') => Cow::Borrowed(text),
        _ if preferred_line_ending == LineEndingStyle::Lf && !text.contains('\r') => {
            Cow::Borrowed(text)
        }
        _ => {
            let replacement = preferred_line_ending.as_str();
            let mut normalized = String::with_capacity(text.len());
            let mut chars = text.chars().peekable();

            while let Some(ch) = chars.next() {
                match ch {
                    '\r' => {
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                            normalized.push_str(replacement);
                        } else {
                            normalized.push(ch);
                        }
                    }
                    '\n' => normalized.push_str(replacement),
                    _ => normalized.push(ch),
                }
            }

            Cow::Owned(normalized)
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct TextInspection {
    pub(super) line_count: usize,
    pub(super) line_endings: LineEndingStyle,
    pub(super) line_ending_counts: LineEndingCounts,
    pub(super) artifact_summary: TextArtifactSummary,
    pub(super) is_ascii_subset: bool,
}

#[derive(Default)]
struct InspectionState {
    line_count: usize,
    line_ending_counts: LineEndingCounts,
    artifact_summary: TextArtifactSummary,
    is_ascii_subset: bool,
    pending_cr: bool,
    starts_with_lf: bool,
    ends_with_cr: bool,
    seen_any: bool,
}

impl TextInspection {
    pub(super) fn inspect(text: &str) -> Self {
        Self::inspect_with_line_endings(text, None)
    }

    pub(super) fn inspect_with_line_endings(
        text: &str,
        line_endings: Option<LineEndingStyle>,
    ) -> Self {
        TextScanSummary::scan_text(text).into_inspection(line_endings)
    }

    pub(super) fn inspect_span_refs<'a>(spans: impl Iterator<Item = &'a str>) -> Self {
        let spans = spans.collect::<Vec<_>>();
        Self::inspect_span_slice(&spans)
    }

    pub(super) fn inspect_span_slice(spans: &[&str]) -> Self {
        TextScanSummary::scan_span_slice(spans).into_inspection(None)
    }
}

impl InspectionState {
    fn new() -> Self {
        Self {
            line_count: 1,
            is_ascii_subset: true,
            ..Self::default()
        }
    }

    fn observe_text(&mut self, text: &str) {
        let bytes = text.as_bytes();
        if let Some(first) = bytes.first() {
            if !self.seen_any {
                self.starts_with_lf = *first == b'\n';
                self.seen_any = true;
            }
            self.ends_with_cr = bytes.last() == Some(&b'\r');
        }

        let mut index = 0usize;

        while index < bytes.len() {
            let byte = bytes[index];

            if self.pending_cr {
                self.pending_cr = false;
                if byte == b'\n' {
                    self.record_crlf();
                    index += 1;
                    continue;
                }
                self.record_cr();
            }

            match byte {
                b'\r' => self.pending_cr = true,
                b'\n' => self.record_lf(),
                b'\x1B' => self.artifact_summary.has_ansi_sequences = true,
                b'\x08' => self.artifact_summary.has_backspaces = true,
                b'\t' => {}
                0x00..=0x1F | 0x7F => self.artifact_summary.other_control_count += 1,
                0x80..=0xFF => self.observe_non_ascii_byte(bytes, index),
                _ => {}
            }

            index += 1;
        }
    }

    fn observe_non_ascii_byte(&mut self, bytes: &[u8], index: usize) {
        self.is_ascii_subset = false;

        if is_c1_control(bytes, index) {
            self.artifact_summary.other_control_count += 1;
        } else if is_unicode_format_control(bytes, index) {
            self.artifact_summary.has_unicode_format_controls = true;
        }
    }

    fn record_crlf(&mut self) {
        self.line_ending_counts.crlf += 1;
        self.line_count += 1;
    }

    fn record_cr(&mut self) {
        self.line_ending_counts.cr += 1;
        self.line_count += 1;
    }

    fn record_lf(&mut self) {
        self.line_ending_counts.lf += 1;
        self.line_count += 1;
    }

    fn finish_summary(mut self) -> TextScanSummary {
        if self.pending_cr {
            self.record_cr();
        }

        TextScanSummary {
            line_ending_counts: self.line_ending_counts,
            artifact_summary: self.artifact_summary,
            is_ascii_subset: self.is_ascii_subset,
            starts_with_lf: self.starts_with_lf,
            ends_with_cr: self.ends_with_cr,
            has_bytes: self.seen_any,
        }
    }
}

#[derive(Default)]
struct TextScanSummary {
    line_ending_counts: LineEndingCounts,
    artifact_summary: TextArtifactSummary,
    is_ascii_subset: bool,
    starts_with_lf: bool,
    ends_with_cr: bool,
    has_bytes: bool,
}

impl TextScanSummary {
    fn scan_text(text: &str) -> Self {
        if let Some(summary) = Self::scan_text_parallel(text) {
            return summary;
        }
        Self::scan_text_serial(text)
    }

    fn scan_text_serial(text: &str) -> Self {
        let mut state = InspectionState::new();
        state.observe_text(text);
        state.finish_summary()
    }

    fn scan_text_parallel(text: &str) -> Option<Self> {
        if text.len() < PARALLEL_TEXT_INSPECTION_MIN_BYTES {
            return None;
        }
        let workers = inspection_worker_count(text.len());
        Self::scan_text_parallel_with_workers(text, workers)
    }

    fn scan_text_parallel_with_workers(text: &str, workers: usize) -> Option<Self> {
        if workers <= 1 {
            return None;
        }

        let ranges = chunk_ranges_for_text(text, workers);
        if ranges.len() <= 1 {
            return None;
        }

        let mut summaries = Vec::with_capacity(ranges.len());
        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(ranges.len());
            for range in ranges {
                handles.push(scope.spawn(move || Self::scan_text_serial(&text[range])));
            }
            for handle in handles {
                if let Ok(summary) = handle.join() {
                    summaries.push(summary);
                }
            }
        });

        Some(Self::combine(summaries))
    }

    fn scan_spans<'a>(spans: impl Iterator<Item = &'a str>) -> Self {
        let mut state = InspectionState::new();
        for span in spans {
            state.observe_text(span);
        }
        state.finish_summary()
    }

    fn scan_span_slice(spans: &[&str]) -> Self {
        match spans {
            [] => Self::scan_spans(std::iter::empty()),
            [span] => Self::scan_text(span),
            _ => Self::scan_span_slice_parallel(spans)
                .unwrap_or_else(|| Self::scan_spans(spans.iter().copied())),
        }
    }

    fn scan_span_slice_parallel(spans: &[&str]) -> Option<Self> {
        let total_bytes = spans.iter().map(|span| span.len()).sum::<usize>();
        if total_bytes < PARALLEL_TEXT_INSPECTION_MIN_BYTES {
            return None;
        }
        let workers = inspection_worker_count(total_bytes).min(spans.len());
        Self::scan_span_slice_parallel_with_workers(spans, workers)
    }

    fn scan_span_slice_parallel_with_workers(spans: &[&str], workers: usize) -> Option<Self> {
        if workers <= 1 {
            return None;
        }

        let chunk_size = spans.len().div_ceil(workers);
        let mut summaries = Vec::with_capacity(workers);
        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for chunk in spans.chunks(chunk_size) {
                handles.push(scope.spawn(move || Self::scan_spans(chunk.iter().copied())));
            }
            for handle in handles {
                if let Ok(summary) = handle.join() {
                    summaries.push(summary);
                }
            }
        });

        Some(Self::combine(summaries))
    }

    fn combine(summaries: Vec<Self>) -> Self {
        let mut combined = Self {
            is_ascii_subset: true,
            ..Self::default()
        };
        let mut previous_ended_with_cr = false;

        for summary in summaries.into_iter().filter(|summary| summary.has_bytes) {
            if !combined.has_bytes {
                combined.starts_with_lf = summary.starts_with_lf;
                combined.has_bytes = true;
            }
            combined.ends_with_cr = summary.ends_with_cr;
            combined.is_ascii_subset &= summary.is_ascii_subset;
            combined.artifact_summary.merge(summary.artifact_summary);
            combined
                .line_ending_counts
                .add_assign(summary.line_ending_counts);
            if previous_ended_with_cr && summary.starts_with_lf {
                combined.line_ending_counts.cr = combined.line_ending_counts.cr.saturating_sub(1);
                combined.line_ending_counts.lf = combined.line_ending_counts.lf.saturating_sub(1);
                combined.line_ending_counts.crlf += 1;
            }
            previous_ended_with_cr = summary.ends_with_cr;
        }

        combined
    }

    fn into_inspection(self, line_endings: Option<LineEndingStyle>) -> TextInspection {
        let line_endings =
            line_endings.unwrap_or_else(|| line_ending_style(self.line_ending_counts));
        let mut artifact_summary = self.artifact_summary;
        artifact_summary.has_carriage_returns =
            line_endings != LineEndingStyle::Cr && self.line_ending_counts.cr > 0;

        TextInspection {
            line_count: 1
                + self.line_ending_counts.lf
                + self.line_ending_counts.crlf
                + self.line_ending_counts.cr,
            line_endings,
            line_ending_counts: self.line_ending_counts,
            artifact_summary,
            is_ascii_subset: self.is_ascii_subset,
        }
    }
}

impl LineEndingCounts {
    fn add_assign(&mut self, other: Self) {
        self.lf += other.lf;
        self.crlf += other.crlf;
        self.cr += other.cr;
    }
}

impl TextArtifactSummary {
    fn merge(&mut self, other: Self) {
        self.has_ansi_sequences |= other.has_ansi_sequences;
        self.has_carriage_returns |= other.has_carriage_returns;
        self.has_backspaces |= other.has_backspaces;
        self.has_unicode_format_controls |= other.has_unicode_format_controls;
        self.other_control_count += other.other_control_count;
    }
}

fn is_c1_control(bytes: &[u8], index: usize) -> bool {
    bytes[index] == 0xC2
        && bytes
            .get(index + 1)
            .is_some_and(|byte| (0x80..=0x9F).contains(byte))
}

fn is_unicode_format_control(bytes: &[u8], index: usize) -> bool {
    match bytes[index] {
        // U+061C ARABIC LETTER MARK
        0xD8 => bytes.get(index + 1) == Some(&0x9C),
        // U+200B..U+200F and U+202A..U+202E
        0xE2 if bytes.get(index + 1) == Some(&0x80) => bytes
            .get(index + 2)
            .is_some_and(|byte| (0x8B..=0x8F).contains(byte) || (0xAA..=0xAE).contains(byte)),
        // U+2060..U+2064 and U+2066..U+206F
        0xE2 if bytes.get(index + 1) == Some(&0x81) => bytes
            .get(index + 2)
            .is_some_and(|byte| (0xA0..=0xA4).contains(byte) || (0xA6..=0xAF).contains(byte)),
        // U+FEFF BYTE ORDER MARK
        0xEF => bytes.get(index + 1) == Some(&0xBB) && bytes.get(index + 2) == Some(&0xBF),
        _ => false,
    }
}

fn inspection_worker_count(total_bytes: usize) -> usize {
    let by_size = (total_bytes / PARALLEL_TEXT_INSPECTION_MIN_BYTES).max(1);
    thread::available_parallelism()
        .map(|parallelism| {
            parallelism
                .get()
                .min(PARALLEL_TEXT_INSPECTION_MAX_WORKERS)
                .min(by_size)
        })
        .unwrap_or(1)
        .max(1)
}

fn chunk_ranges_for_text(text: &str, workers: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::with_capacity(workers);
    let target = text.len().div_ceil(workers);
    let mut start = 0usize;

    while start < text.len() {
        let mut end = (start + target).min(text.len());
        while end < text.len() && !text.is_char_boundary(end) {
            end += 1;
        }
        if end <= start {
            end = text[start..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| start + offset)
                .unwrap_or(text.len());
        }
        ranges.push(start..end);
        start = end;
    }

    ranges
}

pub(super) fn line_ending_style(counts: LineEndingCounts) -> LineEndingStyle {
    let nonzero = [counts.lf > 0, counts.crlf > 0, counts.cr > 0]
        .into_iter()
        .filter(|present| *present)
        .count();
    match nonzero {
        0 => LineEndingStyle::None,
        1 if counts.crlf > 0 => LineEndingStyle::Crlf,
        1 if counts.lf > 0 => LineEndingStyle::Lf,
        1 => LineEndingStyle::Cr,
        _ => LineEndingStyle::Mixed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let span_refs = spans.iter().copied().collect::<Vec<_>>();

        let parallel = TextScanSummary::scan_span_slice_parallel_with_workers(&span_refs, 2)
            .expect("parallel scan");
        let serial = TextScanSummary::scan_spans(span_refs.iter().copied());

        assert_eq!(parallel.line_ending_counts, serial.line_ending_counts);
    }
}
