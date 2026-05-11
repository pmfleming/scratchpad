use super::*;

const PREVIEW_MAX_CHARS: usize = 80;

#[derive(Clone, Copy)]
pub(super) struct ReplacementPreviewContext<'a> {
    pub(super) ui: &'a egui::Ui,
    pub(super) galley: &'a Arc<egui::Galley>,
    pub(super) galley_pos: egui::Pos2,
    pub(super) rect: egui::Rect,
    pub(super) options: TextEditOptions<'a>,
    pub(super) char_offset_base: usize,
    pub(super) slice_end: usize,
    pub(super) display_map: Option<&'a DisplayTextMap>,
}

pub(super) fn paint_replacement_previews(
    context: ReplacementPreviewContext<'_>,
    view: &EditorViewState,
) {
    let Some(preview) = view.search_replacement_preview.as_ref() else {
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
    preview
        .entries
        .iter()
        .filter(|entry| entry.range.start < slice_range.end && entry.range.end > slice_range.start)
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
    let local_start = context
        .display_map
        .map(|map| map.doc_to_display_cursor(doc_local_start))
        .unwrap_or(doc_local_start);
    let local_end = context
        .display_map
        .map(|map| map.doc_to_display_cursor(doc_local_end))
        .unwrap_or(doc_local_end);
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
