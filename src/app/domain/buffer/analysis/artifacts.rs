use super::inspection::TextInspection;
use super::line_endings::LineEndingStyle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextArtifactSummary {
    pub has_ansi_sequences: bool,
    pub has_carriage_returns: bool,
    pub has_backspaces: bool,
    pub has_unicode_format_controls: bool,
    pub other_control_count: usize,
    pub(crate) ansi_sequence_count: Option<usize>,
    pub(crate) backspace_count: Option<usize>,
    pub(crate) unicode_format_control_count: Option<usize>,
    pub(crate) other_control_count_exact: Option<usize>,
}

impl Default for TextArtifactSummary {
    fn default() -> Self {
        Self {
            has_ansi_sequences: false,
            has_carriage_returns: false,
            has_backspaces: false,
            has_unicode_format_controls: false,
            other_control_count: 0,
            ansi_sequence_count: Some(0),
            backspace_count: Some(0),
            unicode_format_control_count: Some(0),
            other_control_count_exact: Some(0),
        }
    }
}

impl TextArtifactSummary {
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        TextInspection::inspect(text).artifact_summary
    }

    #[must_use]
    pub fn from_text_with_line_endings(text: &str, line_endings: LineEndingStyle) -> Self {
        TextInspection::inspect_with_line_endings(text, Some(line_endings)).artifact_summary
    }

    #[must_use]
    pub fn has_control_chars(&self) -> bool {
        self.has_ansi_sequences
            || self.has_carriage_returns
            || self.has_backspaces
            || self.has_unicode_format_controls
            || self.other_control_count > 0
    }

    #[must_use]
    pub fn status_text(&self) -> Option<String> {
        if !self.has_control_chars() {
            return None;
        }

        let mut parts = Vec::new();

        if self.has_ansi_sequences {
            parts.push("ANSI");
        }
        if self.has_carriage_returns {
            parts.push("CR");
        }
        if self.has_backspaces {
            parts.push("BS");
        }
        if self.has_unicode_format_controls {
            parts.push("Unicode controls");
        }
        if self.other_control_count > 0 {
            parts.push("CTL");
        }

        Some(format!("Control characters detected: {}", parts.join(", ")))
    }

    pub(crate) fn mark_ansi_sequence(&mut self) {
        self.has_ansi_sequences = true;
        self.ansi_sequence_count = self
            .ansi_sequence_count
            .and_then(|count| count.checked_add(1));
    }

    pub(crate) fn mark_backspace(&mut self) {
        self.has_backspaces = true;
        self.backspace_count = self.backspace_count.and_then(|count| count.checked_add(1));
    }

    pub(crate) fn mark_unicode_format_control(&mut self) {
        self.has_unicode_format_controls = true;
        self.unicode_format_control_count = self
            .unicode_format_control_count
            .and_then(|count| count.checked_add(1));
    }

    pub(crate) fn mark_other_control(&mut self) {
        self.other_control_count = self.other_control_count.saturating_add(1);
        self.other_control_count_exact = self
            .other_control_count_exact
            .and_then(|count| count.checked_add(1));
    }

    pub(crate) fn has_uncertain_non_line_ending_evidence(&self) -> bool {
        self.ansi_sequence_count.is_none()
            || self.backspace_count.is_none()
            || self.unicode_format_control_count.is_none()
            || self.other_control_count_exact.is_none()
    }

    pub(crate) fn apply_non_line_ending_evidence_delta(&mut self, deleted: &Self, inserted: &Self) {
        let has_ansi_sequences =
            self.has_ansi_sequences || deleted.has_ansi_sequences || inserted.has_ansi_sequences;
        let has_backspaces =
            self.has_backspaces || deleted.has_backspaces || inserted.has_backspaces;
        let has_unicode_format_controls = self.has_unicode_format_controls
            || deleted.has_unicode_format_controls
            || inserted.has_unicode_format_controls;
        let other_control_count = self
            .other_control_count
            .max(deleted.other_control_count)
            .max(inserted.other_control_count);
        self.ansi_sequence_count = apply_evidence_count_delta(
            self.ansi_sequence_count,
            deleted.ansi_sequence_count,
            inserted.ansi_sequence_count,
        );
        self.backspace_count = apply_evidence_count_delta(
            self.backspace_count,
            deleted.backspace_count,
            inserted.backspace_count,
        );
        self.unicode_format_control_count = apply_evidence_count_delta(
            self.unicode_format_control_count,
            deleted.unicode_format_control_count,
            inserted.unicode_format_control_count,
        );
        self.other_control_count_exact = apply_evidence_count_delta(
            self.other_control_count_exact,
            deleted.other_control_count_exact,
            inserted.other_control_count_exact,
        );
        self.has_ansi_sequences =
            evidence_count_has_control(self.ansi_sequence_count, has_ansi_sequences);
        self.has_backspaces = evidence_count_has_control(self.backspace_count, has_backspaces);
        self.has_unicode_format_controls = evidence_count_has_control(
            self.unicode_format_control_count,
            has_unicode_format_controls,
        );
        self.other_control_count = self
            .other_control_count_exact
            .unwrap_or_else(|| usize::from(other_control_count > 0));
    }
}

fn apply_evidence_count_delta(
    current: Option<usize>,
    deleted: Option<usize>,
    inserted: Option<usize>,
) -> Option<usize> {
    match (current, deleted, inserted) {
        (Some(current), Some(deleted), Some(inserted)) => {
            current.checked_sub(deleted)?.checked_add(inserted)
        }
        _ => None,
    }
}

fn evidence_count_has_control(count: Option<usize>, previous: bool) -> bool {
    count.map_or(previous, |count| count > 0)
}

#[cfg(test)]
mod tests {
    use super::TextArtifactSummary;

    #[test]
    fn status_text_lists_only_detected_control_kinds() {
        let mut summary = TextArtifactSummary::default();
        summary.mark_ansi_sequence();
        summary.mark_other_control();

        assert_eq!(
            summary.status_text(),
            Some("Control characters detected: ANSI, CTL".to_owned())
        );
    }

    #[test]
    fn evidence_delta_preserves_uncertain_previous_evidence() {
        let mut summary = TextArtifactSummary {
            has_ansi_sequences: true,
            ansi_sequence_count: None,
            ..TextArtifactSummary::default()
        };

        summary.apply_non_line_ending_evidence_delta(
            &TextArtifactSummary::default(),
            &TextArtifactSummary::default(),
        );

        assert!(summary.has_ansi_sequences);
        assert!(summary.has_uncertain_non_line_ending_evidence());
    }
}
