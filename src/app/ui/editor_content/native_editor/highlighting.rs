use super::types::{EditorHighlightStyle, TextEditOptions};
use crate::app::capacity_metrics;
use crate::app::domain::SearchHighlightState;
use eframe::egui;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HighlightKind {
    Selection,
    SearchActive,
    SearchPassive,
}

/// Indexed by `HighlightKind as usize`.
const HIGHLIGHT_PRIORITY: [HighlightKind; 3] = [
    HighlightKind::Selection,
    HighlightKind::SearchActive,
    HighlightKind::SearchPassive,
];

#[derive(Clone)]
struct TextHighlightRange {
    range: Range<usize>,
    kind: HighlightKind,
}

struct HighlightLayoutStyle<'a> {
    wrap_width: f32,
    word_wrap: bool,
    right_to_left_reading_order: bool,
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
    let started_at = Instant::now();
    let job = layout_job_with_highlights(
        text,
        search_highlights,
        selection_range,
        HighlightLayoutStyle {
            wrap_width,
            word_wrap: options.word_wrap,
            right_to_left_reading_order: options.right_to_left_reading_order,
            font_id: options.editor_font_id,
            text_color: options.text_color,
            highlight: options.highlight_style,
            dark_mode: ui.visuals().dark_mode,
        },
    );
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    capacity_metrics::record_layout_job(text.len(), started_at.elapsed());
    galley
}

#[must_use]
pub fn build_layouter(
    font_id: egui::FontId,
    word_wrap: bool,
    text_color: egui::Color32,
    highlight_style: EditorHighlightStyle,
    search_highlights: SearchHighlightState,
    selection_range: Option<Range<usize>>,
) -> super::types::LayouterFn {
    Box::new(move |ui: &egui::Ui, text: &str, wrap_width: f32| {
        let started_at = Instant::now();
        let job = layout_job_with_highlights(
            text,
            &search_highlights,
            selection_range.clone(),
            HighlightLayoutStyle {
                wrap_width,
                word_wrap,
                right_to_left_reading_order: false,
                font_id: &font_id,
                text_color,
                highlight: highlight_style,
                dark_mode: ui.visuals().dark_mode,
            },
        );
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        capacity_metrics::record_layout_job(text.len(), started_at.elapsed());
        galley
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
    if style.right_to_left_reading_order {
        job.halign = egui::Align::RIGHT;
    }

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
    visit_highlight_segments(highlights, char_to_byte.char_len(), |segment, kind| {
        let bytes = char_to_byte.byte_offset(segment.start)..char_to_byte.byte_offset(segment.end);
        let (text_color, background) = segment_colors(kind, style);
        append_job_segment(job, &text[bytes], style.font_id, text_color, background);
    });
}

fn visit_highlight_segments(
    highlights: &[TextHighlightRange],
    len: usize,
    mut visit: impl FnMut(Range<usize>, Option<HighlightKind>),
) {
    let mut events = Vec::with_capacity(highlights.len() * 2);
    for highlight in highlights {
        if valid_highlight_range(&highlight.range, len) {
            events.push((highlight.range.start, highlight.kind as usize, 1_i32));
            events.push((highlight.range.end, highlight.kind as usize, -1_i32));
        }
    }
    events.sort_unstable_by_key(|event| event.0);
    let mut active = [0_i32; HIGHLIGHT_PRIORITY.len()];
    let mut position = 0;
    let mut index = 0;
    while position < len {
        while index < events.len() && events[index].0 == position {
            let (_, kind, delta) = events[index];
            active[kind] += delta;
            index += 1;
        }
        let end = events.get(index).map_or(len, |event| event.0);
        // Overlapping highlights resolve in discriminant order: selection first.
        let kind = active
            .iter()
            .position(|count| *count > 0)
            .map(|index| HIGHLIGHT_PRIORITY[index]);
        visit(position..end, kind);
        position = end;
    }
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

#[cfg(test)]
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

#[cfg(test)]
enum HighlightPromotion {
    Done(HighlightKind),
    Continue(Option<HighlightKind>),
}

#[cfg(test)]
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

#[cfg(test)]
mod sweep_tests {
    use super::{
        CharByteMap, HighlightKind, TextHighlightRange, highlight_kind_for_segment,
        visit_highlight_segments,
    };

    #[test]
    fn sweep_matches_priority_for_overlapping_unsorted_ranges() {
        for seed in 0..32 {
            let mut ranges = Vec::new();
            for i in 0..50 {
                let start = (i * 17 + seed * 13) % 80;
                ranges.push(TextHighlightRange {
                    range: start..(start + 1 + (i * 7 % 20)).min(100),
                    kind: [
                        HighlightKind::Selection,
                        HighlightKind::SearchActive,
                        HighlightKind::SearchPassive,
                    ][(i + seed) % 3],
                });
            }
            let mut covered = 0;
            visit_highlight_segments(&ranges, 100, |segment, kind| {
                assert_eq!(segment.start, covered);
                for offset in segment.clone() {
                    assert_eq!(kind, highlight_kind_for_segment(&ranges, offset));
                }
                covered = segment.end;
            });
            assert_eq!(covered, 100);
        }
    }

    #[test]
    fn sweep_preserves_unicode_byte_boundaries_and_ignores_empty_ranges() {
        let text = "é東京🦀x";
        let map = CharByteMap::build(text);
        let ranges = vec![
            TextHighlightRange {
                range: 1..3,
                kind: HighlightKind::SearchPassive,
            },
            TextHighlightRange {
                range: 2..4,
                kind: HighlightKind::SearchActive,
            },
            TextHighlightRange {
                range: 2..2,
                kind: HighlightKind::Selection,
            },
        ];
        let mut reconstructed = String::new();
        visit_highlight_segments(&ranges, map.char_len(), |range, _| {
            reconstructed.push_str(&text[map.byte_offset(range.start)..map.byte_offset(range.end)]);
        });
        assert_eq!(reconstructed, text);
    }
}
