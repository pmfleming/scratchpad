use crate::app::domain::buffer::ByteSpan;
use crate::app::services::settings_store::{IndentationStyle, TabDisplayMode};
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
    #[must_use]
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

    #[must_use]
    pub fn active_text_format(self, font_id: egui::FontId, dark_mode: bool) -> egui::TextFormat {
        egui::TextFormat {
            font_id,
            color: self.text_color(),
            background: self.active_background(dark_mode),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn passive_text_format(self, font_id: egui::FontId) -> egui::TextFormat {
        egui::TextFormat {
            font_id,
            color: self.text_color(),
            background: self.passive_background(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy)]
pub struct TextEditOptions<'a> {
    pub request_focus: bool,
    pub word_wrap: bool,
    pub right_to_left_reading_order: bool,
    pub editor_font_id: &'a egui::FontId,
    pub text_color: egui::Color32,
    pub highlight_style: EditorHighlightStyle,
    pub warm_layout_cache: bool,
    pub indentation_style: IndentationStyle,
    pub indentation_width: u8,
    pub tab_display: TabDisplayMode,
}

impl<'a> TextEditOptions<'a> {
    #[must_use]
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
            right_to_left_reading_order: false,
            editor_font_id,
            text_color,
            highlight_style,
            warm_layout_cache: true,
            indentation_style: IndentationStyle::default(),
            indentation_width: 4,
            tab_display: TabDisplayMode::default(),
        }
    }

    #[must_use]
    pub fn with_indentation(
        mut self,
        indentation_style: IndentationStyle,
        indentation_width: u8,
        tab_display: TabDisplayMode,
    ) -> Self {
        self.indentation_style = indentation_style;
        self.indentation_width = indentation_width.clamp(1, 16);
        self.tab_display = tab_display;
        self
    }

    #[must_use]
    pub fn with_layout_cache_warming(mut self, enabled: bool) -> Self {
        self.warm_layout_cache = enabled;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharCursor {
    pub index: usize,
    pub prefer_next_row: bool,
}

impl CharCursor {
    #[must_use]
    pub fn new(index: usize) -> Self {
        Self {
            index,
            prefer_next_row: false,
        }
    }

    pub(super) fn to_egui_ccursor(self) -> egui::text::CCursor {
        egui::text::CCursor {
            index: egui::text::CharIndex(self.index),
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
    #[must_use]
    pub fn one(cursor: CharCursor) -> Self {
        Self {
            primary: cursor,
            secondary: cursor,
        }
    }

    #[must_use]
    pub fn two(min: usize, max: usize) -> Self {
        Self {
            primary: CharCursor::new(max),
            secondary: CharCursor::new(min),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.primary.index == self.secondary.index
    }

    #[must_use]
    pub fn sorted_indices(&self) -> (usize, usize) {
        let a = self.primary.index;
        let b = self.secondary.index;
        if a <= b { (a, b) } else { (b, a) }
    }

    #[must_use]
    pub fn as_sorted_char_range(&self) -> Range<usize> {
        let (start, end) = self.sorted_indices();
        start..end
    }

    #[must_use]
    pub fn to_egui(&self) -> egui::text::CCursorRange {
        egui::text::CCursorRange {
            primary: self.primary.to_egui_ccursor(),
            secondary: self.secondary.to_egui_ccursor(),
            h_pos: None,
        }
    }

    #[must_use]
    pub fn from_egui(range: egui::text::CCursorRange) -> Self {
        Self {
            primary: CharCursor {
                index: range.primary.index.into(),
                prefer_next_row: range.primary.prefer_next_row,
            },
            secondary: CharCursor {
                index: range.secondary.index.into(),
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
    pub deleted_spans: Vec<ByteSpan>,
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
        ((f32::from(left) * left_weight) + (f32::from(right) * right_weight)).round() as u8
    };
    egui::Color32::from_rgb(
        channel(left.r(), right.r()),
        channel(left.g(), right.g()),
        channel(left.b(), right.b()),
    )
}
