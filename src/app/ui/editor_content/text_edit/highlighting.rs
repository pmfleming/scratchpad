use super::{EditorHighlightStyle, LayoutCapture, TextLayouter, char_to_byte_map};
use crate::app::domain::SearchHighlightState;
use eframe::egui;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

pub(super) struct HighlightLayoutStyle<'a> {
    pub(super) wrap_width: f32,
    pub(super) word_wrap: bool,
    pub(super) font_id: &'a egui::FontId,
    pub(super) text_color: egui::Color32,
    pub(super) highlight: EditorHighlightStyle,
    pub(super) dark_mode: bool,
}

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

pub fn build_layouter(
    font_id: egui::FontId,
    word_wrap: bool,
    text_color: egui::Color32,
    highlight_style: EditorHighlightStyle,
    search_highlights: SearchHighlightState,
    selection_range: Option<Range<usize>>,
) -> TextLayouter {
    Box::new(
        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let job = layout_job_with_highlights(
                buf.as_str(),
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
        },
    )
}

pub(super) fn tracked_layouter(
    font_id: egui::FontId,
    word_wrap: bool,
    text_color: egui::Color32,
    highlight_style: EditorHighlightStyle,
    search_highlights: SearchHighlightState,
    selection_range: Option<Range<usize>>,
) -> (TextLayouter, LayoutCapture) {
    let mut layouter = build_layouter(
        font_id,
        word_wrap,
        text_color,
        highlight_style,
        search_highlights,
        selection_range,
    );
    let layout_capture = Rc::new(RefCell::new(None));
    let capture_for_layouter = Rc::clone(&layout_capture);
    let tracking_layouter = Box::new(
        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let galley = layouter(ui, buf, wrap_width);
            *capture_for_layouter.borrow_mut() = Some(galley.clone());
            galley
        },
    );

    (tracking_layouter, layout_capture)
}

pub(super) fn layout_job_with_highlights(
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

    let char_to_byte = char_to_byte_map(text);
    let text_char_len = char_to_byte.len().saturating_sub(1);
    let highlights = merged_highlight_ranges(search_highlights, selection_range, text_char_len);

    if highlights.is_empty() {
        append_job_segment(
            &mut job,
            text,
            style.font_id,
            style.text_color,
            egui::Color32::TRANSPARENT,
        );
        return job;
    }

    let mut boundaries = vec![0, text_char_len];
    for highlight in &highlights {
        boundaries.push(highlight.range.start);
        boundaries.push(highlight.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    for window in boundaries.windows(2) {
        let segment_start = window[0];
        let segment_end = window[1];
        if segment_start >= segment_end || segment_end > text_char_len {
            continue;
        }
        let start_byte = char_to_byte[segment_start];
        let end_byte = char_to_byte[segment_end];
        let kind = highlight_kind_for_segment(&highlights, segment_start);
        let (text_color, background) = match kind {
            Some(HighlightKind::Selection | HighlightKind::SearchActive) => (
                style.highlight.text_color(),
                style.highlight.active_background(style.dark_mode),
            ),
            Some(HighlightKind::SearchPassive) => (
                style.highlight.text_color(),
                style.highlight.passive_background(),
            ),
            None => (style.text_color, egui::Color32::TRANSPARENT),
        };
        append_job_segment(
            &mut job,
            &text[start_byte..end_byte],
            style.font_id,
            text_color,
            background,
        );
    }

    job
}

fn merged_highlight_ranges(
    search_highlights: &SearchHighlightState,
    selection_range: Option<Range<usize>>,
    text_char_len: usize,
) -> Vec<TextHighlightRange> {
    let mut highlights = Vec::new();
    if let Some(range) = selection_range.filter(|range| range.end <= text_char_len) {
        highlights.push(TextHighlightRange {
            range,
            kind: HighlightKind::Selection,
        });
    }
    for (index, range) in search_highlights.ranges.iter().enumerate() {
        if range.start >= range.end || range.end > text_char_len {
            continue;
        }
        highlights.push(TextHighlightRange {
            range: range.clone(),
            kind: if search_highlights.active_range_index == Some(index) {
                HighlightKind::SearchActive
            } else {
                HighlightKind::SearchPassive
            },
        });
    }
    highlights
}

fn highlight_kind_for_segment(
    highlights: &[TextHighlightRange],
    segment_start: usize,
) -> Option<HighlightKind> {
    let mut best = None;
    for highlight in highlights {
        if highlight.range.start <= segment_start && segment_start < highlight.range.end {
            match highlight.kind {
                HighlightKind::Selection => return Some(HighlightKind::Selection),
                HighlightKind::SearchActive => best = Some(HighlightKind::SearchActive),
                HighlightKind::SearchPassive if best.is_none() => {
                    best = Some(HighlightKind::SearchPassive)
                }
                _ => {}
            }
        }
    }
    best
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

pub(super) fn windowed_search_highlights(
    search_highlights: &SearchHighlightState,
    visible_char_range: &Range<usize>,
) -> SearchHighlightState {
    let mut ranges = Vec::new();
    let mut active_range_index = None;

    for (index, range) in search_highlights.ranges.iter().enumerate() {
        let start = range.start.max(visible_char_range.start);
        let end = range.end.min(visible_char_range.end);
        if start >= end {
            continue;
        }

        if search_highlights.active_range_index == Some(index) {
            active_range_index = Some(ranges.len());
        }
        ranges.push((start - visible_char_range.start)..(end - visible_char_range.start));
    }

    SearchHighlightState {
        ranges,
        active_range_index,
    }
}

pub(super) fn windowed_char_range(
    range: Option<Range<usize>>,
    visible_char_range: &Range<usize>,
) -> Option<Range<usize>> {
    let range = range?;
    let start = range.start.max(visible_char_range.start);
    let end = range.end.min(visible_char_range.end);
    (start < end).then_some((start - visible_char_range.start)..(end - visible_char_range.start))
}
