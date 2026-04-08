pub mod artifact;
pub mod gutter;
pub mod text_edit;

use crate::app::domain::{BufferState, EditorViewState, RenderedLayout};
use crate::app::theme::*;
use eframe::egui;

pub use artifact::{render_artifact_view, make_control_chars_visible, make_control_chars_clean};
pub use gutter::render_line_number_gutter;
pub use text_edit::{render_editor_text_edit, render_read_only_text_edit, build_layouter};

pub(crate) fn render_editor_content(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) -> bool {
    egui::Frame::NONE
        .fill(EDITOR_BG)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            ui.horizontal_top(|ui| {
                if view.show_line_numbers {
                    render_line_number_gutter(ui, buffer, view, previous_layout, editor_font_id);
                    ui.separator();
                }

                if buffer.artifact_summary.has_control_chars() {
                    render_artifact_view(ui, buffer, view, word_wrap, editor_font_id)
                } else {
                    render_editor_text_edit(ui, buffer, view, word_wrap, editor_font_id)
                }
            })
            .inner
        })
        .inner
}
