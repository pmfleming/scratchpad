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
        editor_gutter: app.state.app_settings.editor_gutter(),
        viewport: None,
        previous_snapshot: None,
        gutter_snapshot: None,
        text_edit: TextEditOptions::new(
            request_focus,
            app.state.app_settings.word_wrap(),
            editor_font_id,
            app.state.app_settings.editor_text_color(),
            EditorHighlightStyle::new(
                app.state.app_settings.editor_text_highlight_color(),
                app.state.app_settings.editor_text_highlight_text_color(),
            ),
        )
        .with_alt_reserved_for_shortcuts(
            crate::app::platform::resolved_profile(app.state.app_settings.platform_profile())
                == crate::app::platform::PlatformProfile::Hyprland,
        )
        .with_layout_cache_warming(is_active || request_focus),
        background_color: app.state.app_settings.editor_background_color(),
    }
}
