use super::activate_slot;
use super::menu_ui::{WIDTH as TAB_CONTEXT_MENU_WIDTH, menu_button};
use crate::app::app_state::{ScratchpadApp, StatusDomain, frame};
use crate::app::platform_file;
use eframe::egui;
use egui_phosphor::regular::{COPY, FOLDER_OPEN, TRANSLATE};
use std::path::Path;

pub(super) fn render_location_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    encoding_enabled: bool,
    copy_path_enabled: bool,
    reveal_enabled: bool,
    path: Option<&Path>,
) {
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Encoding",
        Some(TRANSLATE),
        encoding_enabled,
    ) {
        activate_slot(app, slot_index);
        frame::open_encoding_dialog(app);
        ui.close();
    }
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Copy Path",
        Some(COPY),
        copy_path_enabled,
    ) {
        if let Some(path) = path {
            ui.copy_text(path.display().to_string());
        }
        ui.close();
    }
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        platform_file::reveal_file_label(),
        Some(FOLDER_OPEN),
        reveal_enabled,
    ) {
        if let Some(path) = path
            && let Err(error) = platform_file::reveal_file(path)
        {
            app.state.status.set_warning_status_with_detail(
                StatusDomain::File,
                platform_file::reveal_file_error_message(),
                error.to_string(),
            );
        }
        ui.close();
    }
}
