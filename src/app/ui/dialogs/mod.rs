mod common;
mod encoding;
mod pending;
mod restore_conflict;
mod transaction_log;

use crate::app::app_state::ScratchpadApp;
use eframe::egui;

pub(crate) fn show_pending_action_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    pending::show_pending_action_modal(ctx, app);
}

pub(crate) fn show_startup_restore_conflict_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    restore_conflict::show_startup_restore_conflict_modal(ctx, app);
}

pub(crate) fn show_transaction_log_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    transaction_log::show_transaction_log_window(ctx, app);
}

pub(crate) fn show_encoding_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    encoding::show_encoding_window(ctx, app);
}
