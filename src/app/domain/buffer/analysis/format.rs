use encoding_rs::Encoding;
use serde::{Deserialize, Serialize};

use super::inspection::TextInspection;
use super::line_endings::{LineEndingCounts, LineEndingStyle, resolve_preferred_line_ending};

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
    #[must_use]
    pub fn utf8_for_new_file(text: &str) -> Self {
        Self::from_inspection(
            TextInspection::inspect(text),
            "UTF-8".to_owned(),
            false,
            EncodingSource::DefaultForNewFile,
            false,
        )
    }

    #[must_use]
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

    #[must_use]
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

    #[must_use]
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
        format!("{source}{ascii}")
    }

    #[must_use]
    pub fn line_endings_label(&self) -> &'static str {
        self.line_endings.label()
    }

    #[must_use]
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

    #[must_use]
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

    #[must_use]
    pub fn has_non_compliant_characters(&self, text: &str) -> bool {
        let Some(encoding) = Encoding::for_label(self.encoding_name.as_bytes()) else {
            return true;
        };

        if encoding == encoding_rs::UTF_8 {
            return false;
        }

        let (_, _, had_replacements) = encoding.encode(text);
        had_replacements
    }

    pub fn has_non_compliant_characters_spans(
        &self,
        spans: impl Iterator<Item = impl AsRef<str>>,
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
            let mut src = span.as_ref();
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
        let (_result, _read, _written, had_errors) = encoder.encode_from_utf8("", &mut dst, true);
        had_errors
    }

    pub(super) fn apply_inspection(&mut self, inspection: &TextInspection) {
        self.line_ending_counts = inspection.line_ending_counts;
        self.line_endings = inspection.line_endings;
        self.is_ascii_subset = inspection.is_ascii_subset;
    }

    pub(super) fn from_inspection(
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

#[cfg(test)]
mod tests {
    use super::{EncodingSource, TextFormatMetadata};

    #[test]
    fn encoding_label_names_known_windows_ansi_encoding() {
        let format = TextFormatMetadata::detected(
            "plain",
            "windows-1252".to_owned(),
            false,
            EncodingSource::Heuristic,
            false,
        );

        assert_eq!(format.encoding_label(), "Windows-1252 (ANSI)");
    }

    #[test]
    fn format_warning_text_combines_format_warnings() {
        let format = TextFormatMetadata::detected(
            "a\r\nb\n",
            "UTF-8".to_owned(),
            false,
            EncodingSource::Heuristic,
            true,
        );

        assert_eq!(
            format.format_warning_text(),
            Some("Mixed line endings detected; Decoding substitutions present".to_owned())
        );
    }
}
