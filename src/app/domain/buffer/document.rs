use super::{LineEndingStyle, platform_default_line_ending};
use eframe::egui::{self, TextBuffer};
use std::borrow::Cow;
use std::ops::Range;

pub type TextDocumentUndoState = (egui::text::CCursorRange, String);
pub type TextDocumentUndoer = egui::util::undoer::Undoer<TextDocumentUndoState>;
pub const TEXT_DOCUMENT_MAX_UNDOS: usize = 100;
pub(crate) type TextReplacements<'a> = &'a [(Range<usize>, String)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextReplacementError {
    InvalidRange,
    OutOfBounds,
    NotDescending,
    OverlappingRanges,
}

#[derive(Clone)]
pub struct TextDocument {
    text: String,
    undoer: TextDocumentUndoer,
    preferred_line_ending: LineEndingStyle,
}

impl TextDocument {
    pub fn new(text: String) -> Self {
        Self::with_preferred_line_ending(text, platform_default_line_ending())
    }

    pub fn with_preferred_line_ending(
        text: String,
        preferred_line_ending: LineEndingStyle,
    ) -> Self {
        Self {
            text,
            undoer: new_text_document_undoer(),
            preferred_line_ending,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn undoer(&self) -> TextDocumentUndoer {
        self.undoer.clone()
    }

    pub fn set_undoer(&mut self, undoer: TextDocumentUndoer) {
        self.undoer = undoer;
    }

    pub fn clear_undoer(&mut self) {
        self.undoer = new_text_document_undoer();
    }

    pub fn set_preferred_line_ending(&mut self, preferred_line_ending: LineEndingStyle) {
        self.preferred_line_ending = preferred_line_ending;
    }

    pub fn replace_text(&mut self, text: String) {
        self.text = text;
        self.clear_undoer();
    }

    pub(crate) fn replace_char_ranges_with_undo(
        &mut self,
        replacements: TextReplacements<'_>,
        previous_selection: egui::text::CCursorRange,
        next_selection: egui::text::CCursorRange,
    ) -> Result<(), TextReplacementError> {
        if replacements.is_empty() {
            return Ok(());
        }

        validate_replacements(replacements, self.text.chars().count())?;

        let previous_state = (previous_selection, self.text.clone());
        for (range, replacement) in replacements {
            self.delete_char_range(range.clone());
            let byte_index = self.byte_index_from_char_index(range.start);
            let normalized =
                normalize_editor_inserted_text(replacement, self.preferred_line_ending);
            self.text.insert_str(byte_index, normalized.as_ref());
        }
        let current_state = (next_selection, self.text.clone());
        self.undoer.add_undo(&previous_state);
        self.undoer.add_undo(&current_state);
        Ok(())
    }
}

fn new_text_document_undoer() -> TextDocumentUndoer {
    TextDocumentUndoer::with_settings(egui::util::undoer::Settings {
        max_undos: TEXT_DOCUMENT_MAX_UNDOS,
        ..Default::default()
    })
}

impl TextBuffer for TextDocument {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        self.as_str()
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        let byte_idx = self.byte_index_from_char_index(char_index);
        let normalized_text = normalize_editor_inserted_text(text, self.preferred_line_ending);
        self.text.insert_str(byte_idx, normalized_text.as_ref());
        normalized_text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: Range<usize>) {
        assert!(
            char_range.start <= char_range.end,
            "start must be <= end, but got {char_range:?}"
        );

        let byte_start = self.byte_index_from_char_index(char_range.start);
        let byte_end = self.byte_index_from_char_index(char_range.end);
        self.text.drain(byte_start..byte_end);
    }

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
}

fn normalize_editor_inserted_text(
    text: &str,
    preferred_line_ending: LineEndingStyle,
) -> Cow<'_, str> {
    match text {
        "\r" | "\r\n" | "\n" => Cow::Borrowed(preferred_line_ending.as_str()),
        _ if !text.contains('\n') => Cow::Borrowed(text),
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

fn validate_replacements(
    replacements: TextReplacements<'_>,
    text_char_len: usize,
) -> Result<(), TextReplacementError> {
    let mut previous_start = None;

    for (range, _) in replacements {
        if range.start > range.end {
            return Err(TextReplacementError::InvalidRange);
        }
        if range.end > text_char_len {
            return Err(TextReplacementError::OutOfBounds);
        }
        if let Some(last_start) = previous_start {
            if range.start > last_start {
                return Err(TextReplacementError::NotDescending);
            }
            if range.end > last_start {
                return Err(TextReplacementError::OverlappingRanges);
            }
        }
        previous_start = Some(range.start);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TextDocument, TextReplacementError};
    use eframe::egui;

    fn selection(start: usize, end: usize) -> egui::text::CCursorRange {
        egui::text::CCursorRange::two(
            egui::text::CCursor::new(start),
            egui::text::CCursor::new(end),
        )
    }

    #[test]
    fn replacing_char_ranges_updates_text_and_undo_points() {
        let mut document = TextDocument::new("alpha beta".to_owned());
        let previous_selection = selection(6, 10);
        let next_selection = selection(6, 11);

        document
            .replace_char_ranges_with_undo(
                &[(6..10, "gamma".to_owned())],
                previous_selection,
                next_selection,
            )
            .expect("replace current match");

        assert_eq!(document.as_str(), "alpha gamma");
        let current_state = (next_selection, document.as_str().to_owned());
        let mut undoer = document.undoer();
        let previous_state = undoer
            .undo(&current_state)
            .cloned()
            .expect("undo state should exist");
        assert_eq!(previous_state.0, previous_selection);
        assert_eq!(previous_state.1, "alpha beta");
    }

    #[test]
    fn multiple_replacements_must_be_descending() {
        let mut document = TextDocument::new("alpha beta gamma".to_owned());
        let error = document
            .replace_char_ranges_with_undo(
                &[(0..5, "omega".to_owned()), (11..16, "delta".to_owned())],
                selection(0, 5),
                selection(0, 5),
            )
            .expect_err("ascending replacements should be rejected");

        assert_eq!(error, TextReplacementError::NotDescending);
    }

    #[test]
    fn overlapping_replacements_are_rejected() {
        let mut document = TextDocument::new("alpha beta gamma".to_owned());
        let error = document
            .replace_char_ranges_with_undo(
                &[(6..10, "BETA".to_owned()), (4..8, "X".to_owned())],
                selection(6, 10),
                selection(6, 10),
            )
            .expect_err("overlapping replacements should be rejected");

        assert_eq!(error, TextReplacementError::OverlappingRanges);
    }
}