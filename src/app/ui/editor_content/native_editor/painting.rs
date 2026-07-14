use super::layout::DisplayTextMap;
use super::types::CharCursor;
use crate::app::domain::{
    CursorRevealMode, EditorViewState, ImePreeditState, SearchReplacementPreview,
};
use crate::app::ui::editor_content::native_editor::CursorRange;
use crate::app::ui::editor_content::native_editor::TextEditOptions;
use crate::app::ui::scrolling::{ScrollAlign, ScrollIntent};
use eframe::egui;
use std::ops::Range;
use std::sync::Arc;

const CURSOR_REVEAL_MARGIN_PX: f32 = 24.0;
const PREVIEW_MAX_CHARS: usize = 80;

#[derive(Clone, Copy)]
struct ReplacementPreviewContext<'a> {
    ui: &'a egui::Ui,
    galley: &'a Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    options: TextEditOptions<'a>,
    char_offset_base: usize,
    slice_end: usize,
    display_map: Option<&'a DisplayTextMap>,
}

pub(super) struct EditorFrame<'a> {
    pub(super) galley: &'a Arc<egui::Galley>,
    pub(super) galley_pos: egui::Pos2,
    pub(super) rect: egui::Rect,
    pub(super) options: TextEditOptions<'a>,
    pub(super) focused: bool,
    pub(super) char_offset_base: usize,
    pub(super) slice_chars: usize,
    pub(super) display_map: Option<&'a DisplayTextMap>,
    pub(super) active_selection: Option<Range<usize>>,
    pub(super) cursor_range: Option<CursorRange>,
    pub(super) cursor_reveal_mode: Option<CursorRevealMode>,
    pub(super) ime_preedit: Option<&'a ImePreeditState>,
    pub(super) replacement_preview: Option<&'a SearchReplacementPreview>,
}

#[derive(Clone, Copy)]
struct SelectionPaintContext<'a> {
    ui: &'a egui::Ui,
    galley: &'a egui::Galley,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    options: TextEditOptions<'a>,
    char_offset_base: usize,
    slice_chars: usize,
    display_map: Option<&'a DisplayTextMap>,
}

#[derive(Clone, Copy)]
struct ImePreeditPaintContext<'a> {
    ui: &'a egui::Ui,
    galley: &'a Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    options: TextEditOptions<'a>,
    focused: bool,
    char_offset_base: usize,
    display_map: Option<&'a DisplayTextMap>,
}

#[derive(Default)]
pub(super) struct CursorPaintOutcome {
    pub(super) reveal_attempted: bool,
    pub(super) reveal_intent: Option<ScrollIntent>,
    pub(super) ime_geometry: Option<(egui::Rect, egui::Rect)>,
}

pub(super) fn paint_editor(ui: &mut egui::Ui, request: EditorFrame<'_>) -> CursorPaintOutcome {
    paint_contiguous_selection_background(
        SelectionPaintContext {
            ui,
            galley: request.galley,
            galley_pos: request.galley_pos,
            rect: request.rect,
            options: request.options,
            char_offset_base: request.char_offset_base,
            slice_chars: request.slice_chars,
            display_map: request.display_map,
        },
        request.active_selection.as_ref(),
    );
    paint_galley(
        ui,
        request.galley,
        request.galley_pos,
        request.options.text_color,
    );
    paint_replacement_previews(
        ReplacementPreviewContext {
            ui,
            galley: request.galley,
            galley_pos: request.galley_pos,
            rect: request.rect,
            options: request.options,
            char_offset_base: request.char_offset_base,
            display_map: request.display_map,
            slice_end: request.char_offset_base.saturating_add(request.slice_chars),
        },
        request.replacement_preview,
    );
    paint_ime_preedit(
        ImePreeditPaintContext {
            ui,
            galley: request.galley,
            galley_pos: request.galley_pos,
            rect: request.rect,
            options: request.options,
            focused: request.focused,
            char_offset_base: request.char_offset_base,
            display_map: request.display_map,
        },
        request.ime_preedit,
        request.cursor_range.as_ref(),
    );

    if !request.focused {
        return CursorPaintOutcome::default();
    }

    if let Some(cursor_range) = &request.cursor_range {
        let galley_local_cursor_rect = cursor_rect_for_galley(
            ui,
            request.galley,
            request.options,
            local_cursor_for_slice(
                cursor_range.primary,
                request.char_offset_base,
                request.slice_chars,
                request.display_map,
            ),
        );
        let cursor_rect = galley_local_cursor_rect
            .translate(galley_screen_offset(request.galley, request.galley_pos).to_vec2());
        // Reveal targets must be in scroll-content coordinates. The editor rect
        // spans the full document and starts at the content origin, so subtract
        // `rect.min` to translate the screen-space cursor rect into content space.
        // (The slice galley is offset by `start_line * row_height` within the
        // rect, so galley-local coords are NOT content coords.)
        let cursor_rect_content = cursor_rect.translate(-request.rect.min.to_vec2());
        return paint_cursor_effects(
            ui,
            request.rect,
            cursor_rect,
            cursor_rect_content,
            request.cursor_reveal_mode,
        );
    }

    CursorPaintOutcome::default()
}

fn paint_ime_preedit(
    context: ImePreeditPaintContext<'_>,
    ime_preedit: Option<&ImePreeditState>,
    cursor_range: Option<&CursorRange>,
) {
    let Some(preedit) = ime_preedit.filter(|preedit| !preedit.text.is_empty()) else {
        return;
    };
    let Some(cursor_range) = cursor_range else {
        return;
    };
    if !context.focused {
        return;
    }

    let cursor_rect = cursor_rect_for_galley(
        context.ui,
        context.galley,
        context.options,
        local_cursor(
            cursor_range.primary,
            context.char_offset_base,
            context.display_map,
        ),
    )
    .translate(galley_screen_offset(context.galley, context.galley_pos).to_vec2());
    if !cursor_rect.intersects(context.rect) {
        return;
    }

    let font_id = context.options.editor_font_id.clone();
    let text_color = context.options.text_color;
    let galley = context
        .ui
        .fonts_mut(|fonts| fonts.layout_no_wrap(preedit.text.clone(), font_id.clone(), text_color));
    let origin = egui::pos2(cursor_rect.center().x, cursor_rect.min.y);
    let preedit_rect =
        egui::Rect::from_min_size(origin, galley.rect.size()).intersect(context.rect);
    if preedit_rect.width() <= 0.0 || preedit_rect.height() <= 0.0 {
        return;
    }

    let painter = context.ui.painter_at(context.rect.expand(1.0));
    painter.galley(origin, galley, text_color);
    let underline_y = (origin.y + preedit_rect.height()).min(context.rect.bottom());
    painter.line_segment(
        [
            egui::pos2(origin.x, underline_y),
            egui::pos2(preedit_rect.right(), underline_y),
        ],
        egui::Stroke::new(1.0, text_color),
    );

    if let Some(active_range) = active_ime_range(preedit.active_range_chars.clone(), &preedit.text)
    {
        let active_left = origin.x
            + ime_prefix_width(
                context.ui,
                &preedit.text,
                active_range.start,
                font_id.clone(),
                text_color,
            );
        let active_right = origin.x
            + ime_prefix_width(
                context.ui,
                &preedit.text,
                active_range.end,
                font_id,
                text_color,
            );
        let active_rect = egui::Rect::from_min_max(
            egui::pos2(active_left, preedit_rect.top()),
            egui::pos2(active_right, preedit_rect.bottom()),
        )
        .intersect(context.rect);
        if active_rect.width() > 0.0 {
            painter.line_segment(
                [
                    egui::pos2(active_rect.left(), underline_y),
                    egui::pos2(active_rect.right(), underline_y),
                ],
                egui::Stroke::new(2.0, context.ui.visuals().selection.stroke.color),
            );
            painter.line_segment(
                [active_rect.right_top(), active_rect.right_bottom()],
                context.ui.visuals().text_cursor.stroke,
            );
        }
    }
}

fn active_ime_range(
    range: Option<std::ops::Range<usize>>,
    text: &str,
) -> Option<std::ops::Range<usize>> {
    let char_len = text.chars().count();
    let range = range?;
    let start = range.start.min(char_len);
    let end = range.end.min(char_len);
    (start < end).then_some(start..end)
}

fn ime_prefix_width(
    ui: &egui::Ui,
    text: &str,
    char_count: usize,
    font_id: egui::FontId,
    text_color: egui::Color32,
) -> f32 {
    if char_count == 0 {
        return 0.0;
    }
    let prefix = text.chars().take(char_count).collect::<String>();
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(prefix, font_id, text_color)
            .rect
            .width()
    })
}

fn cursor_rect_for_galley(
    ui: &egui::Ui,
    galley: &egui::Galley,
    options: TextEditOptions<'_>,
    cursor: CharCursor,
) -> egui::Rect {
    let row_height = ui
        .fonts_mut(|fonts| fonts.row_height(options.editor_font_id))
        .max(options.editor_font_id.size)
        .max(1.0);
    let rect = if galley.rows.is_empty() {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1.0, row_height))
    } else {
        galley.pos_from_cursor(cursor.to_egui_ccursor())
    };

    if !rect.is_finite() || rect.height() >= row_height * 0.5 {
        return rect.expand(1.5);
    }

    egui::Rect::from_min_size(
        egui::pos2(rect.center().x, rect.min.y),
        egui::vec2(1.0, row_height),
    )
    .expand(1.5)
}

fn paint_contiguous_selection_background(
    context: SelectionPaintContext<'_>,
    active_selection: Option<&Range<usize>>,
) {
    let Some(selection) = active_selection.filter(|range| range.start < range.end) else {
        return;
    };
    let slice_start = context.char_offset_base;
    let slice_end = context.char_offset_base.saturating_add(context.slice_chars);
    let doc_local_start = selection.start.max(slice_start).saturating_sub(slice_start);
    let doc_local_end = selection.end.min(slice_end).saturating_sub(slice_start);
    let local_start = context.display_map.map_or(doc_local_start, |map| {
        map.doc_to_display_cursor(doc_local_start)
    });
    let local_end = context.display_map.map_or(doc_local_end, |map| {
        map.doc_to_display_cursor(doc_local_end)
    });
    if local_start >= local_end {
        return;
    }

    let fill = context
        .options
        .highlight_style
        .active_background(context.ui.visuals().dark_mode);
    let painter = context.ui.painter_at(context.rect.expand(1.0));
    let mut row_start = 0usize;
    for row in &context.galley.rows {
        let row_text_chars = usize::from(row.char_count_excluding_newline());
        let row_end = row_start.saturating_add(row.char_count_including_newline().into());
        if local_start < row_end && local_end > row_start {
            let start_col = local_start.saturating_sub(row_start).min(row_text_chars);
            let end_col = local_end.saturating_sub(row_start).min(row_text_chars);
            let selection_reaches_line_end =
                row.ends_with_newline && local_end > row_start + row_text_chars;
            let selection_covers_whole_row = local_start <= row_start && local_end >= row_end;
            let left = row_screen_x(
                context.galley,
                context.galley_pos,
                row.pos.x,
                row.x_offset(egui::text::CharIndex(start_col)),
            );
            let right = if selection_reaches_line_end || selection_covers_whole_row {
                context.rect.right()
            } else {
                row_screen_x(
                    context.galley,
                    context.galley_pos,
                    row.pos.x,
                    row.x_offset(egui::text::CharIndex(end_col)),
                )
            };
            let highlight_rect = egui::Rect::from_min_max(
                egui::pos2(left.min(right), context.galley_pos.y + row.min_y()),
                egui::pos2(left.max(right), context.galley_pos.y + row.max_y()),
            )
            .intersect(context.rect.expand(1.0));
            if highlight_rect.width() > 0.0 && highlight_rect.height() > 0.0 {
                painter.rect_filled(highlight_rect, egui::CornerRadius::ZERO, fill);
            }
        }
        row_start = row_end;
    }
}

fn row_screen_x(galley: &egui::Galley, galley_pos: egui::Pos2, row_x: f32, column_x: f32) -> f32 {
    galley_pos.x + column_x + row_x - galley.rect.left()
}

fn paint_replacement_previews(
    context: ReplacementPreviewContext<'_>,
    preview: Option<&SearchReplacementPreview>,
) {
    let Some(preview) = preview else {
        return;
    };
    let slice_range = context.char_offset_base..context.slice_end;
    for entry in visible_preview_entries(preview, &slice_range) {
        if !slice_range.contains(&entry.range.start) {
            continue;
        }
        paint_replacement_preview(context, entry.range.clone(), &entry.replacement);
    }
}

fn visible_preview_entries<'a>(
    preview: &'a SearchReplacementPreview,
    slice_range: &Range<usize>,
) -> impl Iterator<Item = &'a crate::app::domain::SearchReplacementPreviewEntry> {
    let start_index = preview
        .entries
        .partition_point(|entry| entry.range.end <= slice_range.start);
    let end_index = preview
        .entries
        .partition_point(|entry| entry.range.start < slice_range.end);
    preview.entries[start_index..end_index].iter()
}

fn paint_replacement_preview(
    context: ReplacementPreviewContext<'_>,
    range: Range<usize>,
    replacement: &str,
) {
    let doc_local_start = range.start.saturating_sub(context.char_offset_base);
    let doc_local_end = range
        .end
        .min(context.slice_end)
        .saturating_sub(context.char_offset_base);
    let local_start = context.display_map.map_or(doc_local_start, |map| {
        map.doc_to_display_cursor(doc_local_start)
    });
    let local_end = context.display_map.map_or(doc_local_end, |map| {
        map.doc_to_display_cursor(doc_local_end)
    });
    if local_start > local_end {
        return;
    }

    let start_pos = context
        .galley
        .pos_from_cursor(CharCursor::new(local_start).to_egui_ccursor());
    let end_pos = context
        .galley
        .pos_from_cursor(CharCursor::new(local_end).to_egui_ccursor());
    let row_height = context
        .ui
        .fonts_mut(|fonts| fonts.row_height(context.options.editor_font_id));
    let replacement_label = preview_label(replacement);
    let label_width = context.ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                replacement_label.clone(),
                context.options.editor_font_id.clone(),
                context.options.highlight_style.text_color(),
            )
            .rect
            .width()
    });
    let preview_rect = replacement_preview_rect(
        galley_screen_offset(context.galley, context.galley_pos),
        start_pos,
        end_pos,
        row_height,
        label_width,
        context.rect.expand(1.0),
    );
    if preview_rect.width() <= 0.0 || preview_rect.height() <= 0.0 {
        return;
    }

    let painter = context.ui.painter_at(context.rect.expand(1.0));
    let fill = context
        .options
        .highlight_style
        .active_background(context.ui.visuals().dark_mode);
    let stroke = egui::Stroke::new(
        1.0,
        context
            .options
            .highlight_style
            .text_color()
            .gamma_multiply(0.75),
    );
    painter.rect(
        preview_rect,
        egui::CornerRadius::same(3),
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );
    if !replacement_label.is_empty() {
        painter.text(
            preview_rect.left_center() + egui::vec2(4.0, 0.0),
            egui::Align2::LEFT_CENTER,
            replacement_label,
            context.options.editor_font_id.clone(),
            context.options.highlight_style.text_color(),
        );
    }
}

fn replacement_preview_rect(
    galley_pos: egui::Pos2,
    start_pos: egui::Rect,
    end_pos: egui::Rect,
    row_height: f32,
    label_width: f32,
    clip_rect: egui::Rect,
) -> egui::Rect {
    let top = start_pos.min.y.min(end_pos.min.y);
    let left = start_pos.min.x.min(end_pos.min.x);
    let match_right = start_pos.min.x.max(end_pos.min.x);
    let label_right = left + label_width.max(8.0) + 8.0;
    egui::Rect::from_min_max(
        galley_pos + egui::vec2(left, top),
        galley_pos + egui::vec2(match_right.max(label_right), top + row_height.max(1.0)),
    )
    .intersect(clip_rect)
}

fn preview_label(replacement: &str) -> String {
    let flattened = replacement.replace(['\r', '\n'], " ");
    let mut label = flattened
        .chars()
        .take(PREVIEW_MAX_CHARS)
        .collect::<String>();
    if flattened.chars().count() > PREVIEW_MAX_CHARS {
        label.push_str("...");
    }
    label
}

pub(super) fn local_cursor(
    cursor: CharCursor,
    char_offset_base: usize,
    display_map: Option<&DisplayTextMap>,
) -> CharCursor {
    let doc_local = cursor.index.saturating_sub(char_offset_base);
    CharCursor {
        index: display_map.map_or(doc_local, |map| map.doc_to_display_cursor(doc_local)),
        prefer_next_row: cursor.prefer_next_row,
    }
}

fn local_cursor_for_slice(
    cursor: CharCursor,
    char_offset_base: usize,
    slice_chars: usize,
    display_map: Option<&DisplayTextMap>,
) -> CharCursor {
    let doc_local = cursor
        .index
        .saturating_sub(char_offset_base)
        .min(slice_chars);
    CharCursor {
        index: display_map.map_or(doc_local, |map| {
            map.doc_to_display_cursor(doc_local).min(map.display_len())
        }),
        prefer_next_row: cursor.prefer_next_row,
    }
}

pub(super) fn paint_galley(
    ui: &egui::Ui,
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    text_color: egui::Color32,
) {
    let offset = galley_screen_offset(galley, galley_pos);
    ui.painter().galley(offset, galley.clone(), text_color);
}

pub(super) fn galley_screen_offset(galley: &egui::Galley, galley_pos: egui::Pos2) -> egui::Pos2 {
    galley_pos - egui::vec2(galley.rect.left(), 0.0)
}

fn paint_cursor_effects(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    cursor_rect_screen: egui::Rect,
    cursor_rect_content: egui::Rect,
    reveal_mode: Option<CursorRevealMode>,
) -> CursorPaintOutcome {
    paint_cursor(ui, rect, cursor_rect_screen);
    let reveal_intent = reveal_mode.map(|mode| {
        let align_y = match mode {
            CursorRevealMode::KeepVisible => {
                Some(ScrollAlign::NearestWithMargin(CURSOR_REVEAL_MARGIN_PX))
            }
            CursorRevealMode::KeepHorizontalVisible => None,
            CursorRevealMode::Center => Some(ScrollAlign::Center),
        };
        let reveal_rect = egui::Rect::from_min_max(
            egui::pos2(cursor_rect_content.left(), cursor_rect_content.center().y),
            egui::pos2(cursor_rect_content.right(), cursor_rect_content.center().y),
        );
        ScrollIntent::Reveal {
            rect: reveal_rect,
            align_y,
            align_x: Some(ScrollAlign::NearestWithMargin(0.0)),
        }
    });
    CursorPaintOutcome {
        reveal_attempted: reveal_mode.is_some(),
        reveal_intent,
        ime_geometry: Some((rect, cursor_rect_screen)),
    }
}

fn paint_cursor(ui: &egui::Ui, rect: egui::Rect, cursor_rect: egui::Rect) {
    let painter = ui.painter_at(rect.expand(1.0));
    let stroke = ui.visuals().text_cursor.stroke;
    painter.line_segment(
        [cursor_rect.center_top(), cursor_rect.center_bottom()],
        (stroke.width, stroke.color),
    );
}

pub(super) fn publish_ime_output(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    cursor_rect: egui::Rect,
    view: &mut EditorViewState,
) {
    let to_global = ui
        .ctx()
        .layer_transform_to_global(ui.layer_id())
        .unwrap_or_default();
    let visible_rect = rect.intersect(ui.clip_rect());
    if !visible_rect.is_finite() || visible_rect.width() <= 0.0 || visible_rect.height() <= 0.0 {
        return;
    }
    let rect = to_global * visible_rect;
    let cursor_rect = to_global * cursor_rect;
    if !view.mark_ime_output(rect, cursor_rect) {
        return;
    }

    ui.output_mut(|output| {
        output.ime = Some(egui::output::IMEOutput {
            rect,
            cursor_rect,
            should_interrupt_composition: false,
        });
    });
}

pub(super) fn consume_cursor_reveal(
    view: &mut EditorViewState,
    changed: bool,
    reveal_attempted: bool,
) {
    if !changed && (view.cursor_reveal_mode().is_none() || reveal_attempted) {
        view.clear_cursor_reveal();
    }
}

#[cfg(test)]
mod tests {
    use super::replacement_preview_rect;
    use eframe::egui;

    #[test]
    fn replacement_preview_rect_covers_original_match_when_label_is_shorter() {
        let rect = replacement_preview_rect(
            egui::pos2(10.0, 20.0),
            egui::Rect::from_min_size(egui::pos2(5.0, 7.0), egui::vec2(1.0, 16.0)),
            egui::Rect::from_min_size(egui::pos2(40.0, 7.0), egui::vec2(1.0, 16.0)),
            16.0,
            8.0,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 200.0)),
        );

        assert_eq!(rect.min, egui::pos2(15.0, 27.0));
        assert_eq!(rect.max, egui::pos2(50.0, 43.0));
    }

    #[test]
    fn replacement_preview_rect_expands_for_longer_label() {
        let rect = replacement_preview_rect(
            egui::pos2(0.0, 0.0),
            egui::Rect::from_min_size(egui::pos2(5.0, 7.0), egui::vec2(1.0, 16.0)),
            egui::Rect::from_min_size(egui::pos2(15.0, 7.0), egui::vec2(1.0, 16.0)),
            16.0,
            40.0,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 200.0)),
        );

        assert_eq!(rect.min, egui::pos2(5.0, 7.0));
        assert_eq!(rect.max, egui::pos2(53.0, 23.0));
    }
}
