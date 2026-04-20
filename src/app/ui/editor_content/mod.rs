pub mod artifact;
pub mod gutter;
pub mod native_editor;

use crate::app::domain::{BufferState, EditorViewState, RenderedLayout};
use eframe::egui;

pub use artifact::{make_control_chars_clean, make_control_chars_visible, render_artifact_view};
pub use gutter::render_line_number_gutter;
pub use native_editor::{
    CursorRange, EditorHighlightStyle, TextEditOptions, build_layouter, render_editor_text_edit,
    render_editor_visible_text_window, render_read_only_text_edit,
};

pub(crate) struct EditorContentOutcome {
    pub(crate) changed: bool,
    pub(crate) focused: bool,
}

pub(crate) struct EditorContentStyle<'a> {
    pub(crate) editor_gutter: u8,
    pub(crate) is_active: bool,
    pub(crate) previous_layout: Option<&'a RenderedLayout>,
    pub(crate) text_edit: TextEditOptions<'a>,
    pub(crate) background_color: egui::Color32,
}

pub(crate) fn render_editor_content(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    style: EditorContentStyle<'_>,
) -> EditorContentOutcome {
    let inspect_control_chars =
        buffer.artifact_summary.has_control_chars() && view.show_control_chars;
    let gutter = i8::try_from(style.editor_gutter).unwrap_or(i8::MAX);
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
                        view,
                        style.previous_layout,
                        style.text_edit.editor_font_id,
                        style.text_edit.text_color,
                        style.background_color,
                    );
                    ui.separator();
                }

                if inspect_control_chars {
                    render_artifact_view(ui, buffer, view, style.previous_layout, style.text_edit)
                } else if !style.is_active {
                    render_editor_visible_text_window(
                        ui,
                        buffer,
                        view,
                        style.previous_layout,
                        style.text_edit,
                    )
                    .unwrap_or_else(|| render_editor_text_edit(ui, buffer, view, style.text_edit))
                } else {
                    render_editor_text_edit(ui, buffer, view, style.text_edit)
                }
            })
            .inner
        })
        .inner
        .into()
}

impl From<(bool, bool)> for EditorContentOutcome {
    fn from((changed, focused): (bool, bool)) -> Self {
        Self { changed, focused }
    }
}
