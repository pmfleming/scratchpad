use super::{
    EditorWidgetOutcome, TextEditOptions, highlighting, layout, painting, store_latest_snapshot,
    sync_ime_output_focus, types,
};
use crate::app::domain::EditorViewState;
use crate::app::ui::widget_ids;
use eframe::egui;

pub fn render_read_only_text_edit(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    text: String,
    desired_rows: usize,
    options: TextEditOptions<'_>,
) -> EditorWidgetOutcome {
    let selection_range = view
        .cursor_range
        .as_ref()
        .and_then(types::selection_char_range);

    let wrap_width = if options.word_wrap {
        ui.available_width()
    } else {
        f32::INFINITY
    };
    let galley = highlighting::build_galley(
        ui,
        &text,
        options,
        &view.search_highlights,
        selection_range,
        wrap_width,
    );

    let row_height = layout::editor_row_height(ui, options.editor_font_id);
    let desired_height = desired_rows.max(1) as f32 * row_height;
    let size = egui::vec2(
        layout::editor_desired_width(ui, &galley, options.word_wrap, None),
        desired_height,
    );
    let response = widget_ids::allocate_exact_rect_interact(
        ui,
        size,
        ("native_editor.empty", view.id),
        egui::Sense::click(),
        "native_editor.empty",
    );
    let rect = response.rect;

    if ui.is_rect_visible(rect) {
        painting::paint_galley(ui, &galley, rect.min, options.text_color);
    }

    let focused = response.has_focus() || response.gained_focus();
    sync_ime_output_focus(view, focused);
    store_latest_snapshot(view, &galley, row_height, false, None, 0, 0);
    view.cursor_range = None;
    view.editor_has_focus = focused;
    EditorWidgetOutcome {
        changed: false,
        focused,
        request_editor_focus: false,
        response,
    }
}
