mod primary;
mod vertical;

use super::layout::HeaderLayout;
use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::caption_controls;
use eframe::egui;

pub(crate) fn show_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    primary::show_primary_actions(ui, app)
}

pub(crate) fn show_vertical_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    vertical::show_vertical_primary_actions(ui, app)
}

pub(crate) fn show_caption_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
) {
    if caption_controls(ui, ctx, layout.caption_controls_width) {
        app.request_exit(ctx);
    }
}
