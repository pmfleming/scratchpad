use super::activate_slot;
use super::menu_ui::{WIDTH as TAB_CONTEXT_MENU_WIDTH, menu_button};
use crate::app::app_state::{ScratchpadApp, StatusDomain, frame};
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
        "Reveal In Explorer",
        Some(FOLDER_OPEN),
        reveal_enabled,
    ) {
        if let Some(path) = path
            && let Err(error) = reveal_in_explorer(path)
        {
            app.state.status.set_warning_status_with_detail(
                StatusDomain::File,
                "Could not reveal this file in Explorer.",
                error.to_string(),
            );
        }
        ui.close();
    }
}

#[cfg(target_os = "windows")]
fn reveal_in_explorer(path: &Path) -> std::io::Result<()> {
    use std::ffi::OsString;
    use std::process::Command;

    let mut select_arg = OsString::from("/select,");
    select_arg.push(path);
    Command::new("explorer.exe")
        .arg(select_arg)
        .spawn()
        .map(|_| ())
}

#[cfg(not(target_os = "windows"))]
fn reveal_in_explorer(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Reveal in Explorer is only available on Windows.",
    ))
}
