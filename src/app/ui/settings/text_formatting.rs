use super::*;

fn nearest_font_size_index(font_size: f32) -> usize {
    FONT_SIZE_OPTIONS
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            let left_distance = (font_size - **left as f32).abs();
            let right_distance = (font_size - **right as f32).abs();
            left_distance.total_cmp(&right_distance)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

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
        let current_index = nearest_font_size_index(app.font_size());
        let mut selected_index = current_index as u32;
        ui.add_sized(
            egui::vec2(SettingsUi::CONTROLS.width, 0.0),
            egui::Slider::new(
                &mut selected_index,
                0..=(FONT_SIZE_OPTIONS.len() - 1) as u32,
            )
            .step_by(1.0)
            .show_value(false),
        );
        ui.add_space(8.0);
        ui.label(FONT_SIZE_OPTIONS[selected_index as usize].to_string());

        let selected_size = FONT_SIZE_OPTIONS[selected_index as usize] as f32;
        if selected_index as usize != current_index {
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
            ui.add_sized(
                egui::vec2(SettingsUi::CONTROLS.width, 0.0),
                egui::Slider::new(&mut selected_gutter, EDITOR_GUTTER_RANGE.clone())
                    .step_by(1.0)
                    .show_value(false),
            );
            ui.add_space(8.0);
            ui.label(format!("{selected_gutter} px"));

            if selected_gutter != app.editor_gutter() {
                app.set_editor_gutter(selected_gutter);
            }
        },
    );
}
