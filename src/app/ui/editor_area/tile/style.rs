use crate::app::app_state::ScratchpadApp;
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::ui::editor_content::{EditorContentStyle, EditorHighlightStyle, TextEditOptions};
use eframe::egui;

pub(super) fn editor_font_id(font_size: f32) -> egui::FontId {
    egui::FontId::new(font_size, egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()))
}

pub(super) fn editor_content_style<'a>(
    app: &ScratchpadApp,
    is_active: bool,
    request_focus: bool,
    editor_font_id: &'a egui::FontId,
) -> EditorContentStyle<'a> {
    EditorContentStyle {
        editor_gutter: app.editor_gutter(),
        viewport: None,
        previous_snapshot: None,
        text_edit: TextEditOptions::new(
            request_focus,
            app.word_wrap(),
            editor_font_id,
            app.editor_text_color(),
            EditorHighlightStyle::new(
                app.editor_text_highlight_color(),
                app.editor_text_highlight_text_color(),
            ),
        )
        .with_layout_cache_warming(is_active || request_focus),
        background_color: app.editor_background_color(),
    }
}
