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
    render_editor_focused_text_window, render_editor_text_edit, render_editor_visible_text_window,
    render_read_only_text_edit,
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
    pub(crate) is_active: bool,
    pub(crate) viewport: Option<egui::Rect>,
    pub(crate) previous_layout: Option<&'a RenderedLayout>,
    pub(crate) text_edit: TextEditOptions<'a>,
    pub(crate) background_color: egui::Color32,
}

const LARGE_BUFFER_VIEWPORT_BYTES: usize = 5 * 1024 * 1024;

pub(crate) fn render_editor_content(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    view_id: ViewId,
    style: EditorContentStyle<'_>,
) -> EditorContentOutcome {
    let inspect_control_chars =
        buffer.artifact_summary.has_control_chars() && view.show_control_chars;
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

                    if inspect_control_chars {
                        render_artifact_view(
                            ui,
                            buffer,
                            view,
                            style.previous_layout,
                            style.text_edit,
                        )
                    } else if should_prefer_focused_window(buffer, view, &style) {
                        render_editor_focused_text_window(
                            ui,
                            buffer,
                            view,
                            style.previous_layout,
                            style.text_edit,
                            style.viewport,
                        )
                        .unwrap_or_else(|| {
                            render_editor_text_edit(
                                ui,
                                buffer,
                                view,
                                style.text_edit,
                                style.viewport,
                            )
                        })
                    } else if should_prefer_visible_window(buffer, view, &style) {
                        render_editor_visible_text_window(
                            ui,
                            buffer,
                            view,
                            style.previous_layout,
                            style.text_edit,
                            style.viewport,
                        )
                        .unwrap_or_else(|| {
                            render_editor_text_edit(
                                ui,
                                buffer,
                                view,
                                style.text_edit,
                                style.viewport,
                            )
                        })
                    } else {
                        render_editor_text_edit(ui, buffer, view, style.text_edit, style.viewport)
                    }
                })
                .inner
            })
            .inner
            .into()
    })
    .inner
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

fn should_prefer_visible_window(
    buffer: &BufferState,
    view: &EditorViewState,
    style: &EditorContentStyle<'_>,
) -> bool {
    if style.text_edit.word_wrap || style.text_edit.request_focus || style.previous_layout.is_none()
    {
        return false;
    }

    !style.is_active
        || (buffer.current_file_length().bytes >= LARGE_BUFFER_VIEWPORT_BYTES
            && !view.editor_has_focus)
}

fn should_prefer_focused_window(
    buffer: &BufferState,
    view: &EditorViewState,
    style: &EditorContentStyle<'_>,
) -> bool {
    style.is_active
        && (view.editor_has_focus || style.text_edit.request_focus)
        && !style.text_edit.word_wrap
        && (style.previous_layout.is_some() || style.viewport.is_some())
        && buffer.current_file_length().bytes >= LARGE_BUFFER_VIEWPORT_BYTES
}
