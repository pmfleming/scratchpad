use super::{
    AppThemeMode, CategoryCard, ComboSelectRow, EditorAppearanceSource, EditorFontPreset,
    FONT_SIZE_OPTIONS, IndentationStyle, ScratchpadApp, SettingsUi, TabKeyBehavior,
    available_width_control, category_card, combo_select_row, egui, inner_divider,
    inner_select_row, nearest_option_index, record_settings_control_box, render_preview_panel,
    toggle_card, toggle_select_row, u32_slider_value_control,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThemeModeSelection {
    System,
    Light,
    Dark,
    Custom,
}

const THEME_MODE_OPTIONS: [ThemeModeSelection; 3] = [
    ThemeModeSelection::System,
    ThemeModeSelection::Light,
    ThemeModeSelection::Dark,
];
const THEME_MODE_OPTIONS_WITH_CUSTOM: [ThemeModeSelection; 4] = [
    ThemeModeSelection::System,
    ThemeModeSelection::Light,
    ThemeModeSelection::Dark,
    ThemeModeSelection::Custom,
];

impl ThemeModeSelection {
    fn label(self) -> &'static str {
        match self {
            Self::System => "Use system setting",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Custom => "Custom",
        }
    }

    fn pill_label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Custom => "Custom",
        }
    }
}

pub(super) fn render_text_formatting_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_card(
        ui,
        CategoryCard {
            heading: "Editor Appearance",
            id_source: "settings_editor_appearance_card",
            icon: egui_phosphor::regular::TEXT_ALIGN_JUSTIFY,
            title: "Text and colors",
            description: "Choose app-defined appearance or use OS settings.",
            default_open: true,
        },
        |ui| {
            render_appearance_source_row(ui, app);
            inner_divider(ui);
            if app.state.app_settings.uses_system_editor_appearance() {
                render_system_summary_row(ui, app);
            } else {
                render_app_appearance_rows(ui, app);
            }
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

fn render_appearance_source_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    combo_select_row(
        ui,
        ComboSelectRow {
            label: "Appearance",
            description: Some(
                "App keeps explicit settings; System follows OS font, size, and colors.",
            ),
            combo_id: "settings_editor_appearance_source",
            record_label: "combo.Appearance source",
            current: app.state.app_settings.editor_appearance_source(),
            options: &EditorAppearanceSource::ALL,
            selected_label: EditorAppearanceSource::label,
            option_label: EditorAppearanceSource::label,
            on_change: |source| {
                crate::app::app_state::settings_controller::set_editor_appearance_source(
                    app, source,
                );
            },
        },
    );
}

fn render_system_summary_row(ui: &mut egui::Ui, app: &ScratchpadApp) {
    inner_select_row(
        ui,
        "System values",
        Some("Font, size, text, background, and highlight are resolved from OS settings."),
        |ui| {
            available_width_control(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!(
                        "{} · {:.0} pt",
                        app.state.app_settings.editor_font_selection().label(),
                        app.state.app_settings.font_size()
                    ));
                });
            });
        },
    );
}

fn render_app_appearance_rows(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    render_scratchpad_font_family_row(ui, app);
    inner_divider(ui);
    render_font_size_row(ui, app);
    inner_divider(ui);
    render_theme_mode_row(ui, app);
    inner_divider(ui);
    render_color_row(
        ui,
        "Text color",
        "Overrides mode defaults.",
        app.state.app_settings.editor_text_color(),
        |app, color| {
            crate::app::app_state::settings_controller::set_editor_text_color(app, color);
        },
        app,
    );
    inner_divider(ui);
    render_color_row(
        ui,
        "Background",
        "Overrides mode defaults.",
        app.state.app_settings.editor_background_color(),
        |app, color| {
            crate::app::app_state::settings_controller::set_editor_background_color(app, color);
        },
        app,
    );
    inner_divider(ui);
    render_color_row(
        ui,
        "Highlight",
        "Search match color.",
        app.state.app_settings.editor_text_highlight_color(),
        |app, color| {
            crate::app::app_state::settings_controller::set_editor_text_highlight_color(app, color);
        },
        app,
    );
}

fn render_scratchpad_font_family_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
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
                crate::app::app_state::settings_controller::set_editor_font(app, font);
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
            "settings.editor.font_size.slider",
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

pub(super) fn render_editing_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_card(
        ui,
        CategoryCard {
            heading: "Editing",
            id_source: "settings_indentation_card",
            icon: egui_phosphor::regular::TEXT_OUTDENT,
            title: "Indentation",
            description: "Choose how Tab indents text and how tabs are displayed.",
            default_open: true,
        },
        |ui| {
            combo_select_row(
                ui,
                ComboSelectRow {
                    label: "Tab key",
                    description: Some("Indent text or move keyboard focus."),
                    combo_id: "settings_tab_key_behavior",
                    record_label: "combo.Tab key behavior",
                    current: app.state.app_settings.tab_key_behavior(),
                    options: &TabKeyBehavior::ALL,
                    selected_label: TabKeyBehavior::label,
                    option_label: TabKeyBehavior::label,
                    on_change: |behavior| {
                        crate::app::app_state::settings_controller::set_tab_key_behavior(
                            app, behavior,
                        );
                    },
                },
            );
            inner_divider(ui);
            combo_select_row(
                ui,
                ComboSelectRow {
                    label: "Indent using",
                    description: Some("A run of spaces or an actual tab character."),
                    combo_id: "settings_indentation_style",
                    record_label: "combo.Indentation style",
                    current: app.state.app_settings.indentation_style(),
                    options: &IndentationStyle::ALL,
                    selected_label: IndentationStyle::label,
                    option_label: IndentationStyle::label,
                    on_change: |style| {
                        crate::app::app_state::settings_controller::set_indentation_style(
                            app, style,
                        );
                    },
                },
            );
            inner_divider(ui);
            render_tab_width_row(ui, app);
            inner_divider(ui);
            toggle_select_row(
                ui,
                "Show tab characters",
                Some("Draw a muted arrow across each tab without changing the text."),
                "settings.show_tab_characters",
                app.state.app_settings.show_tab_characters(),
                |visible| {
                    crate::app::app_state::settings_controller::set_show_tab_characters(
                        app, visible,
                    );
                },
            );
        },
    );
}

fn render_tab_width_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(
        ui,
        "Indent width",
        Some("Columns per indentation level."),
        |ui| {
            let mut selected_width = u32::from(app.state.app_settings.editor_tab_width());
            let width_label = format!("{selected_width} columns");
            u32_slider_value_control(
                ui,
                "settings.editor.tab_width.slider",
                "slider.Indent width",
                &mut selected_width,
                1..=16,
                88.0,
                width_label,
            );

            if selected_width != u32::from(app.state.app_settings.editor_tab_width()) {
                crate::app::app_state::settings_controller::set_editor_tab_width(
                    app,
                    selected_width as u8,
                );
            }
        },
    );
}

fn render_theme_mode_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let description = format!(
        "App color preset. Detected system mode: {}.",
        detected_system_theme_label(ui)
    );
    let options = if app.state.app_settings.has_custom_editor_palette() {
        &THEME_MODE_OPTIONS_WITH_CUSTOM[..]
    } else {
        &THEME_MODE_OPTIONS[..]
    };
    let system_theme = ui.ctx().system_theme();
    combo_select_row(
        ui,
        ComboSelectRow {
            label: "Color mode",
            description: Some(&description),
            combo_id: "settings_theme_mode",
            record_label: "combo.Theme mode",
            current: selected_theme_mode(app),
            options,
            selected_label: ThemeModeSelection::pill_label,
            option_label: ThemeModeSelection::label,
            on_change: |mode| apply_theme_mode_selection(app, mode, system_theme),
        },
    );
}

fn render_color_row(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    initial_color: egui::Color32,
    on_change: impl Fn(&mut ScratchpadApp, egui::Color32),
    app: &mut ScratchpadApp,
) {
    inner_select_row(ui, label, Some(description), |ui| {
        available_width_control(ui, |ui| {
            let mut color = initial_color;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let response = ui.color_edit_button_srgba(&mut color);
                record_settings_control_box(format!("color.{label}"), response.rect);
                if response.changed() {
                    on_change(app, color);
                }
            });
        });
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

fn detected_system_theme_label(ui: &egui::Ui) -> &'static str {
    match ui.ctx().system_theme() {
        Some(egui::Theme::Light) => "Light",
        Some(egui::Theme::Dark) => "Dark",
        None => "Unknown",
    }
}

fn selected_theme_mode(app: &ScratchpadApp) -> ThemeModeSelection {
    if app.state.app_settings.has_custom_editor_palette() {
        ThemeModeSelection::Custom
    } else {
        match app.state.app_settings.theme_mode() {
            AppThemeMode::System => ThemeModeSelection::System,
            AppThemeMode::Light => ThemeModeSelection::Light,
            AppThemeMode::Dark => ThemeModeSelection::Dark,
        }
    }
}

fn apply_theme_mode_selection(
    app: &mut ScratchpadApp,
    selection: ThemeModeSelection,
    system_theme: Option<egui::Theme>,
) {
    match selection {
        ThemeModeSelection::System => {
            crate::app::app_state::settings_controller::apply_theme_mode_preset(
                app,
                AppThemeMode::System,
                system_theme,
            );
        }
        ThemeModeSelection::Light => {
            crate::app::app_state::settings_controller::apply_theme_mode_preset(
                app,
                AppThemeMode::Light,
                system_theme,
            );
        }
        ThemeModeSelection::Dark => {
            crate::app::app_state::settings_controller::apply_theme_mode_preset(
                app,
                AppThemeMode::Dark,
                system_theme,
            );
        }
        ThemeModeSelection::Custom => {}
    }
}
