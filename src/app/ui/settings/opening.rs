use super::*;
use crate::app::services::settings_store::{FileOpenDisposition, StartupSessionBehavior};

const FILE_OPEN_OPTIONS: [FileOpenDisposition; 2] = [
    FileOpenDisposition::NewTab,
    FileOpenDisposition::CurrentTab,
];
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
        "Show recent files features when that history UI is available.",
        app.recent_files_enabled(),
        |enabled| app.set_recent_files_enabled(enabled),
    );
}

fn render_opening_files_card(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut selected = app.file_open_disposition();
    toggleless_select_card(
        ui,
        egui_phosphor::regular::ARROW_SQUARE_OUT,
        "Opening files",
        "Choose where files are opened.",
        |ui| {
            fixed_width_control(ui, |ui| {
                egui::ComboBox::from_id_salt("settings_opening_files")
                    .selected_text(file_open_pill_label(selected))
                    .width(SettingsUi::CONTROLS.width)
                    .show_ui(ui, |ui| {
                        for option in FILE_OPEN_OPTIONS {
                            ui.selectable_value(&mut selected, option, file_open_label(option));
                        }
                    });
            });
        },
    );

    if selected != app.file_open_disposition() {
        app.set_file_open_disposition(selected);
    }
}

fn render_startup_card(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let id = ui.make_persistent_id("settings_startup_behavior_card");
    let is_open = ui
        .data_mut(|data| data.get_persisted::<bool>(id))
        .unwrap_or(true);
    let mut selected = app.startup_session_behavior();

    settings_card_frame(ui, |ui| {
        let response = clickable_card_header(
            ui,
            id,
            egui_phosphor::regular::TRAY,
            "When Scratchpad starts",
            Some("Choose how the app restores your workspace."),
            |ui| {
                let chevron = if is_open {
                    egui_phosphor::regular::CARET_UP
                } else {
                    egui_phosphor::regular::CARET_DOWN
                };
                ui.label(
                    egui::RichText::new(chevron)
                        .size(18.0)
                        .color(SettingsUi::icon_color(ui)),
                );
            },
        );

        if response.clicked() {
            ui.data_mut(|data| data.insert_persisted(id, !is_open));
        }

        if is_open {
            inner_divider(ui);
            ui.add_space(10.0);
            ui.vertical(|ui| {
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
            });
        }
    });

    if selected != app.startup_session_behavior() {
        app.set_startup_session_behavior(selected);
    }
}

fn toggleless_select_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    add_trailing: impl FnOnce(&mut egui::Ui),
) {
    settings_card_frame(ui, |ui| {
        card_header(ui, icon, title, Some(description), add_trailing);
    });
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
        StartupSessionBehavior::StartFreshSession => {
            "Start new session and discard unsaved changes"
        }
    }
}
