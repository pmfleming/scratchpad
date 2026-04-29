use super::{LineEndingCounts, LineEndingStyle, TextArtifactSummary};

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
}

impl TextInspection {
    pub(super) fn inspect(text: &str) -> Self {
        Self::inspect_with_line_endings(text, None)
    }

    pub(super) fn inspect_with_line_endings(
        text: &str,
        line_endings: Option<LineEndingStyle>,
    ) -> Self {
        let mut state = InspectionState::new();
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\r' {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                    state.record_crlf();
                } else {
                    state.record_cr();
                }
                continue;
            }
            state.observe_char(ch);
        }

        state.finish(line_endings)
    }

    pub(super) fn inspect_spans<'a>(spans: impl Iterator<Item = &'a str>) -> Self {
        let mut state = InspectionState::new();

        for span in spans {
            for ch in span.chars() {
                state.observe_span_char(ch);
            }
        }

        state.finish(None)
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

    fn observe_span_char(&mut self, ch: char) {
        if self.pending_cr {
            self.pending_cr = false;
            if ch == '\n' {
                self.record_crlf();
                return;
            }
            self.record_cr();
        }

        if ch == '\r' {
            self.pending_cr = true;
            return;
        }

        self.observe_char(ch);
    }

    fn observe_char(&mut self, ch: char) {
        self.is_ascii_subset &= ch.is_ascii();
        match ch {
            '\n' => self.record_lf(),
            '\u{1B}' => self.artifact_summary.has_ansi_sequences = true,
            '\u{0008}' => self.artifact_summary.has_backspaces = true,
            '\t' => {}
            _ if ch.is_control() => self.artifact_summary.other_control_count += 1,
            _ => {}
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

    fn finish(mut self, line_endings: Option<LineEndingStyle>) -> TextInspection {
        if self.pending_cr {
            self.record_cr();
        }

        let line_endings =
            line_endings.unwrap_or_else(|| line_ending_style(self.line_ending_counts));
        self.artifact_summary.has_carriage_returns =
            line_endings != LineEndingStyle::Cr && self.line_ending_counts.cr > 0;

        TextInspection {
            line_count: self.line_count,
            line_endings,
            line_ending_counts: self.line_ending_counts,
            artifact_summary: self.artifact_summary,
            is_ascii_subset: self.is_ascii_subset,
        }
    }
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
