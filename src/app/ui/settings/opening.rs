use super::{
    ScratchpadApp, egui,
    widgets::{
        action_row, destructive_action_row, destructive_button, group_heading, segmented_row,
    },
};
use crate::app::{
    app_state::settings_controller as settings,
    services::settings_store::{FileOpenDisposition, StartupSessionBehavior},
};

pub(super) fn render_opening(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut selected = app.state.app_settings.file_open_disposition();
    if segmented_row(
        ui,
        "Open files in",
        &mut selected,
        &[
            (FileOpenDisposition::NewTab, "New tab"),
            (FileOpenDisposition::CurrentTab, "Current tab"),
        ],
    ) {
        settings::set_file_open_disposition(app, selected);
    }
}

pub(super) fn render_application(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut startup = app.state.app_settings.startup_session_behavior();
    if segmented_row(
        ui,
        "On launch",
        &mut startup,
        &[
            (
                StartupSessionBehavior::ContinuePreviousSession,
                "Continue session",
            ),
            (StartupSessionBehavior::StartFreshSession, "Start fresh"),
        ],
    ) {
        settings::set_startup_session_behavior(app, startup);
    }
    group_heading(ui, "Configuration");
    let path = crate::app::app_state::settings_state::settings_path(app)
        .display()
        .to_string();
    if action_row(
        ui,
        "Settings file",
        "Open file",
        Some("TOML · Loaded on startup"),
    ) {
        settings::open_settings_file_tab(app);
    }
    ui.label(
        egui::RichText::new(path)
            .size(12.5)
            .color(super::text_muted(ui)),
    );
    let confirm_id = egui::Id::unique("settings.confirm_reset");
    if destructive_action_row(
        ui,
        "Reset all settings",
        "Reset settings…",
        Some("Restore preferences without deleting documents."),
    ) {
        ui.data_mut(|data| data.insert_temp(confirm_id, true));
    }
}

/// Outside the category body so changing search or collapsing a group cannot
/// strand the confirmation state. Cancellation never touches persisted settings.
pub(super) fn reset_confirmation(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let id = egui::Id::unique("settings.confirm_reset");
    if !ctx
        .data_mut(|data| data.get_temp::<bool>(id))
        .unwrap_or(false)
    {
        return;
    }
    let mut close = false;
    let response = egui::Modal::new(id).show(ctx, |ui| {
        super::SettingsUi::apply_typography(ui);
        super::SettingsUi::apply_controls(ui);
        ui.set_max_width(360.0);
        ui.heading("Reset all settings?");
        ui.label("Restore default preferences. Documents and recovery data will not be deleted.");
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                close = true;
            }
            if destructive_button(ui, "Reset all settings").clicked() {
                settings::reset_settings_to_defaults(app);
                ctx.data_mut(|data| {
                    data.remove::<bool>(egui::Id::unique("settings.custom_palette"));
                });
                close = true;
            }
        });
    });
    if close || response.should_close() {
        ctx.data_mut(|data| data.insert_temp(id, false));
    }
}
