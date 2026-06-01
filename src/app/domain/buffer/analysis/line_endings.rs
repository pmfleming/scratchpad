use serde::{Deserialize, Serialize};

use super::inspection::TextInspection;

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
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
            Self::Cr => "CR",
            Self::Mixed => "Mixed",
        }
    }

    #[must_use]
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
        entries.sort_by_key(|right| std::cmp::Reverse(right.0));
        (entries[0].0 > 0).then_some(entries[0].1)
    }
}

pub(super) fn resolve_preferred_line_ending(
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

#[must_use]
pub fn platform_default_line_ending() -> LineEndingStyle {
    if cfg!(windows) {
        LineEndingStyle::Crlf
    } else {
        LineEndingStyle::Lf
    }
}

#[must_use]
pub fn analyze_line_endings(text: &str) -> (LineEndingCounts, LineEndingStyle) {
    let inspection = TextInspection::inspect(text);
    (inspection.line_ending_counts, inspection.line_endings)
}

#[cfg(test)]
mod tests {
    use super::{LineEndingCounts, LineEndingStyle, resolve_preferred_line_ending};

    #[test]
    fn mixed_preferred_line_ending_uses_dominant_style() {
        let counts = LineEndingCounts {
            lf: 2,
            crlf: 4,
            cr: 1,
        };

        assert_eq!(
            resolve_preferred_line_ending(LineEndingStyle::Mixed, counts),
            LineEndingStyle::Crlf
        );
    }

    #[test]
    fn concrete_line_ending_style_is_already_preferred() {
        assert_eq!(
            resolve_preferred_line_ending(LineEndingStyle::Cr, LineEndingCounts::default()),
            LineEndingStyle::Cr
        );
    }
}
