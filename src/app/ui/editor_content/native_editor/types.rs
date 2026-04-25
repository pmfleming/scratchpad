use eframe::egui;
use std::ops::Range;
use std::sync::Arc;

pub type LayouterFn = Box<dyn FnMut(&egui::Ui, &str, f32) -> Arc<egui::Galley>>;

#[derive(Clone, Copy)]
pub struct EditorHighlightStyle {
    pub(super) background: egui::Color32,
    pub(super) text: egui::Color32,
}

impl EditorHighlightStyle {
    pub fn new(background: egui::Color32, text: egui::Color32) -> Self {
        Self { background, text }
    }

    pub(super) fn passive_background(self) -> egui::Color32 {
        self.background
    }

    pub(super) fn active_background(self, dark_mode: bool) -> egui::Color32 {
        if dark_mode {
            blend_colors(self.background, egui::Color32::BLACK, 0.18)
        } else {
            blend_colors(self.background, egui::Color32::BLACK, 0.28)
        }
    }

    pub(super) fn text_color(self) -> egui::Color32 {
        self.text
    }
}

#[derive(Clone, Copy)]
pub struct TextEditOptions<'a> {
    pub request_focus: bool,
    pub word_wrap: bool,
    pub editor_font_id: &'a egui::FontId,
    pub text_color: egui::Color32,
    pub highlight_style: EditorHighlightStyle,
}

impl<'a> TextEditOptions<'a> {
    pub fn new(
        request_focus: bool,
        word_wrap: bool,
        editor_font_id: &'a egui::FontId,
        text_color: egui::Color32,
        highlight_style: EditorHighlightStyle,
    ) -> Self {
        Self {
            request_focus,
            word_wrap,
            editor_font_id,
            text_color,
            highlight_style,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharCursor {
    pub index: usize,
    pub prefer_next_row: bool,
}

impl CharCursor {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            prefer_next_row: false,
        }
    }

    pub(super) fn to_egui_ccursor(self) -> egui::text::CCursor {
        egui::text::CCursor {
            index: self.index,
            prefer_next_row: self.prefer_next_row,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorRange {
    pub primary: CharCursor,
    pub secondary: CharCursor,
}

impl CursorRange {
    pub fn one(cursor: CharCursor) -> Self {
        Self {
            primary: cursor,
            secondary: cursor,
        }
    }

    pub fn two(min: usize, max: usize) -> Self {
        Self {
            primary: CharCursor::new(max),
            secondary: CharCursor::new(min),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.primary.index == self.secondary.index
    }

    pub fn sorted_indices(&self) -> (usize, usize) {
        let a = self.primary.index;
        let b = self.secondary.index;
        if a <= b { (a, b) } else { (b, a) }
    }

    pub fn as_sorted_char_range(&self) -> Range<usize> {
        let (start, end) = self.sorted_indices();
        start..end
    }

    pub fn to_egui(&self) -> egui::text::CCursorRange {
        egui::text::CCursorRange {
            primary: self.primary.to_egui_ccursor(),
            secondary: self.secondary.to_egui_ccursor(),
            h_pos: None,
        }
    }

    pub fn from_egui(range: egui::text::CCursorRange) -> Self {
        Self {
            primary: CharCursor {
                index: range.primary.index,
                prefer_next_row: range.primary.prefer_next_row,
            },
            secondary: CharCursor {
                index: range.secondary.index,
                prefer_next_row: range.secondary.prefer_next_row,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditOperation {
    pub start_char: usize,
    pub deleted_text: String,
    pub inserted_text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OperationRecord {
    pub previous_cursor: CursorRange,
    pub next_cursor: CursorRange,
    pub edits: Vec<EditOperation>,
}

pub(super) fn selection_char_range(cursor_range: &CursorRange) -> Option<Range<usize>> {
    (!cursor_range.is_empty()).then(|| cursor_range.as_sorted_char_range())
}

pub(super) fn blend_colors(
    left: egui::Color32,
    right: egui::Color32,
    right_weight: f32,
) -> egui::Color32 {
    let right_weight = right_weight.clamp(0.0, 1.0);
    let left_weight = 1.0 - right_weight;
    let channel = |left: u8, right: u8| {
        ((left as f32 * left_weight) + (right as f32 * right_weight)).round() as u8
    };
    egui::Color32::from_rgb(
        channel(left.r(), right.r()),
        channel(left.g(), right.g()),
        channel(left.b(), right.b()),
    )
}
