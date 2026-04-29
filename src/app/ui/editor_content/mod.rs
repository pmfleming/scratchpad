pub mod artifact;
pub mod gutter;
pub mod native_editor;

use crate::app::domain::{BufferState, EditorViewState, RenderedLayout, ViewId};
use crate::app::ui::widget_ids;
use eframe::egui;

pub use artifact::{make_control_chars_clean, make_control_chars_visible, render_artifact_view};
pub use gutter::render_line_number_gutter;
pub use native_editor::{
    CursorRange, EditorHighlightStyle, TextEditOptions, build_layouter,
    render_editor_text_edit, render_read_only_text_edit,
};

pub(crate) struct EditorContentOutcome {
    pub(crate) changed: bool,
    pub(crate) focused: bool,
    pub(crate) request_editor_focus: bool,
    pub(crate) requested_scroll_offset: Option<egui::Vec2>,
    pub(crate) interaction_response: Option<egui::Response>,
}

pub(crate) struct EditorContentStyle<'a> {
    pub(crate) editor_gutter: u8,
    pub(crate) viewport: Option<egui::Rect>,
    pub(crate) previous_layout: Option<&'a RenderedLayout>,
    pub(crate) text_edit: TextEditOptions<'a>,
    pub(crate) background_color: egui::Color32,
}

pub(crate) fn render_editor_content(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    view_id: ViewId,
    style: EditorContentStyle<'_>,
) -> EditorContentOutcome {
    let gutter = i8::try_from(style.editor_gutter).unwrap_or(i8::MAX);
    widget_ids::scope(ui, ("editor_content", view_id), |ui| {
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
                            style.previous_layout,
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
    if buffer.artifact_summary.has_control_chars() && view.show_control_chars {
        return render_artifact_view(ui, buffer, view, style.previous_layout, style.text_edit);
    }

    render_editor_text_edit(ui, buffer, view, style.text_edit, style.viewport)
}

impl From<native_editor::EditorWidgetOutcome> for EditorContentOutcome {
    fn from(outcome: native_editor::EditorWidgetOutcome) -> Self {
        Self {
            changed: outcome.changed,
            focused: outcome.focused,
            request_editor_focus: outcome.request_editor_focus,
            requested_scroll_offset: outcome.requested_scroll_offset,
            interaction_response: Some(outcome.response),
        }
    }
}
