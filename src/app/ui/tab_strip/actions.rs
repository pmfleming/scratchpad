mod primary;
mod vertical;

use super::layout::HeaderLayout;
use crate::app::app_state::{ScratchpadApp, frame};
use crate::app::chrome::caption_controls;
use crate::app::platform;
use eframe::egui;

pub(crate) fn show_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) -> bool {
    if !show_file_search_primary_actions() {
        return false;
    }

    primary::show_primary_actions(ui, app);
    true
}

pub(crate) fn show_vertical_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) -> bool {
    vertical::show_vertical_primary_actions(ui, app)
}

pub(super) fn show_file_search_primary_actions() -> bool {
    !cfg!(target_os = "linux")
}

pub(crate) fn show_caption_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
) {
    if !platform::capabilities(app.state.app_settings.platform_profile())
        .show_window_caption_buttons
    {
        return;
    }

    if caption_controls(ui, ctx, layout.caption_controls_width) {
        frame::request_exit(app, ctx);
    }
}
