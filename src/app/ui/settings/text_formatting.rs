use super::*;

pub(super) fn render_text_formatting_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_card(
        ui,
        "Text Formatting",
        "settings_font_card",
        egui_phosphor::regular::TEXT_ALIGN_JUSTIFY,
        "Font",
        "Choose the text appearance for editor content.",
        true,
        |ui| {
            render_font_family_row(ui, app);
            inner_divider(ui);
            render_font_size_row(ui, app);
            inner_divider(ui);
            render_gutter_row(ui, app);
            ui.add_space(SettingsUi::LAYOUT.preview_top_margin);
            render_preview_panel(ui, app);
        },
    );
    ui.add_space(SettingsUi::LAYOUT.card_gap);
    toggle_card(
        ui,
        egui_phosphor::regular::TEXT_OUTDENT,
        "Word wrap",
        "Fit text within the editor width by default.",
        app.word_wrap(),
        |enabled| app.set_word_wrap(enabled),
    );
}

fn render_font_family_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    combo_select_row(
        ui,
        "Family",
        Some("Bundled editor font."),
        "settings_editor_font",
        "combo.Font family",
        app.editor_font(),
        &EditorFontPreset::ALL,
        EditorFontPreset::label,
        EditorFontPreset::label,
        |font| app.set_editor_font(font),
    );
}

fn render_font_size_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(ui, "Size", Some("Editor text size."), |ui| {
        let current_index =
            nearest_option_index(app.font_size(), &FONT_SIZE_OPTIONS, |size| size as f32);
        let mut selected_index = current_index as u32;
        let selected_size = FONT_SIZE_OPTIONS[selected_index as usize];
        u32_slider_value_control(
            ui,
            "settings.font_size.slider",
            "slider.Font size",
            &mut selected_index,
            0..=(FONT_SIZE_OPTIONS.len() - 1) as u32,
            40.0,
            selected_size.to_string(),
        );

        let selected_size = FONT_SIZE_OPTIONS[selected_index as usize] as f32;
        if selected_index as usize != current_index {
            app.set_font_size(selected_size);
        }
    });
}

fn render_gutter_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(ui, "Gutter", Some("Editor padding."), |ui| {
        let mut selected_gutter = u32::from(app.editor_gutter());
        let gutter_label = format!("{selected_gutter} px");
        u32_slider_value_control(
            ui,
            "settings.gutter.slider",
            "slider.Gutter",
            &mut selected_gutter,
            0..=32,
            64.0,
            gutter_label,
        );

        if selected_gutter != u32::from(app.editor_gutter()) {
            app.set_editor_gutter(selected_gutter as u8);
        }
    });
}
