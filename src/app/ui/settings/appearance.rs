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
        "Appearance",
        "settings_appearance_card",
        egui_phosphor::regular::SUN,
        "Theme",
        "Mode and editor colors.",
        true,
        |ui| {
            render_theme_mode_row(ui, app);
            render_color_row(
                ui,
                "Text color",
                "Overrides mode defaults.",
                app.editor_text_color(),
                |app, color| app.set_editor_text_color(color),
                app,
            );
            render_color_row(
                ui,
                "Background",
                "Overrides mode defaults.",
                app.editor_background_color(),
                |app, color| app.set_editor_background_color(color),
                app,
            );
            render_color_row(
                ui,
                "Highlight",
                "Search match color.",
                app.editor_text_highlight_color(),
                |app, color| app.set_editor_text_highlight_color(color),
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
        "Tab Position",
        "settings_tab_position_card",
        egui_phosphor::regular::TEXT_OUTDENT,
        "Tab list",
        "Tab placement and visibility.",
        true,
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
    let options = if app.has_custom_editor_palette() {
        &THEME_MODE_OPTIONS_WITH_CUSTOM[..]
    } else {
        &THEME_MODE_OPTIONS[..]
    };
    let system_theme = ui.ctx().system_theme();
    combo_select_row(
        ui,
        "Mode",
        Some(&description),
        "settings_theme_mode",
        "combo.Theme mode",
        selected_theme_mode(app),
        options,
        ThemeModeSelection::pill_label,
        ThemeModeSelection::label,
        |mode| apply_theme_mode_selection(app, mode, system_theme),
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
        let response = ui.color_edit_button_srgba(&mut color);
        record_settings_control_box(format!("color.{label}"), response.rect);
        if response.changed() {
            on_change(app, color);
        }
    });
    inner_divider(ui);
}

fn render_tab_list_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    combo_select_row(
        ui,
        "Tab list",
        Some("Strip or side list."),
        "settings_tab_list_position",
        "combo.Tab list",
        app.tab_list_position(),
        &TAB_LIST_POSITIONS,
        tab_list_position_label,
        tab_list_position_label,
        |position| app.set_tab_list_position(position),
    );
}

fn render_new_tab_placement_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    combo_select_row(
        ui,
        "New tabs",
        Some("Placement for new tabs."),
        "settings_new_tab_placement",
        "combo.New tabs",
        app.new_tab_placement(),
        &NEW_TAB_PLACEMENT_OPTIONS,
        new_tab_placement_pill_label,
        new_tab_placement_label,
        |placement| app.set_new_tab_placement(placement),
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
        app.auto_hide_tab_list(),
        |enabled| app.set_auto_hide_tab_list(enabled),
    );
}

fn render_auto_hide_delay_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(
        ui,
        "Auto-hide delay",
        Some("Grace period before collapse."),
        |ui| {
            let current_index = nearest_option_index(
                app.tab_list_auto_hide_delay_seconds(),
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
                app.set_tab_list_auto_hide_delay_seconds(
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
        app.status_bar_visible(),
        |visible| app.defer_status_bar_visible(visible, &ctx),
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
