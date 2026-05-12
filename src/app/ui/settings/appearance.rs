use super::*;
use crate::app::services::settings_store::NewTabPlacement;

const AUTO_HIDE_DELAY_OPTIONS: [f32; 13] = [
    0.1, 0.3, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
];
const TAB_LIST_POSITIONS: [TabListPosition; 4] = [
    TabListPosition::Top,
    TabListPosition::Bottom,
    TabListPosition::Left,
    TabListPosition::Right,
];
const NEW_TAB_PLACEMENT_OPTIONS: [NewTabPlacement; 4] = [
    NewTabPlacement::Start,
    NewTabPlacement::End,
    NewTabPlacement::AfterSelection,
    NewTabPlacement::BeforeSelection,
];

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

pub(super) fn render_appearance_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_card(
        ui,
        CategoryCard {
            heading: "Appearance",
            id_source: "settings_appearance_card",
            icon: egui_phosphor::regular::SUN,
            title: "Theme",
            description: "Mode and editor colors.",
            default_open: true,
        },
        |ui| {
            render_theme_mode_row(ui, app);
            render_color_row(
                ui,
                "Text color",
                "Overrides mode defaults.",
                app.state.app_settings.editor_text_color(),
                |app, color| {
                    crate::app::app_state::settings_controller::set_editor_text_color(app, color)
                },
                app,
            );
            render_color_row(
                ui,
                "Background",
                "Overrides mode defaults.",
                app.state.app_settings.editor_background_color(),
                |app, color| {
                    crate::app::app_state::settings_controller::set_editor_background_color(
                        app, color,
                    )
                },
                app,
            );
            render_color_row(
                ui,
                "Highlight",
                "Search match color.",
                app.state.app_settings.editor_text_highlight_color(),
                |app, color| {
                    crate::app::app_state::settings_controller::set_editor_text_highlight_color(
                        app, color,
                    )
                },
                app,
            );
            ui.add_space(SettingsUi::LAYOUT.preview_top_margin);
            render_preview_panel(ui, app);
        },
    );
}

pub(super) fn render_tab_position_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_card(
        ui,
        CategoryCard {
            heading: "Tab Position",
            id_source: "settings_tab_position_card",
            icon: egui_phosphor::regular::TEXT_OUTDENT,
            title: "Tab list",
            description: "Tab placement and visibility.",
            default_open: true,
        },
        |ui| {
            render_tab_list_row(ui, app);
            inner_divider(ui);
            render_new_tab_placement_row(ui, app);
            inner_divider(ui);
            render_auto_hide_row(ui, app);
            inner_divider(ui);
            render_auto_hide_delay_row(ui, app);
            inner_divider(ui);
            render_status_bar_row(ui, app);
        },
    );
}

fn render_theme_mode_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let description = format!("App mode. Detected: {}.", detected_system_theme_label(ui));
    let options = if app.state.app_settings.has_custom_editor_palette() {
        &THEME_MODE_OPTIONS_WITH_CUSTOM[..]
    } else {
        &THEME_MODE_OPTIONS[..]
    };
    let system_theme = ui.ctx().system_theme();
    combo_select_row(
        ui,
        ComboSelectRow {
            label: "Mode",
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
    inner_divider(ui);
}

fn render_tab_list_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    combo_select_row(
        ui,
        ComboSelectRow {
            label: "Tab list",
            description: Some("Strip or side list."),
            combo_id: "settings_tab_list_position",
            record_label: "combo.Tab list",
            current: app.state.app_settings.tab_list_position(),
            options: &TAB_LIST_POSITIONS,
            selected_label: tab_list_position_label,
            option_label: tab_list_position_label,
            on_change: |position| {
                crate::app::app_state::settings_controller::set_tab_list_position(app, position)
            },
        },
    );
}

fn render_new_tab_placement_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    combo_select_row(
        ui,
        ComboSelectRow {
            label: "New tabs",
            description: Some("Placement for new tabs."),
            combo_id: "settings_new_tab_placement",
            record_label: "combo.New tabs",
            current: app.state.app_settings.new_tab_placement(),
            options: &NEW_TAB_PLACEMENT_OPTIONS,
            selected_label: new_tab_placement_pill_label,
            option_label: new_tab_placement_label,
            on_change: |placement| {
                crate::app::app_state::settings_controller::set_new_tab_placement(app, placement)
            },
        },
    );
}

fn tab_list_position_label(position: TabListPosition) -> &'static str {
    match position {
        TabListPosition::Top => "Top",
        TabListPosition::Bottom => "Bottom",
        TabListPosition::Left => "Left",
        TabListPosition::Right => "Right",
    }
}

fn new_tab_placement_label(placement: NewTabPlacement) -> &'static str {
    match placement {
        NewTabPlacement::Start => "Start of list",
        NewTabPlacement::End => "End of list",
        NewTabPlacement::BeforeSelection => "Before selection",
        NewTabPlacement::AfterSelection => "After selection",
    }
}

fn new_tab_placement_pill_label(placement: NewTabPlacement) -> &'static str {
    match placement {
        NewTabPlacement::Start => "Start",
        NewTabPlacement::End => "End",
        NewTabPlacement::BeforeSelection => "Before",
        NewTabPlacement::AfterSelection => "After",
    }
}

fn render_auto_hide_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    toggle_select_row(
        ui,
        "Auto-hide tab list",
        Some("Collapse until pointer is near."),
        "settings.auto_hide_tab_list",
        app.state.app_settings.auto_hide_tab_list(),
        |enabled| crate::app::app_state::settings_controller::set_auto_hide_tab_list(app, enabled),
    );
}

fn render_auto_hide_delay_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(
        ui,
        "Auto-hide delay",
        Some("Grace period before collapse."),
        |ui| {
            let current_index = nearest_option_index(
                app.state.app_settings.tab_list_auto_hide_delay_seconds(),
                &AUTO_HIDE_DELAY_OPTIONS,
                |seconds| seconds,
            );
            let mut selected_index = current_index as u32;
            let delay_label =
                auto_hide_delay_label(AUTO_HIDE_DELAY_OPTIONS[selected_index as usize]);
            u32_slider_value_control(
                ui,
                "settings.auto_hide_delay.slider",
                "slider.Auto-hide delay",
                &mut selected_index,
                0..=(AUTO_HIDE_DELAY_OPTIONS.len() - 1) as u32,
                52.0,
                delay_label,
            );

            if selected_index as usize != current_index {
                crate::app::app_state::settings_controller::set_tab_list_auto_hide_delay_seconds(
                    app,
                    AUTO_HIDE_DELAY_OPTIONS[selected_index as usize],
                );
            }
        },
    );
}

fn render_status_bar_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let ctx = ui.ctx().clone();
    toggle_select_row(
        ui,
        "Status bar",
        Some("Show the bottom status strip."),
        "settings.status_bar_visible",
        app.state.app_settings.status_bar_visible(),
        |visible| {
            crate::app::app_state::settings_controller::defer_status_bar_visible(app, visible, &ctx)
        },
    );
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
            )
        }
        ThemeModeSelection::Light => {
            crate::app::app_state::settings_controller::apply_theme_mode_preset(
                app,
                AppThemeMode::Light,
                system_theme,
            )
        }
        ThemeModeSelection::Dark => {
            crate::app::app_state::settings_controller::apply_theme_mode_preset(
                app,
                AppThemeMode::Dark,
                system_theme,
            )
        }
        ThemeModeSelection::Custom => {}
    }
}
