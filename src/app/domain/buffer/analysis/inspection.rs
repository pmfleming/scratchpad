use super::{LineEndingCounts, LineEndingStyle, TextArtifactSummary};
use std::borrow::Cow;

mod scan;
#[cfg(test)]
mod tests;

pub(super) use scan::TextInspection;
#[cfg(test)]
use scan::TextScanSummary;

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
