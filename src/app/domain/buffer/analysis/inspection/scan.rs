use super::line_ending_style;
use super::{LineEndingCounts, LineEndingStyle, TextArtifactSummary};

mod parallel;

pub(in crate::app::domain::buffer::analysis) struct StreamingInspectionState {
    state: InspectionState,
    digest: blake3::Hasher,
}
impl StreamingInspectionState {
    pub(in crate::app::domain::buffer::analysis) fn new() -> Self {
        Self {
            state: InspectionState::new(),
            digest: blake3::Hasher::new(),
        }
    }
    pub(in crate::app::domain::buffer::analysis) fn observe(&mut self, text: &str) {
        self.state.observe_text(text);
        self.digest.update(text.as_bytes());
    }
    pub(in crate::app::domain::buffer::analysis) fn finish(self) -> (TextInspection, String) {
        (
            self.state.finish_summary().into_inspection(None),
            self.digest.finalize().to_hex().to_string(),
        )
    }
}

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
    pub(in crate::app::domain::buffer::analysis) fn try_inspect_span_refs(
        spans: impl Iterator<Item = std::io::Result<impl AsRef<str> + Sync>>,
    ) -> std::io::Result<Self> {
        let mut summary = TextScanSummary::scan_text_serial("");
        let mut batch = Vec::new();
        let mut bytes = 0;
        for span in spans {
            let span = span?;
            bytes += span.as_ref().len();
            batch.push(span);
            if bytes >= 4 * 1024 * 1024 {
                let next = TextScanSummary::scan_span_slice_parallel(&batch)
                    .unwrap_or_else(|| TextScanSummary::scan_spans(batch.iter()));
                summary = TextScanSummary::combine(vec![summary, next]);
                batch.clear();
                bytes = 0;
            }
        }
        if !batch.is_empty() {
            let next = TextScanSummary::scan_span_slice_parallel(&batch)
                .unwrap_or_else(|| TextScanSummary::scan_spans(batch.iter()));
            summary = TextScanSummary::combine(vec![summary, next]);
        }
        Ok(summary.into_inspection(None))
    }

    pub(in crate::app::domain::buffer::analysis) fn inspect(text: &str) -> Self {
        Self::inspect_with_line_endings(text, None)
    }

    pub(in crate::app::domain::buffer::analysis) fn inspect_with_line_endings(
        text: &str,
        line_endings: Option<LineEndingStyle>,
    ) -> Self {
        TextScanSummary::scan_text(text).into_inspection(line_endings)
    }

    pub(in crate::app::domain::buffer::analysis) fn inspect_span_refs(
        spans: impl Iterator<Item = impl AsRef<str> + Sync>,
    ) -> Self {
        TextScanSummary::scan_span_refs(spans).into_inspection(None)
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
        self.observe_edges(bytes);
        if bytes.len() >= 64 && ordinary_utf8(bytes) {
            self.observe_ordinary_utf8(bytes);
            return;
        }

        let mut index = 0usize;
        while index < bytes.len() {
            let byte = bytes[index];
            if self.resolve_pending_cr(byte) {
                index += 1;
                continue;
            }
            let printable = printable_ascii_run(&bytes[index..]);
            if printable > 0 {
                index += printable;
                continue;
            }
            match byte {
                b'\r' => self.pending_cr = true,
                b'\n' => self.record_lf(),
                b'\x1B' => self.artifact_summary.mark_ansi_sequence(),
                b'\x08' => self.artifact_summary.mark_backspace(),
                b'\t' => {}
                0x00..=0x1F | 0x7F => self.artifact_summary.mark_other_control(),
                0x80..=0xFF => self.observe_non_ascii_byte(bytes, index),
                _ => {}
            }
            index += 1;
        }
    }

    /// Tracks the first and last bytes seen, which decide leading-LF and
    /// trailing CR/LF joins across spans.
    fn observe_edges(&mut self, bytes: &[u8]) {
        let (Some(&first), Some(&last)) = (bytes.first(), bytes.last()) else {
            return;
        };
        if !self.seen_any {
            self.starts_with_lf = first == b'\n';
            self.seen_any = true;
        }
        self.ends_with_cr = last == b'\r';
        self.ends_with_lf = last == b'\n';
    }

    /// Counts a span without CR or artifact bytes with whole-slice passes.
    fn observe_ordinary_utf8(&mut self, bytes: &[u8]) {
        let mut lines = memchr::memchr_iter(b'\n', bytes).count();
        if self.resolve_pending_cr(bytes[0]) {
            lines -= 1;
        }
        self.line_count += lines;
        self.line_ending_counts.lf += lines;
        self.is_ascii_subset &= bytes.is_ascii();
    }

    /// Settles a CR left pending by the previous byte. Returns true when `next`
    /// is its LF, which then belongs to the recorded CRLF.
    fn resolve_pending_cr(&mut self, next: u8) -> bool {
        if !self.pending_cr {
            return false;
        }
        self.pending_cr = false;
        if next == b'\n' {
            self.record_crlf();
            return true;
        }
        self.record_cr();
        false
    }

    fn observe_non_ascii_byte(&mut self, bytes: &[u8], index: usize) {
        self.is_ascii_subset = false;
        if is_c1_control(bytes, index) {
            self.artifact_summary.mark_other_control();
        } else if is_unicode_format_control(bytes, index) {
            self.artifact_summary.mark_unicode_format_control();
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

// Scan the common UTF-8 case with independent vectorizable passes. CR and
// artifact lead bytes retain the existing decoder, including cross-span CRLF.
// Masks combine as `u8` bit operations so the reduction stays branch-free.
fn ordinary_utf8(bytes: &[u8]) -> bool {
    let controls = bytes.iter().fold(0u8, |mask, &byte| {
        mask | (u8::from(byte < b' ') & u8::from(byte.wrapping_sub(b'\t') > 1))
            | u8::from(byte == 0x7f)
    });
    controls == 0
        && memchr::memchr3(0xc2, 0xd8, 0xe2, bytes).is_none()
        && memchr::memchr(0xef, bytes).is_none()
}

/// Length of the printable-ASCII block or word that starts `bytes`, or 0.
/// The reduction over 32 bytes can vectorize without platform-specific
/// intrinsics. Controls and UTF-8 retain the scalar decoder.
fn printable_ascii_run(bytes: &[u8]) -> usize {
    if !bytes
        .first()
        .is_some_and(|byte| (b' '..=b'~').contains(byte))
    {
        return 0;
    }
    if bytes.first_chunk::<32>().is_some_and(printable_ascii_block) {
        return 32;
    }
    if bytes
        .first_chunk::<8>()
        .is_some_and(|word| printable_ascii_word(*word))
    {
        return 8;
    }
    0
}

fn printable_ascii_word(bytes: [u8; 8]) -> bool {
    const HIGH: u64 = 0x8080_8080_8080_8080;
    let word = u64::from_ne_bytes(bytes);
    word & HIGH == 0
        && word.wrapping_add(0x6060_6060_6060_6060) & HIGH == HIGH
        && word.wrapping_add(0x0101_0101_0101_0101) & HIGH == 0
}

fn printable_ascii_block(bytes: &[u8; 32]) -> bool {
    bytes.iter().fold(0u8, |mask, &byte| {
        mask | u8::from(!(b' '..=b'~').contains(&byte))
    }) == 0
}

#[cfg(test)]
mod fast_scan_tests {
    use super::{printable_ascii_block, printable_ascii_word};

    #[test]
    fn every_byte_and_position_preserves_control_and_utf8_detection() {
        for padding in [b' ', b'~', b'a'] {
            for index in 0..32 {
                for byte in 0..=255 {
                    let mut block = [padding; 32];
                    block[index] = byte;
                    assert_eq!(printable_ascii_block(&block), (b' '..=b'~').contains(&byte));
                }
            }
            for index in 0..8 {
                for byte in 0..=255 {
                    let mut word = [padding; 8];
                    word[index] = byte;
                    assert_eq!(printable_ascii_word(word), (b' '..=b'~').contains(&byte));
                }
            }
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

    pub(super) fn scan_spans(spans: impl Iterator<Item = impl AsRef<str>>) -> Self {
        let mut state = InspectionState::new();
        for span in spans {
            state.observe_text(span.as_ref());
        }
        state.finish_summary()
    }

    pub(super) fn scan_span_refs(spans: impl Iterator<Item = impl AsRef<str> + Sync>) -> Self {
        let mut state = InspectionState::new();
        let mut total_bytes = 0usize;
        let mut spans = spans;

        while let Some(span) = spans.next() {
            state.observe_text(span.as_ref());
            total_bytes += span.as_ref().len();
            if total_bytes >= parallel::parallel_inspection_min_bytes() {
                let prefix = state.finish_summary();
                let remaining = spans.collect::<Vec<_>>();
                if remaining.is_empty() {
                    return prefix;
                }
                let suffix = Self::scan_span_slice_parallel(&remaining)
                    .unwrap_or_else(|| Self::scan_spans(remaining.iter()));
                return Self::combine(vec![prefix, suffix]);
            }
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

    fn scan_span_slice_parallel<T: AsRef<str> + Sync>(spans: &[T]) -> Option<Self> {
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
        self.ansi_sequence_count =
            add_optional_counts(self.ansi_sequence_count, other.ansi_sequence_count);
        self.backspace_count = add_optional_counts(self.backspace_count, other.backspace_count);
        self.unicode_format_control_count = add_optional_counts(
            self.unicode_format_control_count,
            other.unicode_format_control_count,
        );
        self.other_control_count_exact = add_optional_counts(
            self.other_control_count_exact,
            other.other_control_count_exact,
        );
    }
}

fn add_optional_counts(left: Option<usize>, right: Option<usize>) -> Option<usize> {
    left?.checked_add(right?)
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
