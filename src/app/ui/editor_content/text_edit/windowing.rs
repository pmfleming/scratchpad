use super::{
    TextEditOptions, TextEditOutcome, VISIBLE_ROW_OVERSCAN, layout_job_with_highlights,
    paint_text_cursor, selection_char_range, visible_window_y_offset, windowed_char_range,
    windowed_search_highlights,
};
use crate::app::domain::{RenderedTextWindow, SearchHighlightState};
use eframe::egui;

pub(super) fn repaint_visible_window_overlay(
    ui: &mut egui::Ui,
    outcome: &TextEditOutcome,
    visible_window: &RenderedTextWindow,
    search_highlights: &SearchHighlightState,
    options: TextEditOptions<'_>,
) {
    let selection_range = windowed_char_range(
        outcome.cursor_range.and_then(selection_char_range),
        &visible_window.char_range,
    );
    let search_highlights =
        windowed_search_highlights(search_highlights, &visible_window.char_range);
    if selection_range.is_none() && search_highlights.ranges.is_empty() {
        return;
    }

    let overlay_galley = ui.fonts_mut(|fonts| {
        fonts.layout_job(layout_job_with_highlights(
            &visible_window.text,
            &search_highlights,
            selection_range,
            super::highlighting::HighlightLayoutStyle {
                wrap_width: f32::INFINITY,
                word_wrap: false,
                font_id: options.editor_font_id,
                text_color: options.text_color,
                highlight: options.highlight_style,
                dark_mode: ui.visuals().dark_mode,
            },
        ))
    });

    let painter = ui.painter_at(outcome.text_clip_rect.expand(1.0));
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let y_offset = visible_window_y_offset(visible_window, row_height);
    let galley_pos = outcome.galley_pos + egui::vec2(0.0, y_offset);
    painter.galley(
        galley_pos - egui::vec2(overlay_galley.rect.left(), 0.0),
        overlay_galley.clone(),
        options.text_color,
    );

    if outcome.focused
        && let Some(cursor) = outcome
            .cursor_range
            .map(|range| range.primary.index)
            .filter(|index| {
                *index >= visible_window.char_range.start && *index <= visible_window.char_range.end
            })
            .map(|index| egui::text::CCursor::new(index - visible_window.char_range.start))
    {
        paint_text_cursor(
            ui,
            &painter,
            &overlay_galley,
            galley_pos,
            cursor,
            options.editor_font_id,
        );
    }
}

pub(super) fn visible_window_y_offset(visible_window: &RenderedTextWindow, row_height: f32) -> f32 {
    let row_offset = visible_window
        .layout_row_offset
        .max(visible_window.row_range.start);
    row_offset as f32 * row_height
}

pub(super) fn visible_row_range_for_galley(
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    clip_rect: egui::Rect,
) -> Option<std::ops::Range<usize>> {
    let first_visible = galley
        .rows
        .iter()
        .position(|row| galley_pos.y + row.max_y() >= clip_rect.top())?;
    let last_visible = galley
        .rows
        .iter()
        .rposition(|row| galley_pos.y + row.min_y() <= clip_rect.bottom())
        .unwrap_or(first_visible);
    let start = first_visible.saturating_sub(VISIBLE_ROW_OVERSCAN);
    let end = (last_visible + 1 + VISIBLE_ROW_OVERSCAN).min(galley.rows.len());
    Some(start..end)
}
