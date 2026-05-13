use super::line_ending_style;
use super::{LineEndingCounts, LineEndingStyle, TextArtifactSummary};

mod parallel;

#[derive(Clone, Debug)]
pub(in crate::app::domain::buffer::analysis) struct TextInspection {
    pub(in crate::app::domain::buffer::analysis) line_count: usize,
    pub(in crate::app::domain::buffer::analysis) line_endings: LineEndingStyle,
    pub(in crate::app::domain::buffer::analysis) line_ending_counts: LineEndingCounts,
    pub(in crate::app::domain::buffer::analysis) artifact_summary: TextArtifactSummary,
    pub(in crate::app::domain::buffer::analysis) is_ascii_subset: bool,
    pub(in crate::app::domain::buffer::analysis) has_bytes: bool,
    pub(in crate::app::domain::buffer::analysis) ends_with_lf: bool,
}

impl TextInspection {
    pub(in crate::app::domain::buffer::analysis) fn inspect(text: &str) -> Self {
        Self::inspect_with_line_endings(text, None)
    }

    pub(in crate::app::domain::buffer::analysis) fn inspect_with_line_endings(
        text: &str,
        line_endings: Option<LineEndingStyle>,
    ) -> Self {
        TextScanSummary::scan_text(text).into_inspection(line_endings)
    }

    pub(in crate::app::domain::buffer::analysis) fn inspect_span_refs<'a>(
        spans: impl Iterator<Item = &'a str>,
    ) -> Self {
        let spans = spans.collect::<Vec<_>>();
        Self::inspect_span_slice(&spans)
    }

    pub(in crate::app::domain::buffer::analysis) fn inspect_span_slice(spans: &[&str]) -> Self {
        TextScanSummary::scan_span_slice(spans).into_inspection(None)
    }
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
    ends_with_lf: bool,
    seen_any: bool,
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
            self.ends_with_lf = bytes.last() == Some(&b'\n');
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
            ends_with_lf: self.ends_with_lf,
            has_bytes: self.seen_any,
        }
    }
}

#[derive(Default)]
pub(super) struct TextScanSummary {
    pub(super) line_ending_counts: LineEndingCounts,
    pub(super) artifact_summary: TextArtifactSummary,
    pub(super) is_ascii_subset: bool,
    starts_with_lf: bool,
    ends_with_cr: bool,
    ends_with_lf: bool,
    has_bytes: bool,
}

impl TextScanSummary {
    pub(super) fn scan_text_serial(text: &str) -> Self {
        let mut state = InspectionState::new();
        state.observe_text(text);
        state.finish_summary()
    }

    #[cfg(test)]
    pub(super) fn scan_text_parallel_with_workers(text: &str, workers: usize) -> Option<Self> {
        parallel::scan_text_with_workers(text, workers)
    }

    pub(super) fn scan_spans<'a>(spans: impl Iterator<Item = &'a str>) -> Self {
        let mut state = InspectionState::new();
        for span in spans {
            state.observe_text(span);
        }
        state.finish_summary()
    }

    #[cfg(test)]
    pub(super) fn scan_span_slice_parallel_with_workers(
        spans: &[&str],
        workers: usize,
    ) -> Option<Self> {
        parallel::scan_span_slice_with_workers(spans, workers)
    }

    fn scan_text(text: &str) -> Self {
        Self::scan_text_parallel(text).unwrap_or_else(|| Self::scan_text_serial(text))
    }

    fn scan_text_parallel(text: &str) -> Option<Self> {
        parallel::scan_text(text)
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
        parallel::scan_span_slice(spans)
    }

    pub(super) fn combine(summaries: Vec<Self>) -> Self {
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
            combined.ends_with_lf = summary.ends_with_lf;
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
            has_bytes: self.has_bytes,
            ends_with_lf: self.ends_with_lf,
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
        0xD8 => bytes.get(index + 1) == Some(&0x9C),
        0xE2 if bytes.get(index + 1) == Some(&0x80) => bytes
            .get(index + 2)
            .is_some_and(|byte| (0x8B..=0x8F).contains(byte) || (0xAA..=0xAE).contains(byte)),
        0xE2 if bytes.get(index + 1) == Some(&0x81) => bytes
            .get(index + 2)
            .is_some_and(|byte| (0xA0..=0xA4).contains(byte) || (0xA6..=0xAF).contains(byte)),
        0xEF => bytes.get(index + 1) == Some(&0xBB) && bytes.get(index + 2) == Some(&0xBF),
        _ => false,
    }
}
