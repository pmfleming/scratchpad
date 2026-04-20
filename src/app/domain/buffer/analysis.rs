use encoding_rs::Encoding;
use serde::{Deserialize, Serialize};

use super::piece_tree::PieceTreeLite;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEndingStyle {
    #[default]
    None,
    Lf,
    Crlf,
    Cr,
    Mixed,
}

impl LineEndingStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
            Self::Cr => "CR",
            Self::Mixed => "Mixed",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None | Self::Lf | Self::Mixed => "\n",
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineEndingCounts {
    pub lf: usize,
    pub crlf: usize,
    pub cr: usize,
}

impl LineEndingCounts {
    fn dominant_style(self) -> Option<LineEndingStyle> {
        let mut entries = [
            (self.crlf, LineEndingStyle::Crlf),
            (self.lf, LineEndingStyle::Lf),
            (self.cr, LineEndingStyle::Cr),
        ];
        entries.sort_by(|left, right| right.0.cmp(&left.0));
        (entries[0].0 > 0).then_some(entries[0].1)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextArtifactSummary {
    pub has_ansi_sequences: bool,
    pub has_carriage_returns: bool,
    pub has_backspaces: bool,
    pub other_control_count: usize,
}

#[derive(Clone, Debug)]
struct TextInspection {
    line_count: usize,
    line_endings: LineEndingStyle,
    line_ending_counts: LineEndingCounts,
    artifact_summary: TextArtifactSummary,
    is_ascii_subset: bool,
}

impl TextInspection {
    fn inspect(text: &str) -> Self {
        Self::inspect_with_line_endings(text, None)
    }

    fn inspect_with_line_endings(text: &str, line_endings: Option<LineEndingStyle>) -> Self {
        let mut line_count = 1usize;
        let mut line_ending_counts = LineEndingCounts::default();
        let mut artifact_summary = TextArtifactSummary::default();
        let mut is_ascii_subset = true;
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            is_ascii_subset &= ch.is_ascii();
            match ch {
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        line_ending_counts.crlf += 1;
                        chars.next();
                    } else {
                        line_ending_counts.cr += 1;
                    }
                    line_count += 1;
                }
                '\n' => {
                    line_ending_counts.lf += 1;
                    line_count += 1;
                }
                '\u{1B}' => {
                    artifact_summary.has_ansi_sequences = true;
                }
                '\u{0008}' => {
                    artifact_summary.has_backspaces = true;
                }
                '\t' => {}
                _ if ch.is_control() => {
                    artifact_summary.other_control_count += 1;
                }
                _ => {}
            }
        }

        let line_endings = line_endings.unwrap_or_else(|| line_ending_style(line_ending_counts));
        artifact_summary.has_carriage_returns =
            line_endings != LineEndingStyle::Cr && line_ending_counts.cr > 0;

        Self {
            line_count,
            line_endings,
            line_ending_counts,
            artifact_summary,
            is_ascii_subset,
        }
    }

    fn inspect_spans<'a>(spans: impl Iterator<Item = &'a str>) -> Self {
        let mut line_count = 1usize;
        let mut line_ending_counts = LineEndingCounts::default();
        let mut artifact_summary = TextArtifactSummary::default();
        let mut is_ascii_subset = true;
        let mut pending_cr = false;

        for span in spans {
            for ch in span.chars() {
                if pending_cr {
                    pending_cr = false;
                    if ch == '\n' {
                        line_ending_counts.crlf += 1;
                        line_count += 1;
                        continue;
                    } else {
                        line_ending_counts.cr += 1;
                        line_count += 1;
                    }
                }

                is_ascii_subset &= ch.is_ascii();
                match ch {
                    '\r' => {
                        pending_cr = true;
                    }
                    '\n' => {
                        line_ending_counts.lf += 1;
                        line_count += 1;
                    }
                    '\u{1B}' => {
                        artifact_summary.has_ansi_sequences = true;
                    }
                    '\u{0008}' => {
                        artifact_summary.has_backspaces = true;
                    }
                    '\t' => {}
                    _ if ch.is_control() => {
                        artifact_summary.other_control_count += 1;
                    }
                    _ => {}
                }
            }
        }

        if pending_cr {
            line_ending_counts.cr += 1;
            line_count += 1;
        }

        let line_endings = line_ending_style(line_ending_counts);
        artifact_summary.has_carriage_returns =
            line_endings != LineEndingStyle::Cr && line_ending_counts.cr > 0;

        Self {
            line_count,
            line_endings,
            line_ending_counts,
            artifact_summary,
            is_ascii_subset,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodingSource {
    Bom,
    #[default]
    Heuristic,
    ExplicitUserChoice,
    DefaultForNewFile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFormatMetadata {
    pub encoding_name: String,
    pub has_bom: bool,
    pub line_endings: LineEndingStyle,
    pub line_ending_counts: LineEndingCounts,
    #[serde(default)]
    pub preferred_line_ending: LineEndingStyle,
    pub encoding_source: EncodingSource,
    pub is_ascii_subset: bool,
    pub has_decoding_warnings: bool,
}

impl TextFormatMetadata {
    pub fn utf8_for_new_file(text: &str) -> Self {
        Self::from_inspection(
            TextInspection::inspect(text),
            "UTF-8".to_owned(),
            false,
            EncodingSource::DefaultForNewFile,
            false,
        )
    }

    pub fn detected(
        text: &str,
        encoding_name: String,
        has_bom: bool,
        encoding_source: EncodingSource,
        has_decoding_warnings: bool,
    ) -> Self {
        Self::from_inspection(
            TextInspection::inspect(text),
            encoding_name,
            has_bom,
            encoding_source,
            has_decoding_warnings,
        )
    }

    pub fn refresh_from_text(&mut self, text: &str) {
        self.apply_inspection(&TextInspection::inspect(text));
    }

    pub fn encoding_label(&self) -> String {
        let base = match self.encoding_name.as_str() {
            "windows-1252" => "Windows-1252 (ANSI)".to_owned(),
            "UTF-8" => "UTF-8".to_owned(),
            "UTF-16LE" => "UTF-16LE".to_owned(),
            "UTF-16BE" => "UTF-16BE".to_owned(),
            other => other.to_owned(),
        };

        if self.has_bom {
            format!("{base} BOM")
        } else {
            base
        }
    }

    pub fn encoding_tooltip(&self) -> String {
        let source = match self.encoding_source {
            EncodingSource::Bom => "Detected from BOM",
            EncodingSource::Heuristic => "Detected heuristically",
            EncodingSource::ExplicitUserChoice => "Selected explicitly",
            EncodingSource::DefaultForNewFile => "Default for new files",
        };
        let ascii = if self.is_ascii_subset {
            "; ASCII-only content"
        } else {
            ""
        };
        format!("{}{}", source, ascii)
    }

    pub fn line_endings_label(&self) -> &'static str {
        self.line_endings.label()
    }

    pub fn format_warning_text(&self) -> Option<String> {
        let mut warnings = Vec::new();
        if self.line_endings == LineEndingStyle::Mixed {
            warnings.push("Mixed line endings detected".to_owned());
        }
        if self.has_decoding_warnings {
            warnings.push("Decoding substitutions present".to_owned());
        }

        if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("; "))
        }
    }

    pub fn preferred_line_ending_style(&self) -> LineEndingStyle {
        match self.preferred_line_ending {
            LineEndingStyle::Lf | LineEndingStyle::Crlf | LineEndingStyle::Cr => {
                self.preferred_line_ending
            }
            LineEndingStyle::Mixed | LineEndingStyle::None => {
                resolve_preferred_line_ending(self.line_endings, self.line_ending_counts)
            }
        }
    }

    pub fn has_non_compliant_characters(&self, text: &str) -> bool {
        let Some(encoding) = Encoding::for_label(self.encoding_name.as_bytes()) else {
            return true;
        };

        let (_, _, had_replacements) = encoding.encode(text);
        had_replacements
    }

    pub fn has_non_compliant_characters_spans<'a>(
        &self,
        spans: impl Iterator<Item = &'a str>,
    ) -> bool {
        let Some(encoding) = Encoding::for_label(self.encoding_name.as_bytes()) else {
            return true;
        };

        if encoding == encoding_rs::UTF_8 {
            return false;
        }

        let mut encoder = encoding.new_encoder();
        let mut dst = [0u8; 4096];
        for span in spans {
            let mut src = span;
            loop {
                let (result, read, _written, had_errors) =
                    encoder.encode_from_utf8(src, &mut dst, false);
                if had_errors {
                    return true;
                }
                src = &src[read..];
                if result == encoding_rs::CoderResult::InputEmpty {
                    break;
                }
            }
        }
        let (_result, _read, _written, had_errors) =
            encoder.encode_from_utf8("", &mut dst, true);
        had_errors
    }

    fn apply_inspection(&mut self, inspection: &TextInspection) {
        self.line_ending_counts = inspection.line_ending_counts;
        self.line_endings = inspection.line_endings;
        self.is_ascii_subset = inspection.is_ascii_subset;
    }

    fn from_inspection(
        inspection: TextInspection,
        encoding_name: String,
        has_bom: bool,
        encoding_source: EncodingSource,
        has_decoding_warnings: bool,
    ) -> Self {
        Self {
            encoding_name,
            has_bom,
            line_endings: inspection.line_endings,
            line_ending_counts: inspection.line_ending_counts,
            preferred_line_ending: resolve_preferred_line_ending(
                inspection.line_endings,
                inspection.line_ending_counts,
            ),
            encoding_source,
            is_ascii_subset: inspection.is_ascii_subset,
            has_decoding_warnings,
        }
    }
}

fn resolve_preferred_line_ending(
    line_endings: LineEndingStyle,
    line_ending_counts: LineEndingCounts,
) -> LineEndingStyle {
    match line_endings {
        LineEndingStyle::Lf | LineEndingStyle::Crlf | LineEndingStyle::Cr => line_endings,
        LineEndingStyle::Mixed => line_ending_counts
            .dominant_style()
            .unwrap_or_else(platform_default_line_ending),
        LineEndingStyle::None => platform_default_line_ending(),
    }
}

pub fn platform_default_line_ending() -> LineEndingStyle {
    if cfg!(windows) {
        LineEndingStyle::Crlf
    } else {
        LineEndingStyle::Lf
    }
}

pub fn analyze_line_endings(text: &str) -> (LineEndingCounts, LineEndingStyle) {
    let inspection = TextInspection::inspect(text);
    (inspection.line_ending_counts, inspection.line_endings)
}

fn line_ending_style(counts: LineEndingCounts) -> LineEndingStyle {
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

impl TextArtifactSummary {
    pub fn from_text(text: &str) -> Self {
        TextInspection::inspect(text).artifact_summary
    }

    pub fn from_text_with_line_endings(text: &str, line_endings: LineEndingStyle) -> Self {
        TextInspection::inspect_with_line_endings(text, Some(line_endings)).artifact_summary
    }

    pub fn has_control_chars(&self) -> bool {
        self.has_ansi_sequences
            || self.has_carriage_returns
            || self.has_backspaces
            || self.other_control_count > 0
    }

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
        if self.other_control_count > 0 {
            parts.push("CTL");
        }

        Some(format!("Control characters detected: {}", parts.join(", ")))
    }
}

pub fn display_line_count(text: &str) -> usize {
    TextInspection::inspect(text).line_count
}

pub(crate) struct BufferTextMetadata {
    pub(crate) line_count: usize,
    pub(crate) artifact_summary: TextArtifactSummary,
    pub(crate) preferred_line_ending: LineEndingStyle,
    pub(crate) has_non_compliant_characters: bool,
}

pub(crate) fn buffer_text_metadata(
    text: &str,
    format: &mut TextFormatMetadata,
) -> BufferTextMetadata {
    let inspection = TextInspection::inspect(text);
    format.apply_inspection(&inspection);
    let has_non_compliant = format.has_non_compliant_characters(text);
    BufferTextMetadata {
        line_count: inspection.line_count,
        artifact_summary: inspection.artifact_summary,
        preferred_line_ending: format.preferred_line_ending_style(),
        has_non_compliant_characters: has_non_compliant,
    }
}

pub(crate) fn buffer_text_metadata_from_piece_tree(
    tree: &PieceTreeLite,
    format: &mut TextFormatMetadata,
) -> BufferTextMetadata {
    let spans = tree.spans_for_range(0..tree.len_chars());
    let inspection = TextInspection::inspect_spans(spans.map(|s| s.text));
    format.apply_inspection(&inspection);
    BufferTextMetadata {
        line_count: inspection.line_count,
        artifact_summary: inspection.artifact_summary,
        preferred_line_ending: format.preferred_line_ending_style(),
        has_non_compliant_characters: false,
    }
}
