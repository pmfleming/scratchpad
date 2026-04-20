mod button;
mod geometry;

use crate::app::app_state::ScratchpadApp;
use eframe::egui;

pub(crate) fn show_top_drag_bar(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    button::show_top_drag_bar(ui, app);
}
