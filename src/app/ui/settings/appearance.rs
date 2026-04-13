use super::*;

const AUTO_HIDE_DELAY_OPTIONS: [f32; 13] = [
    0.1, 0.3, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThemeModeSelection {
    System,
    Light,
    Dark,
    Custom,
}

impl ThemeModeSelection {
    fn label(self) -> &'static str {
        match self {
            Self::System => "Use system setting",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Custom => "Custom",
        }
    }
}

pub(super) fn render_appearance_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "Appearance");
    expandable_card(
        ui,
        "settings_appearance_card",
        egui_phosphor::regular::SUN,
        "Theme",
        "Choose the app mode and editor colors.",
        true,
        |ui| {
            render_theme_mode_row(ui, app);
            render_color_row(
                ui,
                "Text color",
                "Defaults follow the selected mode until you override it here.",
                app.editor_text_color(),
                |app, color| app.set_editor_text_color(color),
                app,
            );
            render_color_row(
                ui,
                "Background",
                "Defaults follow the selected mode until you override it here.",
                app.editor_background_color(),
                |app, color| app.set_editor_background_color(color),
                app,
            );
            ui.add_space(SettingsUi::LAYOUT.preview_top_margin);
            render_preview_panel(ui, app);
        },
    );
}

pub(super) fn render_tab_position_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "Tab Position");
    expandable_card(
        ui,
        "settings_tab_position_card",
        egui_phosphor::regular::TEXT_OUTDENT,
        "Tab list",
        "Choose where tabs live and how long auto-hidden lists stay visible.",
        true,
        |ui| {
            render_tab_list_row(ui, app);
            inner_divider(ui);
            render_auto_hide_row(ui, app);
            inner_divider(ui);
            render_auto_hide_delay_row(ui, app);
        },
    );
}

fn render_theme_mode_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(
        ui,
        "Mode",
        Some("Follow Windows, force a mode, or show Custom while colors are overridden."),
        |ui| {
            let has_custom_palette = app.has_custom_editor_palette();
            let initial_selection = selected_theme_mode(app);
            let mut selected_mode = initial_selection;
            egui::ComboBox::from_id_salt("settings_theme_mode")
                .selected_text(selected_mode.label())
                .width(SettingsUi::control_width(has_custom_palette))
                .show_ui(ui, |ui| {
                    for mode in [
                        ThemeModeSelection::System,
                        ThemeModeSelection::Light,
                        ThemeModeSelection::Dark,
                    ] {
                        ui.selectable_value(&mut selected_mode, mode, mode.label());
                    }
                    if has_custom_palette {
                        ui.selectable_value(
                            &mut selected_mode,
                            ThemeModeSelection::Custom,
                            ThemeModeSelection::Custom.label(),
                        );
                    }
                });
            if selected_mode != initial_selection {
                apply_theme_mode_selection(app, selected_mode, ui.ctx().system_theme());
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("Detected: {}", detected_system_theme_label(ui)))
                    .size(SettingsUi::TYPOGRAPHY.description)
                    .color(text_muted(ui)),
            );
        },
    );
    inner_divider(ui);
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
        let mut color = initial_color;
        if ui.color_edit_button_srgba(&mut color).changed() {
            on_change(app, color);
        }
    });
    inner_divider(ui);
}

fn render_tab_list_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(
        ui,
        "Tab list",
        Some("Use a horizontal strip or a vertical list on either side."),
        |ui| {
            let mut selected_position = app.tab_list_position();
            egui::ComboBox::from_id_salt("settings_tab_list_position")
                .selected_text(selected_position.label())
                .width(SettingsUi::CONTROLS.width)
                .show_ui(ui, |ui| {
                    for position in TabListPosition::ALL {
                        ui.selectable_value(&mut selected_position, position, position.label());
                    }
                });
            if selected_position != app.tab_list_position() {
                app.set_tab_list_position(selected_position);
            }
        },
    );
}

fn render_auto_hide_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(
        ui,
        "Auto-hide tab list",
        Some("Collapse the active tab list until the pointer nears it."),
        |ui| {
            let mut enabled = app.auto_hide_tab_list();
            toggle_control(ui, &mut enabled);
            if enabled != app.auto_hide_tab_list() {
                app.set_auto_hide_tab_list(enabled);
            }
        },
    );
}

fn render_auto_hide_delay_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(
        ui,
        "Auto-hide delay",
        Some("Keep the active tab list open for a short grace period after it loses context."),
        |ui| {
            let current_index =
                nearest_auto_hide_delay_index(app.tab_list_auto_hide_delay_seconds());
            let mut selected_index = current_index as u32;
            ui.add_sized(
                egui::vec2(SettingsUi::CONTROLS.width, 0.0),
                egui::Slider::new(
                    &mut selected_index,
                    0..=(AUTO_HIDE_DELAY_OPTIONS.len() - 1) as u32,
                )
                .step_by(1.0)
                .show_value(false),
            );
            ui.add_space(8.0);
            ui.label(auto_hide_delay_label(
                AUTO_HIDE_DELAY_OPTIONS[selected_index as usize],
            ));

            if selected_index as usize != current_index {
                app.set_tab_list_auto_hide_delay_seconds(
                    AUTO_HIDE_DELAY_OPTIONS[selected_index as usize],
                );
            }
        },
    );
}

fn nearest_auto_hide_delay_index(seconds: f32) -> usize {
    AUTO_HIDE_DELAY_OPTIONS
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            let left_distance = (seconds - **left).abs();
            let right_distance = (seconds - **right).abs();
            left_distance.total_cmp(&right_distance)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn auto_hide_delay_label(seconds: f32) -> String {
    if seconds.fract().abs() < f32::EPSILON {
        format!("{seconds:.0} s")
    } else {
        format!("{seconds:.1} s")
    }
}

fn detected_system_theme_label(ui: &egui::Ui) -> &'static str {
    match ui.ctx().system_theme() {
        Some(egui::Theme::Light) => "Light",
        Some(egui::Theme::Dark) => "Dark",
        None => "Unknown",
    }
}

fn selected_theme_mode(app: &ScratchpadApp) -> ThemeModeSelection {
    if app.has_custom_editor_palette() {
        ThemeModeSelection::Custom
    } else {
        match app.theme_mode() {
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
            app.apply_theme_mode_preset(AppThemeMode::System, system_theme)
        }
        ThemeModeSelection::Light => app.apply_theme_mode_preset(AppThemeMode::Light, system_theme),
        ThemeModeSelection::Dark => app.apply_theme_mode_preset(AppThemeMode::Dark, system_theme),
        ThemeModeSelection::Custom => {}
    }
}
