use super::*;

pub(super) fn render_text_formatting_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "Text Formatting");
    expandable_card(
        ui,
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
    inner_select_row(ui, "Family", Some("Pick the bundled editor font."), |ui| {
        let mut selected_font = app.editor_font();
        egui::ComboBox::from_id_salt("settings_editor_font")
            .selected_text(selected_font.label())
            .width(SettingsUi::CONTROLS.width)
            .show_ui(ui, |ui| {
                for preset in EditorFontPreset::ALL {
                    ui.selectable_value(&mut selected_font, preset, preset.label());
                }
            });
        if selected_font != app.editor_font() {
            app.set_editor_font(selected_font);
        }
    });
}

fn render_font_size_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(ui, "Size", Some("Adjust the editor text size."), |ui| {
        let mut selected_size = app.font_size().round() as u32;
        egui::ComboBox::from_id_salt("settings_font_size")
            .selected_text(selected_size.to_string())
            .width(SettingsUi::CONTROLS.width)
            .show_ui(ui, |ui| {
                for option in FONT_SIZE_OPTIONS {
                    ui.selectable_value(&mut selected_size, option, option.to_string());
                }
            });

        let selected_size = selected_size as f32;
        if (selected_size - app.font_size()).abs() > f32::EPSILON {
            app.set_font_size(selected_size);
        }
    });
}

fn render_gutter_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(
        ui,
        "Gutter",
        Some("Add space around the editor text area."),
        |ui| {
            let mut selected_gutter = app.editor_gutter();
            ui.add(
                egui::DragValue::new(&mut selected_gutter)
                    .range(EDITOR_GUTTER_RANGE)
                    .speed(0.25)
                    .suffix(" px"),
            );

            if selected_gutter != app.editor_gutter() {
                app.set_editor_gutter(selected_gutter);
            }
        },
    );
}
