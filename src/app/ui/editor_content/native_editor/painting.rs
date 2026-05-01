use super::types::CharCursor;
use crate::app::domain::{CursorRevealMode, EditorViewState};
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
}

#[derive(Default)]
pub(super) struct CursorPaintOutcome {
    pub(super) reveal_attempted: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_editor(
    ui: &mut egui::Ui,
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    focused: bool,
    changed: bool,
    char_offset_base: usize,
    slice_chars: usize,
) -> CursorPaintOutcome {
    paint_galley(ui, galley, galley_pos, options.text_color);
    paint_replacement_previews(
        ReplacementPreviewContext {
            ui,
            galley,
            galley_pos,
            rect,
            options,
            char_offset_base,
            slice_end: char_offset_base.saturating_add(slice_chars),
        },
        view,
    );

    if !focused {
        return CursorPaintOutcome::default();
    }

    if let Some(cursor_range) = &view.cursor_range
        && !changed
    {
        let galley_local_cursor_rect = galley
            .pos_from_cursor(local_cursor(cursor_range.primary, char_offset_base).to_egui_ccursor())
            .expand(1.5);
        let cursor_rect = galley_local_cursor_rect.translate(galley_pos.to_vec2());
        // Reveal targets must be in scroll-content coordinates. The editor rect
        // spans the full document and starts at the content origin, so subtract
        // `rect.min` to translate the screen-space cursor rect into content space.
        // (The slice galley is offset by `start_line * row_height` within the
        // rect, so galley-local coords are NOT content coords.)
        let cursor_rect_content = cursor_rect.translate(-rect.min.to_vec2());
        return paint_cursor_effects(ui, rect, cursor_rect, cursor_rect_content, view);
    }

    CursorPaintOutcome::default()
}

fn paint_replacement_previews(context: ReplacementPreviewContext<'_>, view: &EditorViewState) {
    let Some(replacement) = view.search_replacement_preview.as_deref() else {
        return;
    };
    let slice_range = context.char_offset_base..context.slice_end;
    for range in &view.search_highlights.ranges {
        if !slice_range.contains(&range.start) {
            continue;
        }
        paint_replacement_preview(context, range.clone(), replacement);
    }
}

fn paint_replacement_preview(
    context: ReplacementPreviewContext<'_>,
    range: Range<usize>,
    replacement: &str,
) {
    let local_start = range.start.saturating_sub(context.char_offset_base);
    let local_end = range
        .end
        .min(context.slice_end)
        .saturating_sub(context.char_offset_base);
    if local_start >= local_end {
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
    let top = start_pos.min.y.min(end_pos.min.y);
    let base_left = start_pos.min.x.min(end_pos.min.x);
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
    let preview_rect = egui::Rect::from_min_size(
        context.galley_pos + egui::vec2(base_left, top),
        egui::vec2(label_width.max(8.0) + 8.0, row_height.max(1.0)),
    )
    .intersect(context.rect.expand(1.0));
    if preview_rect.width() <= 0.0 || preview_rect.height() <= 0.0 {
        return;
    }

    let painter = context.ui.painter_at(context.rect.expand(1.0));
    let fill = context
        .options
        .highlight_style
        .active_background(context.ui.visuals().dark_mode)
        .gamma_multiply(0.82);
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

pub(super) fn local_cursor(cursor: CharCursor, char_offset_base: usize) -> CharCursor {
    CharCursor {
        index: cursor.index.saturating_sub(char_offset_base),
        prefer_next_row: cursor.prefer_next_row,
    }
}

pub(super) fn paint_galley(
    ui: &egui::Ui,
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    text_color: egui::Color32,
) {
    let offset = galley_pos - egui::vec2(galley.rect.left(), 0.0);
    ui.painter().galley(offset, galley.clone(), text_color);
}

fn paint_cursor_effects(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    cursor_rect_screen: egui::Rect,
    cursor_rect_content: egui::Rect,
    view: &mut EditorViewState,
) -> CursorPaintOutcome {
    let reveal_mode = view.cursor_reveal_mode();
    paint_cursor(ui, rect, cursor_rect_screen);
    publish_ime_output(ui, rect, cursor_rect_screen, view);
    if let Some(mode) = reveal_mode {
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
        view.request_intent(ScrollIntent::Reveal {
            rect: reveal_rect,
            align_y,
            align_x: Some(ScrollAlign::NearestWithMargin(0.0)),
        });
    }
    CursorPaintOutcome {
        reveal_attempted: reveal_mode.is_some(),
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

fn publish_ime_output(
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
        output.ime = Some(egui::output::IMEOutput { rect, cursor_rect });
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
