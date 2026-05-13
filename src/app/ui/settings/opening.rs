use super::{
    ScratchpadApp, SettingsUi, card_header, category_heading, combo_control, egui, expandable_card,
    radio_option_row, settings_card_frame, toggle_card,
};
use crate::app::services::settings_store::{FileOpenDisposition, StartupSessionBehavior};

const FILE_OPEN_OPTIONS: [FileOpenDisposition; 2] =
    [FileOpenDisposition::NewTab, FileOpenDisposition::CurrentTab];
const STARTUP_SESSION_OPTIONS: [StartupSessionBehavior; 2] = [
    StartupSessionBehavior::ContinuePreviousSession,
    StartupSessionBehavior::StartFreshSession,
];

pub(super) fn render_opening_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "Opening");
    render_opening_files_card(ui, app);
    ui.add_space(SettingsUi::LAYOUT.card_gap);
    render_startup_card(ui, app);
    ui.add_space(SettingsUi::LAYOUT.card_gap);
    toggle_card(
        ui,
        egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
        "Recent files",
        "Enable recent-file UI.",
        app.state.app_settings.recent_files_enabled(),
        |enabled| {
            crate::app::app_state::settings_controller::set_recent_files_enabled(app, enabled)
        },
    );
}

fn render_opening_files_card(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut selected = app.state.app_settings.file_open_disposition();
    settings_card_frame(ui, |ui| {
        card_header(
            ui,
            egui_phosphor::regular::ARROW_SQUARE_OUT,
            "Opening files",
            Some("Where files open."),
            |ui| {
                combo_control(
                    ui,
                    "settings_opening_files",
                    "combo.Opening files",
                    &mut selected,
                    &FILE_OPEN_OPTIONS,
                    file_open_pill_label,
                    file_open_label,
                );
            },
        );
    });

    if selected != app.state.app_settings.file_open_disposition() {
        crate::app::app_state::settings_controller::set_file_open_disposition(app, selected);
    }
}

fn render_startup_card(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut selected = app.state.app_settings.startup_session_behavior();
    expandable_card(
        ui,
        "settings_startup_behavior_card",
        egui_phosphor::regular::TRAY,
        "When Scratchpad starts",
        "Startup restore behavior.",
        true,
        |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(40.0);
                ui.vertical(|ui| {
                    for option in STARTUP_SESSION_OPTIONS {
                        let mut checked = selected == option;
                        let response =
                            radio_option_row(ui, &mut checked, startup_session_label(option));
                        if response.clicked() {
                            selected = option;
                        }
                        ui.add_space(8.0);
                    }
                });
            });
        },
    );

    if selected != app.state.app_settings.startup_session_behavior() {
        crate::app::app_state::settings_controller::set_startup_session_behavior(app, selected);
    }
}

fn file_open_label(option: FileOpenDisposition) -> &'static str {
    match option {
        FileOpenDisposition::NewTab => "Open in a new tab",
        FileOpenDisposition::CurrentTab => "Open in the current tab",
    }
}

fn file_open_pill_label(option: FileOpenDisposition) -> &'static str {
    match option {
        FileOpenDisposition::NewTab => "New tab",
        FileOpenDisposition::CurrentTab => "Current tab",
    }
}

fn startup_session_label(option: StartupSessionBehavior) -> &'static str {
    match option {
        StartupSessionBehavior::ContinuePreviousSession => "Continue previous session",
        StartupSessionBehavior::StartFreshSession => "Start fresh",
    }
}
