use super::types::{EditorHighlightStyle, TextEditOptions};
use crate::app::domain::SearchHighlightState;
use eframe::egui;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum HighlightKind {
    Selection,
    SearchActive,
    SearchPassive,
}

#[derive(Clone)]
struct TextHighlightRange {
    range: Range<usize>,
    kind: HighlightKind,
}

struct HighlightLayoutStyle<'a> {
    wrap_width: f32,
    word_wrap: bool,
    font_id: &'a egui::FontId,
    text_color: egui::Color32,
    highlight: EditorHighlightStyle,
    dark_mode: bool,
}

pub(super) fn build_galley(
    ui: &egui::Ui,
    text: &str,
    options: TextEditOptions<'_>,
    search_highlights: &SearchHighlightState,
    selection_range: Option<Range<usize>>,
    wrap_width: f32,
) -> Arc<egui::Galley> {
    let job = layout_job_with_highlights(
        text,
        search_highlights,
        selection_range,
        HighlightLayoutStyle {
            wrap_width,
            word_wrap: options.word_wrap,
            font_id: options.editor_font_id,
            text_color: options.text_color,
            highlight: options.highlight_style,
            dark_mode: ui.visuals().dark_mode,
        },
    );
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

pub fn build_layouter(
    font_id: egui::FontId,
    word_wrap: bool,
    text_color: egui::Color32,
    highlight_style: EditorHighlightStyle,
    search_highlights: SearchHighlightState,
    selection_range: Option<Range<usize>>,
) -> super::types::LayouterFn {
    Box::new(move |ui: &egui::Ui, text: &str, wrap_width: f32| {
        let job = layout_job_with_highlights(
            text,
            &search_highlights,
            selection_range.clone(),
            HighlightLayoutStyle {
                wrap_width,
                word_wrap,
                font_id: &font_id,
                text_color,
                highlight: highlight_style,
                dark_mode: ui.visuals().dark_mode,
            },
        );
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    })
}

fn layout_job_with_highlights(
    text: &str,
    search_highlights: &SearchHighlightState,
    selection_range: Option<Range<usize>>,
    style: HighlightLayoutStyle<'_>,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = if style.word_wrap {
        style.wrap_width
    } else {
        f32::INFINITY
    };

    let char_to_byte = CharByteMap::build(text);
    let text_char_len = char_to_byte.char_len();
    let highlights = merged_highlight_ranges(search_highlights, selection_range, text_char_len);

    if highlights.is_empty() {
        append_plain_text_segment(&mut job, text, &style);
        return job;
    }

    append_highlighted_segments(&mut job, text, &char_to_byte, &highlights, &style);
    job
}

fn append_plain_text_segment(
    job: &mut egui::text::LayoutJob,
    text: &str,
    style: &HighlightLayoutStyle<'_>,
) {
    append_job_segment(
        job,
        text,
        style.font_id,
        style.text_color,
        egui::Color32::TRANSPARENT,
    );
}

fn append_highlighted_segments(
    job: &mut egui::text::LayoutJob,
    text: &str,
    char_to_byte: &CharByteMap,
    highlights: &[TextHighlightRange],
    style: &HighlightLayoutStyle<'_>,
) {
    for window in highlight_boundaries(highlights, char_to_byte.char_len()).windows(2) {
        append_highlight_window(job, text, char_to_byte, highlights, style, window);
    }
}

fn append_highlight_window(
    job: &mut egui::text::LayoutJob,
    text: &str,
    char_to_byte: &CharByteMap,
    highlights: &[TextHighlightRange],
    style: &HighlightLayoutStyle<'_>,
    window: &[usize],
) {
    let segment = window[0]..window[1];
    if segment.is_empty() || segment.end > char_to_byte.char_len() {
        return;
    }
    let bytes = char_to_byte.byte_offset(segment.start)..char_to_byte.byte_offset(segment.end);
    let kind = highlight_kind_for_segment(highlights, segment.start);
    let (text_color, background) = segment_colors(kind, style);
    append_job_segment(job, &text[bytes], style.font_id, text_color, background);
}

fn highlight_boundaries(highlights: &[TextHighlightRange], text_char_len: usize) -> Vec<usize> {
    let mut boundaries = vec![0, text_char_len];
    for highlight in highlights {
        boundaries.push(highlight.range.start);
        boundaries.push(highlight.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

fn segment_colors(
    kind: Option<HighlightKind>,
    style: &HighlightLayoutStyle<'_>,
) -> (egui::Color32, egui::Color32) {
    match kind {
        Some(HighlightKind::Selection | HighlightKind::SearchActive) => (
            style.highlight.text_color(),
            style.highlight.active_background(style.dark_mode),
        ),
        Some(HighlightKind::SearchPassive) => (
            style.highlight.text_color(),
            style.highlight.passive_background(),
        ),
        None => (style.text_color, egui::Color32::TRANSPARENT),
    }
}

fn merged_highlight_ranges(
    search_highlights: &SearchHighlightState,
    selection_range: Option<Range<usize>>,
    text_char_len: usize,
) -> Vec<TextHighlightRange> {
    let mut highlights = Vec::new();
    push_selection_highlight(&mut highlights, selection_range, text_char_len);
    push_search_highlights(&mut highlights, search_highlights, text_char_len);
    highlights
}

fn push_selection_highlight(
    highlights: &mut Vec<TextHighlightRange>,
    selection_range: Option<Range<usize>>,
    text_char_len: usize,
) {
    if let Some(range) = selection_range.filter(|range| range.end <= text_char_len) {
        highlights.push(TextHighlightRange {
            range,
            kind: HighlightKind::Selection,
        });
    }
}

fn push_search_highlights(
    highlights: &mut Vec<TextHighlightRange>,
    search_highlights: &SearchHighlightState,
    text_char_len: usize,
) {
    for (index, range) in search_highlights.ranges.iter().enumerate() {
        if valid_highlight_range(range, text_char_len) {
            highlights.push(TextHighlightRange {
                range: range.clone(),
                kind: search_highlight_kind(search_highlights, index),
            });
        }
    }
}

fn valid_highlight_range(range: &Range<usize>, text_char_len: usize) -> bool {
    range.start < range.end && range.end <= text_char_len
}

fn search_highlight_kind(search_highlights: &SearchHighlightState, index: usize) -> HighlightKind {
    if search_highlights.active_range_index == Some(index) {
        HighlightKind::SearchActive
    } else {
        HighlightKind::SearchPassive
    }
}

fn highlight_kind_for_segment(
    highlights: &[TextHighlightRange],
    segment_start: usize,
) -> Option<HighlightKind> {
    let mut best: Option<HighlightKind> = None;
    for highlight in highlights {
        if !highlight.range.contains(&segment_start) {
            continue;
        }
        match promote_highlight_kind(best, highlight.kind) {
            HighlightPromotion::Done(kind) => return Some(kind),
            HighlightPromotion::Continue(kind) => best = kind,
        }
    }
    best
}

enum HighlightPromotion {
    Done(HighlightKind),
    Continue(Option<HighlightKind>),
}

fn promote_highlight_kind(
    current: Option<HighlightKind>,
    candidate: HighlightKind,
) -> HighlightPromotion {
    match candidate {
        HighlightKind::Selection => HighlightPromotion::Done(HighlightKind::Selection),
        HighlightKind::SearchActive => {
            HighlightPromotion::Continue(Some(HighlightKind::SearchActive))
        }
        HighlightKind::SearchPassive if current.is_none() => {
            HighlightPromotion::Continue(Some(HighlightKind::SearchPassive))
        }
        HighlightKind::SearchPassive => HighlightPromotion::Continue(current),
    }
}

fn append_job_segment(
    job: &mut egui::text::LayoutJob,
    text: &str,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    background: egui::Color32,
) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font_id.clone(),
            color: text_color,
            background,
            ..Default::default()
        },
    );
}

enum CharByteMap {
    /// All ASCII: char offset == byte offset, no allocation needed.
    Ascii { len: usize },
    /// Non-ASCII: lookup table from char index to byte offset.
    Map(Vec<usize>),
}

impl CharByteMap {
    fn build(text: &str) -> Self {
        if text.is_ascii() {
            CharByteMap::Ascii { len: text.len() }
        } else {
            let mut offsets: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
            offsets.push(text.len());
            CharByteMap::Map(offsets)
        }
    }

    fn char_len(&self) -> usize {
        match self {
            CharByteMap::Ascii { len } => *len,
            CharByteMap::Map(offsets) => offsets.len().saturating_sub(1),
        }
    }

    fn byte_offset(&self, char_index: usize) -> usize {
        match self {
            CharByteMap::Ascii { .. } => char_index,
            CharByteMap::Map(offsets) => offsets[char_index],
        }
    }
}
