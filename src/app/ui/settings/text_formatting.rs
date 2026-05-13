use super::{
    CategoryCard, ComboSelectRow, EditorFontPreset, FONT_SIZE_OPTIONS, ScratchpadApp, SettingsUi,
    category_card, combo_select_row, egui, inner_divider, inner_select_row, nearest_option_index,
    render_preview_panel, toggle_card, u32_slider_value_control,
};

pub(super) fn render_text_formatting_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_card(
        ui,
        CategoryCard {
            heading: "Text Formatting",
            id_source: "settings_font_card",
            icon: egui_phosphor::regular::TEXT_ALIGN_JUSTIFY,
            title: "Font",
            description: "Choose the text appearance for editor content.",
            default_open: true,
        },
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
        app.state.app_settings.word_wrap(),
        |enabled| crate::app::app_state::settings_controller::set_word_wrap(app, enabled),
    );
}

fn render_font_family_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    combo_select_row(
        ui,
        ComboSelectRow {
            label: "Family",
            description: Some("Bundled editor font."),
            combo_id: "settings_editor_font",
            record_label: "combo.Font family",
            current: app.state.app_settings.editor_font(),
            options: &EditorFontPreset::ALL,
            selected_label: EditorFontPreset::label,
            option_label: EditorFontPreset::label,
            on_change: |font| {
                crate::app::app_state::settings_controller::set_editor_font(app, font)
            },
        },
    );
}

fn render_font_size_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(ui, "Size", Some("Editor text size."), |ui| {
        let current_index = nearest_option_index(
            app.state.app_settings.font_size(),
            &FONT_SIZE_OPTIONS,
            |size| size as f32,
        );
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
            crate::app::app_state::settings_controller::set_font_size(app, selected_size);
        }
    });
}

fn render_gutter_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(ui, "Gutter", Some("Editor padding."), |ui| {
        let mut selected_gutter = u32::from(app.state.app_settings.editor_gutter());
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

        if selected_gutter != u32::from(app.state.app_settings.editor_gutter()) {
            crate::app::app_state::settings_controller::set_editor_gutter(
                app,
                selected_gutter as u8,
            );
        }
    });
}
