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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowRenderMode {
    Focused,
    Visible,
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

    match preferred_window_render_mode(view, style) {
        Some(WindowRenderMode::Focused) => render_editor_focused_text_window(
            ui,
            buffer,
            view,
            style.previous_layout,
            style.text_edit,
            style.viewport,
        )
        .unwrap_or_else(|| {
            render_editor_text_edit(ui, buffer, view, style.text_edit, style.viewport)
        }),
        Some(WindowRenderMode::Visible) => render_editor_visible_text_window(
            ui,
            buffer,
            view,
            style.previous_layout,
            style.text_edit,
            style.viewport,
        )
        .unwrap_or_else(|| {
            render_editor_text_edit(ui, buffer, view, style.text_edit, style.viewport)
        }),
        None => render_editor_text_edit(ui, buffer, view, style.text_edit, style.viewport),
    }
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

#[cfg(test)]
fn should_prefer_visible_window(view: &EditorViewState, style: &EditorContentStyle<'_>) -> bool {
    matches!(
        preferred_window_render_mode(view, style),
        Some(WindowRenderMode::Visible)
    )
}

#[cfg(test)]
fn should_prefer_focused_window(view: &EditorViewState, style: &EditorContentStyle<'_>) -> bool {
    matches!(
        preferred_window_render_mode(view, style),
        Some(WindowRenderMode::Focused)
    )
}

fn preferred_window_render_mode(
    view: &EditorViewState,
    style: &EditorContentStyle<'_>,
) -> Option<WindowRenderMode> {
    if style.text_edit.word_wrap || (style.previous_layout.is_none() && style.viewport.is_none()) {
        return None;
    }

    if style.is_active && (view.editor_has_focus || style.text_edit.request_focus) {
        return Some(WindowRenderMode::Focused);
    }

    (!style.text_edit.request_focus).then_some(WindowRenderMode::Visible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{self, FontId};

    fn editor_font_id() -> &'static FontId {
        static EDITOR_FONT_ID: std::sync::LazyLock<FontId> =
            std::sync::LazyLock::new(|| FontId::monospace(14.0));
        &EDITOR_FONT_ID
    }

    fn text_edit_options() -> TextEditOptions<'static> {
        TextEditOptions {
            request_focus: false,
            word_wrap: false,
            editor_font_id: editor_font_id(),
            text_color: egui::Color32::WHITE,
            highlight_style: EditorHighlightStyle::new(
                egui::Color32::LIGHT_BLUE,
                egui::Color32::BLACK,
            ),
        }
    }

    fn style<'a>(
        text_edit: TextEditOptions<'a>,
        previous_layout: Option<&'a RenderedLayout>,
    ) -> EditorContentStyle<'a> {
        EditorContentStyle {
            editor_gutter: 0,
            is_active: false,
            viewport: None,
            previous_layout,
            text_edit,
            background_color: egui::Color32::BLACK,
        }
    }

    #[test]
    fn visible_window_no_longer_depends_on_buffer_size() {
        let buffer = BufferState::new("notes.txt".to_owned(), "short text".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);
        let mut style = style(text_edit_options(), None);
        style.viewport = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(100.0, 100.0),
        ));

        assert!(should_prefer_visible_window(&view, &style));

        style.is_active = true;
        view.editor_has_focus = true;
        assert!(!should_prefer_visible_window(&view, &style));
    }

    #[test]
    fn focused_window_no_longer_depends_on_buffer_size() {
        let buffer = BufferState::new("notes.txt".to_owned(), "short text".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);
        let mut style = style(text_edit_options(), None);

        style.is_active = true;
        style.viewport = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(100.0, 100.0),
        ));
        view.editor_has_focus = true;
        assert!(should_prefer_focused_window(&view, &style));
    }

    #[test]
    fn visible_window_still_requires_viewport_or_layout_and_no_wrap() {
        let buffer = BufferState::new("notes.txt".to_owned(), "short text".to_owned(), None);
        let view = EditorViewState::new(buffer.id, false);
        let mut wrapped = text_edit_options();
        wrapped.word_wrap = true;

        assert!(!should_prefer_visible_window(
            &view,
            &style(text_edit_options(), None)
        ));

        let mut viewport_style = style(wrapped, None);
        viewport_style.viewport = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(100.0, 100.0),
        ));
        assert!(!should_prefer_visible_window(&view, &viewport_style));
    }
}
