use super::types::CharCursor;
use crate::app::domain::{CursorRevealMode, EditorViewState};
use crate::app::ui::editor_content::native_editor::TextEditOptions;
use crate::app::ui::scrolling::{ScrollAlign, ScrollIntent};
use eframe::egui;
use std::sync::Arc;

const CURSOR_REVEAL_MARGIN_PX: f32 = 24.0;

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
) -> CursorPaintOutcome {
    paint_galley(ui, galley, galley_pos, options.text_color);

    if !focused {
        return CursorPaintOutcome::default();
    }

    if let Some(cursor_range) = &view.cursor_range
        && !changed
    {
        let content_cursor_rect = galley
            .pos_from_cursor(local_cursor(cursor_range.primary, char_offset_base).to_egui_ccursor())
            .expand(1.5);
        let cursor_rect = content_cursor_rect.translate(galley_pos.to_vec2());
        return paint_cursor_effects(ui, rect, cursor_rect, content_cursor_rect, view);
    }

    CursorPaintOutcome::default()
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
