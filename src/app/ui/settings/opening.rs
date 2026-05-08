use super::*;
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
        app.recent_files_enabled(),
        |enabled| app.set_recent_files_enabled(enabled),
    );
}

fn render_opening_files_card(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut selected = app.file_open_disposition();
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

    if selected != app.file_open_disposition() {
        app.set_file_open_disposition(selected);
    }
}

fn render_startup_card(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut selected = app.startup_session_behavior();
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

    if selected != app.startup_session_behavior() {
        app.set_startup_session_behavior(selected);
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
