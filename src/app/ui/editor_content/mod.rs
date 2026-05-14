pub(crate) mod extent;
pub mod gutter;
pub mod native_editor;

use crate::app::domain::{BufferState, EditorViewState};
use crate::app::ui::scrolling::DisplaySnapshot;
use crate::app::ui::widget_ids;
use eframe::egui;

pub use gutter::render_line_number_gutter;
pub use native_editor::{
    CursorRange, EditorHighlightStyle, TextEditOptions, build_layouter, render_editor_text_edit,
};

pub(crate) struct EditorContentOutcome {
    pub(crate) changed: bool,
    pub(crate) focused: bool,
    pub(crate) request_editor_focus: bool,
    pub(crate) interaction_response: Option<egui::Response>,
}

pub(crate) struct EditorContentStyle<'a> {
    pub(crate) editor_gutter: u8,
    pub(crate) viewport: Option<egui::Rect>,
    pub(crate) previous_snapshot: Option<&'a DisplaySnapshot>,
    pub(crate) text_edit: TextEditOptions<'a>,
    pub(crate) background_color: egui::Color32,
}

pub(crate) fn render_editor_content(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    style: EditorContentStyle<'_>,
) -> EditorContentOutcome {
    let gutter = i8::try_from(style.editor_gutter).unwrap_or(i8::MAX);
    let content_rect = ui.available_rect_before_wrap();
    widget_ids::rect_scope(ui, content_rect, "editor_content", |ui| {
        egui::Frame::NONE
            .fill(style.background_color)
            .inner_margin(egui::Margin::same(gutter))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;

                ui.horizontal_top(|ui| {
                    if view.show_line_numbers {
                        render_line_number_gutter(
                            ui,
                            buffer,
                            style.viewport,
                            style.previous_snapshot,
                            style.text_edit.editor_font_id,
                            style.text_edit.text_color,
                            style.background_color,
                        );
                        ui.separator();
                    }

                    render_editor_body(ui, buffer, view, &style)
                })
                .inner
            })
            .inner
            .into()
    })
    .inner
}

fn render_editor_body(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    style: &EditorContentStyle<'_>,
) -> native_editor::EditorWidgetOutcome {
    let body_rect = ui.available_rect_before_wrap();
    render_editor_text_edit(
        ui,
        buffer,
        view,
        style.text_edit,
        body_viewport(style.viewport, ui.clip_rect(), body_rect),
    )
}

fn body_viewport(
    viewport: Option<egui::Rect>,
    clip_rect: egui::Rect,
    body_rect: egui::Rect,
) -> Option<egui::Rect> {
    viewport.map(|viewport| {
        let leading_content_width =
            (body_rect.left() - clip_rect.left()).clamp(0.0, viewport.width());
        let body_width = (viewport.width() - leading_content_width).max(1.0);
        egui::Rect::from_min_size(viewport.min, egui::vec2(body_width, viewport.height()))
    })
}

impl From<native_editor::EditorWidgetOutcome> for EditorContentOutcome {
    fn from(outcome: native_editor::EditorWidgetOutcome) -> Self {
        Self {
            changed: outcome.changed,
            focused: outcome.focused,
            request_editor_focus: outcome.request_editor_focus,
            interaction_response: Some(outcome.response),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::body_viewport;
    use eframe::egui;

    #[test]
    fn body_viewport_uses_text_lane_width_after_gutter() {
        let viewport = egui::Rect::from_min_size(egui::pos2(0.0, 40.0), egui::vec2(900.0, 300.0));
        let clip_rect = egui::Rect::from_min_size(egui::pos2(10.0, 40.0), egui::vec2(900.0, 300.0));
        let body_rect =
            egui::Rect::from_min_size(egui::pos2(70.0, 40.0), egui::vec2(1200.0, 300.0));

        let narrowed = body_viewport(Some(viewport), clip_rect, body_rect).unwrap();

        assert_eq!(narrowed.min, viewport.min);
        assert_eq!(narrowed.height(), 300.0);
        assert_eq!(narrowed.width(), 840.0);
    }

    #[test]
    fn body_viewport_uses_visible_viewport_instead_of_stale_available_width() {
        let viewport = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(600.0, 240.0));
        let clip_rect = egui::Rect::from_min_size(egui::pos2(100.0, 0.0), viewport.size());
        let body_rect =
            egui::Rect::from_min_size(egui::pos2(124.0, 0.0), egui::vec2(1200.0, 240.0));

        let narrowed = body_viewport(Some(viewport), clip_rect, body_rect).unwrap();

        assert_eq!(narrowed.width(), 576.0);
    }
}
