pub mod artifact;
pub mod gutter;
pub mod native_editor;

use crate::app::domain::{BufferState, EditorViewState, RenderedLayout, ViewId};
use crate::app::ui::widget_ids;
use eframe::egui;

pub use artifact::{make_control_chars_clean, make_control_chars_visible};
pub use gutter::{LineNumberGutterInput, render_line_number_gutter};
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
                        let published_viewport = view.published_viewport().cloned();
                        render_line_number_gutter(
                            ui,
                            LineNumberGutterInput {
                                buffer,
                                previous_layout: style.previous_layout,
                                display_snapshot: view.latest_display_snapshot.as_ref(),
                                published_viewport: published_viewport.as_ref(),
                                font_id: style.text_edit.editor_font_id,
                                text_color: style.text_edit.text_color,
                                background_color: style.background_color,
                            },
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
    // Single viewport-first render path. The visible-window/focused-window
    // forks were removed in Phase 4+5 of the scrolling rebuild — the unified
    // renderer is responsible for slicing to the viewport via
    // `scrolling::DisplaySnapshot`/`ViewportSlice`.
    render_editor_text_edit(ui, buffer, view, style.text_edit, style.viewport)
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

// Phase 4+5: tests for the WindowRenderMode/preferred_window_render_mode
// helpers were deleted along with those helpers. Replacement coverage for the
// unified viewport-first render path will be added in Phase 6.
